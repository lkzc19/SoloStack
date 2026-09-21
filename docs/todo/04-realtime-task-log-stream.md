# 04 SoloStack 实时日志流与查看 TODO

> 状态：已实施。日志读取已统一到 core 的 `logs` 模块，按 Flink Web 的方式查看。
> 后续增量见第 10 节（组件日志轮转切换）。
> 前置：`02-structured-logging.md` 的结构化存储、任务上下文和脚本输出桥接。
> 目标：参考 Flink Web UI 的任务日志体验，在 SoloStack 中持续跟随日志，而不是每次重新读取完整文件。

---

## 1. 目标行为

- [x] 打开日志视图时，先回填最近 N 行。
- [x] 回填完成后自动进入实时跟随模式。
- [x] 持续显示应用操作、脚本 stdout/stderr 和组件运行日志。
- [x] 用户可以暂停和恢复实时更新。
- [x] 用户可以停止自动滚动，继续保留已经显示的日志。
- [x] 显示实时状态：跟随中、已暂停、当前 offset、丢弃的 UI 行数。
- [x] 日志文件被轮转、截断或重新创建时，不中断后续跟随。
- [x] 应用日志使用独立 `/logs` 页面。
- [x] 主页设置按钮右侧可显示日志入口，通用设置可控制显隐。
- [x] 组件日志入口也进入 `/logs`，按文件选择来源并以原始文本展示。

---

## 2. 实现架构

读取侧统一到一个 `logs` 模块（原 `app_log` 读路径 + `lifecycle/{logs,log_stream}`
已合并到此）；`app_log` 退回纯生产者：

```text
app_log 写入（生产者）
script stdout/stderr
component var/log 文件
        ↓
   core::logs（统一读取）
   LogSource + reader（tail / LogFollower）
        ↓
 Tauri stream manager（订阅/取消/广播，事件 logs-stream://*）
        ↓
   前端 /logs（单渲染器 + 就地搜索）
```

模块：

```text
crates/core/src/logs/
  model.rs    统一行模型 LogLine（app 结构化 / 组件 raw）
  source.rs   LogSource::{App{date}, Component{file}}：定位、枚举、路径守卫
  reader.rs   tail + LogFollower（轮转 / 半行缓冲）——唯一读取实现
```

核心组件：

- [x] `LogSource`：描述 app（按天 / 实时）或组件单文件来源。
- [x] `LogFollower`：按路径和 offset 跟随文件（含轮转与半行缓冲）。
- [x] Stream manager：由 Tauri 层管理订阅、取消和广播（原 `LogHub` 的职责）。
- [x] `LogLine` 批量推送（`logs-stream://batch`），避免逐行 IPC。
- [x] `StreamStatus`：跟随状态、文件轮转状态、丢弃行数和错误。

---

## 3. 文件跟随规则

- [x] 记录文件路径、文件身份和 offset。
- [x] 文件增长时只读取新增字节。
- [x] App 日志日期轮转后自动切换到新文件。
- [ ] 组件日志切换到新的轮转文件。
- [x] 文件长度小于原 offset 时按截断处理，从文件头重新读取。
- [x] 保留未完成行，下一次读取补齐后再输出。
- [x] 兼容 UTF-8、非 UTF-8 和无换行长行。
- [x] 前端使用有界列表，不能无限占用内存。
- [x] UI 队列满时允许丢弃最旧实时显示，但磁盘日志不会被丢弃。

---

## 4. trace 上下文

- [x] 复用生命周期 `trace_id`，把一次操作的操作日志与脚本日志串起来（仅作为字段
      写入，可用关键词过滤）。
- [x] 日志记录包含 `environment_id`。

> app 日志只是方便查看的地方，不做专用聚合 / 关联视图；需要深入排查时用终端直接看
> 日志文件。

---

## 5. Tauri 接口

- [x] `start_log_stream(sources, backfill_lines)`：来源统一为 `LogSourceRequest`
      （`app_log` 可带日期 / `component` 单文件）。
