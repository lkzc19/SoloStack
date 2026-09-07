# SoloStack 下载源设计文档

- 版本：v1.0（2026-08-22）
- 关联源码：`crates/core/src/source.rs`、`crates/core/src/sources.json`、`crates/core/src/download.rs`、`crates/core/src/install.rs`

> 本文档是下载源的**活文档**：修改下载源格式 / 预置内容 / 解析逻辑时需同步更新。

---

## 1. 设计目标

- 组件安装包的下载地址**由 SoloStack 统一维护**，用户（MVP 阶段）不可自定义。
- 每个组件（hadoop / kafka / flink…）预置**多个下载源**（如清华源、官方源），可切换。
- MVP **不考虑架构**：下载地址直接给完整 URL（含文件名与架构后缀），不做 base URL 拼接。
- 地址可查看（初始化时落盘），但由 SoloStack 随版本更新。

---

## 2. 文件格式

源文件为 **JSON**，结构与含义：

```json
{
  "sources": [
    {
      "id": "tuna",
      "name": "清华源",
      "components": {
        "hadoop": {
          "3.5.0": "https://mirrors.tuna.tsinghua.edu.cn/apache/hadoop/common/hadoop-3.5.0/hadoop-3.5.0-aarch64.tar.gz"
        }
      }
    },
    {
      "id": "official",
      "name": "官方源",
      "components": {
        "hadoop": {
          "3.5.0": "https://dlcdn.apache.org/hadoop/common/hadoop-3.5.0/hadoop-3.5.0-aarch64.tar.gz"
        }
      }
    }
  ]
}
```

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `sources[]` | 数组 | 下载源列表 |
| `.id` | string | 源唯一标识（`tuna` / `official`），持久化到 `settings.json` |
| `.name` | string | 展示名（清华源 / 官方源） |
| `.components` | 对象 | 组件名 → 版本 → 下载地址 |
| `.components.<组件>` | 对象 | 如 `hadoop` |
| `.components.<组件>.<版本>` | string | **完整下载 URL**（含文件名，MVP 含架构后缀） |

对应 Rust 结构（`source.rs`）：

```rust
pub struct SourceDef {
    pub id: String,
    pub name: String,
    pub components: BTreeMap<String, BTreeMap<String, String>>, // 组件 → 版本 → URL
}
```

---

## 3. 存放位置与维护方式

| 位置 | 说明 |
|---|---|
| **内置文件** `crates/core/src/sources.json` | SoloStack 维护、随 app 分发（`include_str!` 嵌入二进制），**唯一权威源** |
| **数据根目录** `~/.solostack/sources.json` | 首次初始化时由内置文件写入（`source::init_sources_file`），供用户查看；**用户不可编辑** |

- `load_sources()`：优先读 `~/.solostack/sources.json`；文件缺失时回退内置。
- 下载时以 `settings.json` 中当前选中的 `source_id` + 组件名 + 版本，调用 `resolve_url(source_id, component, version)` 取地址。

---

## 4. 解析逻辑

```rust
pub fn resolve_url(source_id: &str, component: &str, version: &str) -> Result<String, String>
```

- 从源文件找到 `id == source_id` 的源 → 找到 `components[component]` → 找到 `versions[version]` → 返回完整 URL。
- 任一层缺失均返回带中文的错误信息（如 `下载源 tuna 无 kafka 4.0 的下载地址`）。

下载链路（`install.rs` → `download.rs`）：
1. `Settings::load()` 取当前 `source_id`
2. `resolve_url(source_id, template.name, template.version)` 得完整 URL
3. `download(url, progress)` 流式下载到 `~/.solostack/downloads/`（文件名取自 URL 最后一段）
4. 校验（模板 `sha256` 非空时）→ 解压到实例目录

---

## 5. 预置内容（当前）

| 源 | 组件 | 版本 | 下载地址 |
|---|---|---|---|
| 清华源 | hadoop | 3.5.0 | `https://mirrors.tuna.tsinghua.edu.cn/apache/hadoop/common/hadoop-3.5.0/hadoop-3.5.0-aarch64.tar.gz` |
| 官方源 | hadoop | 3.5.0 | `https://dlcdn.apache.org/hadoop/common/hadoop-3.5.0/hadoop-3.5.0-aarch64.tar.gz` |
| 清华源 | kafka | 4.1.0 | `https://mirrors.tuna.tsinghua.edu.cn/apache/kafka/4.1.0/kafka_2.13-4.1.0.tgz` |
| 官方源 | kafka | 4.1.0 | `https://dlcdn.apache.org/kafka/4.1.0/kafka_2.13-4.1.0.tgz` |

> 后续补充 kafka / flink / zookeeper 等组件版本时，在 `crates/core/src/sources.json` 对应源下追加 `components` 条目即可，无需改代码逻辑。

---

## 6. 与旧方案的差异

| 维度 | 旧方案（已废弃） | 新方案 |
|---|---|---|
| 地址来源 | 模板 `download.path` 相对路径 + `Source::base_url()` 拼接 | 源文件直接存完整 URL |
| 源定义 | 代码内枚举（`Source::Tuna/Official/Custom`） | JSON 文件（`SourceDef`） |
| 自定义源 | 支持（`custom` + URL） | MVP 不支持（SoloStack 维护） |
| 架构 | base URL 不区分，模板 path 带 `aarch64` | 地址直接含 `aarch64`，MVP 不区分 |

---

## 6.5 与安装页的联动（2026-08-23 更新）

- **源数据迁至 `config_defs/*.json`**（每组件自带 `source[]` + `java_support`），`sources.json` 不再作为源数据来源。
- **组件 tab**：安装页顶部**横条 tab**（按钮样式），顺序**前端写死**（`["hadoop", "kafka"]`）。
- **下载源选择**：安装页下拉选择该组件的源（来自 config json 的 `source[].name`），可随时切换（网络不佳换源）。
- **版本下拉**：选中源后，版本列表 = 该源 `version` 键——**联动**。
- **JDK 必选**：下拉显示本机所有 JDK，仅 `java_support[选中版本]` 里列出的可选（其余置灰）。
- 安装时前端把 `(component, version, source_id, jdk_version)` 传给 `install_component`，后端用 `config_defs::resolve_url(component, source_name, version)` 取 URL 下载。
- 侧边栏只显示已安装组件（不再由源文件/模板提供"货架"）。

---

## 7. 后续扩展约定

- **新增组件/版本**：编辑 `crates/core/src/sources.json` 对应源，追加 `components` 条目；同时需在组件模板（`.templates/*.json`）补充该版本的元数据（启停脚本/服务/端口）。
- **自定义源（V1.1+）**：在 `sources.json` 结构上扩展 `custom` 源（带 URL 输入），或引入「源 → base URL + 相对路径」回退机制，届时再评估。
- **架构感知（后续）**：可在 `components.<组件>.<版本>` 下拆 `aarch64` / `x86_64` 两个地址，或在 URL 里用占位符替换。
- **校验**：目前模板 `sha256` 为空跳过校验；地址就绪后可补 SHA256。
