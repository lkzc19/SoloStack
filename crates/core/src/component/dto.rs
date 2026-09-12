//! 组件抽象层公共 DTO。

/// 一个配置字段的当前值（前端表单通过 key 引用它；呈现方式由前端决定）。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ConfigFieldValue {
    /// 字段 key（提交 set_field 时用）。
    pub id: String,
    /// 当前值（从配置文件精确读出的字符串语义值）。
    pub value: String,
}

/// 组件配置文件的布局：官方配置目录 + 受管文件清单。
///
/// 配置直接读写解压包里的官方文件，不另存副本；`files` 用于启动前校验与
/// 声明「哪些文件属于本组件」，不参与复制。
#[derive(Debug, Clone)]
pub struct ConfigLayout {
    /// 相对实例根的配置目录（如 `etc/hadoop`、`config`）。
    pub dir: &'static str,
    /// 本组件关注的配置文件（用于存在性校验与文档）。
    pub files: &'static [&'static str],
}

/// 一个 WebUI 入口(GUI 跳转用)。
#[derive(Debug, Clone, serde::Serialize)]
pub struct WebUi {
    pub name: String,
    pub url: String,
}
