use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{open, prepare_content, Entry};

/// 一次配置保存的完整写入计划。规划阶段只读，不修改文件。
#[derive(Debug, Clone, Default)]
pub struct ConfigPlan {
    files: BTreeMap<PathBuf, PlannedFile>,
}

#[derive(Debug, Clone, Default)]
struct PlannedFile {
    sets: Vec<Entry>,
    removes: Vec<String>,
    replace_text: Option<String>,
}

impl ConfigPlan {
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置一个键；同一计划内重复设置同一键时以后一次为准。
    pub fn set(
        &mut self,
        path: impl Into<PathBuf>,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), String> {
        let file = self.file_mut(path.into())?;
        let key = key.into();
        file.sets.retain(|entry| entry.key != key);
        file.sets.push(Entry::new(key, value));
        Ok(())
    }

    /// 删除一个键；计划内不允许同时设置和删除同一个键。
    pub fn remove(
        &mut self,
        path: impl Into<PathBuf>,
        key: impl Into<String>,
    ) -> Result<(), String> {
        let file = self.file_mut(path.into())?;
        let key = key.into();
        if file.sets.iter().any(|entry| entry.key == key) {
            return Err(format!("配置计划冲突：键 {key} 同时被设置和删除"));
        }
        if !file.removes.contains(&key) {
            file.removes.push(key);
        }
        Ok(())
    }

    /// 替换一份不含键值语义的文本文件（如 Hadoop workers）。
    pub fn replace_text(
        &mut self,
        path: impl Into<PathBuf>,
        content: impl Into<String>,
    ) -> Result<(), String> {
        let file = self.file_mut(path.into())?;
        if !file.sets.is_empty() || !file.removes.is_empty() || file.replace_text.is_some() {
            return Err("配置计划冲突：文本替换不能与键值修改混用".to_string());
        }
        file.replace_text = Some(content.into());
        Ok(())
    }

    pub fn merge(&mut self, other: Self) -> Result<(), String> {
        for (path, file) in other.files {
            let target = self.file_mut(path)?;
            if file.replace_text.is_some() {
                if !target.sets.is_empty()
                    || !target.removes.is_empty()
                    || target.replace_text.is_some()
                {
                    return Err("配置计划冲突：文本替换不能与键值修改混用".to_string());
                }
                target.replace_text = file.replace_text;
                continue;
            }
            if target.replace_text.is_some() {
                return Err("配置计划冲突：文本替换不能与键值修改混用".to_string());
            }
            for entry in file.sets {
                if target.removes.contains(&entry.key) {
                    return Err(format!("配置计划冲突：键 {} 同时被设置和删除", entry.key));
                }
                target.sets.retain(|current| current.key != entry.key);
                target.sets.push(entry);
            }
            for key in file.removes {
                if target.sets.iter().any(|entry| entry.key == key) {
                    return Err(format!("配置计划冲突：键 {key} 同时被设置和删除"));
                }
                if !target.removes.contains(&key) {
                    target.removes.push(key);
                }
            }
        }
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn file_mut(&mut self, path: PathBuf) -> Result<&mut PlannedFile, String> {
        let file = self.files.entry(path).or_default();
        if file.replace_text.is_some() {
            return Err("配置计划冲突：文本替换不能与键值修改混用".to_string());
        }
        Ok(file)
    }
}

struct StagedFile {
    path: PathBuf,
    temp: PathBuf,
    original: Option<Vec<u8>>,
}

/// 统一事务入口：先完成全部渲染和暂存，再逐文件替换；提交失败时恢复原文件。
pub fn apply_plan(plan: &ConfigPlan) -> Result<(), String> {
    apply_plan_with(plan, |temp, target| std::fs::rename(temp, target))
}

fn apply_plan_with<F>(plan: &ConfigPlan, mut commit_file: F) -> Result<(), String>
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    let mut staged = Vec::new();

    for (path, file) in &plan.files {
        match stage_file(path, file) {
            Ok(file) => staged.push(file),
            Err(e) => {
                cleanup_temps(&staged);
                return Err(e);
            }
        }
    }

    let mut committed = Vec::new();
    for (index, file) in staged.iter().enumerate() {
        if let Err(e) = commit_file(&file.temp, &file.path) {
            let restore_errors = restore_files(&staged, &committed);
            cleanup_temps(&staged[index..]);
            let mut message = format!("提交配置文件 {} 失败: {e}", file.path.display());
            if !restore_errors.is_empty() {
                message.push_str("\n回滚失败:\n");
                message.push_str(&restore_errors.join("\n"));
            }
            return Err(message);
        }
        committed.push(index);
    }
    Ok(())
}

