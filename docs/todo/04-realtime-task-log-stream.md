# 04 SoloStack 实时日志流与 trace 聚合 TODO

> 状态：第一版已实施，继续补组件日志关联和诊断能力。
> 前置：`02-structured-logging.md` 的结构化存储、任务上下文和脚本输出桥接。
> 目标：参考 Flink Web UI 的任务日志体验，在 SoloStack 中持续跟随日志，而不是每次重新读取完整文件。

---

## 1. 目标行为

- [x] 打开日志视图时，先回填最近 N 行。
- [x] 回填完成后自动进入实时跟随模式。
- [x] 持续显示应用操作、脚本 stdout/stderr 和组件运行日志。
- [x] 用户可以暂停和恢复实时更新。
- [x] 用户可以停止自动滚动，继续保留已经显示的日志。
- [x] 可以按 `trace_id` 只看一组日志。
- [x] 显示实时状态：跟随中、已暂停、当前 offset、丢弃的 UI 行数。
- [x] 日志文件被轮转、截断或重新创建时，不中断后续跟随。
- [x] 可以为一次操作打开独立日志视图，关联该操作产生的全部日志。
- [x] 应用日志使用独立 `/logs` 页面。
- [x] 主页设置按钮右侧可显示日志入口，通用设置可控制显隐。
- [x] 组件日志入口也进入 `/logs`，按文件选择来源并以原始文本展示。

---

## 2. 建议架构

后端新增统一 `LogHub`：

```text
app_log 写入
script stdout/stderr
component var/log 文件
        ↓
      LogHub
        ↓
 有界缓冲 / 批量聚合
        ↓
 Tauri event / 本地流式接口
        ↓
 前端虚拟化日志列表
```

建议模块：

```text
crates/core/src/lifecycle/log_stream.rs
```

核心组件：

- [x] `LogSourceSpec`：描述 app 或组件日志来源。
- [x] `FileFollower`：按路径和 offset 跟随文件。
- [x] `LogHub`：由 Tauri stream manager 管理订阅、取消和广播。
- [x] `LogBatch`：批量推送日志，避免逐行 IPC。
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

## 4. trace 关联

- [x] 复用生命周期 `trace_id`，关联操作日志和脚本日志。
- [x] 同一个 `trace_id` 的记录可以通过筛选聚合展示。
- [ ] 组件日志通过组件、版本和操作时间窗口关联。
- [ ] 未来接入 Flink Job 时使用 Job ID 关联 JobManager/TaskManager 日志。
- [x] 日志记录包含 `environment_id`。

---

## 5. Tauri 接口

- [x] `start_log_stream(sources, backfill_lines)`。
- [x] `stop_log_stream(stream_id)`。
- [x] `pause_log_stream(stream_id)`。
- [x] `resume_log_stream(stream_id)`。
- [x] 事件 `logs-stream://batch`：批量日志。
- [x] 事件 `logs-stream://status`：offset、轮转、丢弃和错误状态。

---

## 6. 前端日志视图

- [x] 使用固定行高虚拟化列表。
- [x] 默认跟随最新日志。
- [x] 用户向上滚动时暂停自动滚动。
- [x] 滚回底部时恢复跟随。
- [x] “暂停”不影响磁盘日志记录。
- [x] “清空”只清当前视图，不删除日志文件。
- [x] 支持按 `trace_id` 筛选。
- [ ] 支持复制当前诊断视图。
- [x] 显示“实时跟随 / 已暂停 / 已加载 N 行 / 丢弃 N 行”。

---

## 7. 非目标

- [x] 第一阶段不引入 Elasticsearch、Loki 或其他远程日志服务。
- [x] 第一阶段不上传用户日志。
- [x] 第一阶段不要求浏览器远程访问；优先使用 Tauri command/event。
- [x] 不把组件长时间运行日志全文复制进 app 日志。

---

## 8. 验收标准

- [x] 打开日志页时能回填历史日志并自动跟随新增日志。
- [x] 暂停后日志文件仍持续增长，恢复后能继续读取。
- [ ] Kafka/Hadoop 日志切换到新轮转文件后仍能继续实时显示。
- [ ] 一次操作的 app、脚本和关联组件日志可以在同一视图中查看。
- [x] 高频日志不会导致 UI 无响应或内存无限增长。
- [x] 关闭日志视图后，所有 follower 和订阅均被释放。

---

## 9. 实施顺序

1. `FileFollower` 和 offset/轮转测试。
2. `LogHub` 与 app/script 实时订阅。
3. Tauri stream command/event。
4. 虚拟化日志视图和跟随/暂停 UI。
5. 组件日志文件接入。
6. 按 `trace_id` 聚合和多环境日志维度。