- [x] `stop_log_stream(stream_id)`。
- [x] `pause_log_stream(stream_id)`。
- [x] `resume_log_stream(stream_id)`。
- [x] `read_log_tail(source, lines)`：Flink 式读取来源末尾若干行（历史 / 静态）。
- [x] `list_component_logs`：列出组件可查看的日志文件。
- [x] `list_app_log_dates`：列出 app 日志可用日期。
- [x] 事件 `logs-stream://batch`：批量日志。
- [x] 事件 `logs-stream://status`：offset、轮转、丢弃和错误状态。

> 收敛说明：旧命令 `get_app_logs`、`query_app_logs`、`list_log_dates`（app）与
> `read_component_log_tail` 已删除，能力并入上表。

---

## 6. 前端日志视图

- [ ] 使用固定行高虚拟化列表（当前为普通列表渲染，有界上限 5000 行）。
- [x] 默认跟随最新日志。
- [x] 用户向上滚动时暂停自动滚动。
- [x] 滚回底部时恢复跟随。
- [x] “暂停”不影响磁盘日志记录。
- [x] “清空”只清当前视图，不删除日志文件。
- [x] 关键词就地过滤（覆盖时间 / trace_id / 级别 / 环境 / 消息）。
- [x] app 日志可按日期切换：实时（跟随最新）或历史某天（静态加载）。
- [x] app 与组件共用同一套 `/logs` 页面与渲染器（`raw` 行直接换行显示）。
- [ ] 支持复制当前诊断视图。
- [x] 显示“实时跟随 / 已暂停 / 已加载 N 行 / 丢弃 N 行”。

---

## 7. 非目标

- [x] 第一阶段不引入 Elasticsearch、Loki 或其他远程日志服务。
- [x] 第一阶段不上传用户日志。
- [x] 第一阶段不要求浏览器远程访问；优先使用 Tauri command/event。
- [x] 不把组件长时间运行日志全文复制进 app 日志。
- [x] 不做 app 日志内的专用 trace 聚合 / 关联视图（需深入排查时用终端看文件）。

---

## 8. 验收标准

- [x] 打开日志页时能回填历史日志并自动跟随新增日志。
- [x] 暂停后日志文件仍持续增长，恢复后能继续读取。
- [ ] Kafka/Hadoop 日志切换到新轮转文件后仍能继续实时显示。
- [x] 高频日志不会导致 UI 无响应或内存无限增长。
- [x] 关闭日志视图后，所有 follower 和订阅均被释放。

---

## 9. 实施顺序

1. `FileFollower` 和 offset/轮转测试。
2. `LogHub` 与 app/script 实时订阅。
3. Tauri stream command/event。
4. 虚拟化日志视图和跟随/暂停 UI。
5. 组件日志文件接入。

---

## 10. 本轮收敛（按 Flink Web 方式查看）

- [x] 读取侧统一到 `crates/core/src/logs/`：`LogSource` + `tail`/`LogFollower`
      一份实现，app 与组件共用。
- [x] 行解析一份：JSONL → `LogLine`，兼容旧 `[time] LEVEL message` 文本，其余
      按原始行（`raw = true`）。
- [x] 路径守卫收成一处（`logs::validated_component_file`），删除命令层重复校验。
- [x] `app_log` 退回生产者：保留写入 / 轮转 / 保留策略 / `Operation` / 脱敏，
      移除读取（`query.rs`）。
- [x] `/logs` 一页一渲染器：来源参数不同（app 日期 / 组件文件），渲染逻辑同一套。
- [x] 搜索遵循 Flink 做法：在**已加载内容**里就地过滤，不做服务端全量检索。

**明确不做**（按最终取舍）：

- app 日志的专用 trace 聚合 / 关联视图（需要时用终端看日志文件）。
- 服务端跨分片全量搜索与偏移翻页（Flink 式就地搜索已足够）。
- 组件日志目录多文件聚合查看（一次看一个文件）。

**暂未实现**（可后续再定）：

- 组件日志切换到新的轮转文件后继续实时显示。