fn stage_file(path: &Path, file: &PlannedFile) -> Result<StagedFile, String> {
    let content = if let Some(text) = &file.replace_text {
        text.clone()
    } else {
        let config = open(path.to_path_buf())?;
        let removals: Vec<&str> = file.removes.iter().map(String::as_str).collect();
        let body = config.render_entries(&file.sets, &removals)?;
        prepare_content(config.comment_style(), &body)
    };

    let original = read_original(path)?;
    let temp = temp_path(path);
    std::fs::write(&temp, content)
        .map_err(|e| format!("暂存配置文件 {} 失败: {e}", temp.display()))?;
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&temp, meta.permissions());
    }
    Ok(StagedFile {
        path: path.to_path_buf(),
        temp,
        original,
    })
}

fn read_original(path: &Path) -> Result<Option<Vec<u8>>, String> {
    if !path.exists() {
        return Ok(None);
    }
    std::fs::read(path)
        .map(Some)
        .map_err(|e| format!("读取原配置 {} 失败: {e}", path.display()))
}

fn restore_files(staged: &[StagedFile], committed: &[usize]) -> Vec<String> {
    let mut errors = Vec::new();
    for index in committed.iter().rev() {
        let file = &staged[*index];
        match &file.original {
            Some(content) => {
                if let Err(e) = std::fs::write(&file.path, content) {
                    errors.push(format!("恢复 {} 失败: {e}", file.path.display()));
                }
            }
            None => {
                if let Err(e) = std::fs::remove_file(&file.path) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        errors.push(format!("删除 {} 失败: {e}", file.path.display()));
                    }
                }
            }
        }
    }
    errors
}

fn cleanup_temps(staged: &[StagedFile]) {
    for file in staged {
        let _ = std::fs::remove_file(&file.temp);
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config");
    path.with_file_name(format!(".{name}.solostack.batch.tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HEADER_BEGIN, HEADER_END};

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("solostack-transaction-{name}"))
    }

    #[test]
    fn applies_multiple_changes_to_one_file_with_one_header() {
        let path = tmp_path("one-file.properties");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, "# keep\na=1\nb=2\n").unwrap();

        let mut plan = ConfigPlan::new();
        plan.set(&path, "a", "3").unwrap();
        plan.set(&path, "b", "4").unwrap();
        plan.set(&path, "c", "5").unwrap();
        let mut commits = 0;
        apply_plan_with(&plan, |temp, target| {
            commits += 1;
            std::fs::rename(temp, target)
        })
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(commits, 1, "同一文件的多处修改只能提交一次");
        assert!(content.contains("# keep"));
        assert!(content.contains("a=3"));
        assert!(content.contains("b=4"));
        assert!(content.contains("c=5"));
        assert_eq!(content.matches(HEADER_BEGIN).count(), 1);
        assert_eq!(content.matches(HEADER_END).count(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn commit_failure_restores_already_committed_files() {
        let first = tmp_path("rollback-a.properties");
        let second = tmp_path("rollback-b.properties");
        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_file(&second);
        std::fs::write(&first, "a=old\n").unwrap();
        std::fs::write(&second, "b=old\n").unwrap();

        let mut plan = ConfigPlan::new();
        plan.set(&first, "a", "new").unwrap();
        plan.set(&second, "b", "new").unwrap();

        let mut commits = 0;
        let err = apply_plan_with(&plan, |temp, target| {
            commits += 1;
            if commits == 2 {
                return Err(std::io::Error::other("injected failure"));
            }
            std::fs::rename(temp, target)
        })
        .unwrap_err();
        assert!(err.contains("injected failure"), "{err}");
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "a=old\n");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "b=old\n");

        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_file(&second);
    }

    #[test]
    fn staging_failure_leaves_all_originals_and_no_temp_files() {
        let first = tmp_path("stage-a.properties");
        let second = tmp_path("stage-b.unknown");
        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_file(&second);
        std::fs::write(&first, "a=old\n").unwrap();
        std::fs::write(&second, "raw\n").unwrap();

        let mut plan = ConfigPlan::new();
        plan.set(&first, "a", "new").unwrap();
        plan.set(&second, "b", "new").unwrap();

        assert!(apply_plan(&plan).is_err());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "a=old\n");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "raw\n");
        assert!(!temp_path(&first).exists());
        assert!(!temp_path(&second).exists());

        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_file(&second);
    }
}
