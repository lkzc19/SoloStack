# 02 SoloStack 结构化日志 TODO

> 状态：第一版已实施；基础日志、脚本输出、轮转脱敏和 GUI 查询已落地，剩余增强项见文末。
> 创建时间：2026-09-14
> 实施时间：2026-09-15
> 目标：让应用操作、组件脚本和运行进程具备统一、可追踪、可筛选的诊断日志。
> 后续：实时日志流与任务级日志视图见 `docs/todo/04-realtime-task-log-stream.md`。

---

## 1. 改造前状态

- app 操作日志主要来自代码中的 `app_log::append`。
- 当前只有 `INFO / WARN / ERROR` 三个字符串常量，没有结构化级别枚举。
- 没有统一的组件、版本、操作、任务 ID、耗时和退出码字段。
- `platform::process` 已经捕获外部脚本的 stdout/stderr，但成功输出通常被调用方丢弃。
- 脚本失败时的输出可能进入前端错误消息，但不一定持久化到 app 日志。
- 组件自身写入 `var/log/<组件>/<版本>/` 的运行日志可以被日志页面列出。
- app 操作日志和组件运行日志目前没有统一的查看、筛选和关联方式。
- 多处 `let _ = app_log::append(...)` 会静默忽略写日志失败。

### 1.1 当前实现

- 已用 `LogLevel`、`LogSource`、`LogRecord`、`LogContext` 建立结构化日志模型。
- 日志持久化为 JSON Lines，GUI 读取结构化记录并格式化成可读列表。
- 生命周期操作具有 `task_id`，外部脚本日志自动继承组件、版本和操作上下文。
- 脚本 stdout/stderr 边读边上报，长行按 4 KB 切块，单次捕获输出限制为 1 MB。
- 已实现日志参数、环境变量和常见 Token/Password 字段脱敏。
- 已按天轮转；默认保留 7 天、总容量上限 1024 MB，可在设置页修改。
- GUI 支持日期、级别、组件、版本、操作、文本筛选、自动刷新、诊断复制和日志目录打开。

---

## 2. 目标

- [x] 一次操作可以从开始、脚本执行、输出、失败到结束完整追踪。
- [x] 日志有明确级别，支持按级别过滤。
- [x] 外部脚本 stdout/stderr 自动记录，不依赖脚本自己写文件。
- [x] 组件运行日志与 SoloStack 操作日志保持边界，但可以按组件/版本关联。
- [x] 长输出不会撑爆单个日志文件或 UI。
- [x] 敏感参数和值可以脱敏。
- [x] 日志轮转和保留策略可配置。
- [x] GUI 可以按日期、级别、组件、版本、操作和文本筛选。

---

## 3. 建议日志模型

每条日志至少包含：

```text
timestamp
level
source
event
component
version
operation
task_id
message
fields
```

`source` 建议取值：

```text
app
process
script.stdout
script.stderr
component
```

示例：

```text
2026-09-14 15:19:01 INFO task=start-42 source=app event=start.begin component=kafka version=4.3.1
2026-09-14 15:19:01 INFO task=start-42 source=process event=script.begin script=bin/kafka-server-start.sh args=[...] timeout=60s
2026-09-14 15:19:02 WARN task=start-42 source=script.stderr message="..."
2026-09-14 15:19:02 ERROR task=start-42 source=process event=script.failed exit_code=7 duration_ms=980
```

### 待确认

- [x] 持久化格式使用可读文本还是 JSON Lines。
- [x] 是否同时保留 JSONL 和面向 GUI 的格式化文本。
- [x] 日志字段使用固定 schema 还是允许事件自定义字段。

已决定：

- [x] 持久化格式使用 JSON Lines。
- [x] GUI 读取结构后自行格式化，不额外维护一份文本日志。
- [x] 固定核心字段，事件差异放入 `fields`。

---

## 4. 日志分级

建议定义：

```rust
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}
```

使用建议：

- `ERROR`：操作失败、脚本非零退出、回滚失败、无法恢复的异常。
- `WARN`：可恢复异常、超时后清理、停止失败、校验失败。
- `INFO`：用户可感知的操作开始/结束、安装、启停、配置保存、环境切换。
- `DEBUG`：脚本参数、工作目录、环境变量白名单、状态轮询结果。
- `TRACE`：逐块进程输出、底层重试和高频诊断；默认关闭。

默认策略：

- [x] 发布版本默认 `INFO`。
- [x] 开发模式默认 `DEBUG`。
- [x] 设置页允许切换日志级别。
- [x] 高频状态轮询不能写入 INFO。

---

## 5. 外部脚本日志

所有通过 `component::exec` 和 `platform::process` 执行的脚本必须自动记录。

### 执行前

- [x] 记录组件、版本、操作名和 `task_id`。
- [x] 记录脚本相对路径、参数、工作目录和超时。
- [x] 记录经过白名单筛选的环境变量。
- [x] 对密码、Token、Cookie 和认证参数脱敏。

### 执行中

- [x] stdout 按行写入 `source=script.stdout`。
- [x] stderr 按行写入 `source=script.stderr`。
- [x] 没有换行的长输出也要按大小切块，避免无限缓冲。
- [ ] 单条日志最大长度可配置，超出部分标记截断。
- [ ] 取消安装时记录取消来源和脚本是否被终止。

### 执行后

- [x] 记录退出码、duration、timeout 和 process_group_killed。
- [ ] 统一记录 cancelled 及其来源。
- [x] 失败时记录 stdout/stderr 的末尾摘要。
- [x] 成功时 stdout 默认写 DEBUG，stderr 默认写 WARN。
- [x] 出错时 stdout/stderr 至少提升到 ERROR 的关联摘要。

---

## 6. 日志分层

### app 操作日志

```text
~/.solostack/app/log/
```

职责：

- SoloStack 自身操作。
- 生命周期编排。
- 外部脚本命令与输出。
- 配置事务和环境切换。

### 组件运行日志

```text
~/.solostack/var/log/<component>/<version>/
```

职责：

- Hadoop/Kafka 自己输出的运行日志。
- 组件进程日志、GC 日志和访问日志。

规则：

- [x] 不把组件长时间运行日志复制进 app 操作日志。
- [x] app 日志记录组件、版本、任务 ID 和脚本路径。
- [x] 日志页面可以关联显示 app 操作日志与组件运行日志。

---

## 7. 轮转、保留与容量

- [x] 按天轮转。
- [x] App 日志只按天轮转，不再维护单文件大小配置。
- [x] 默认保留 7 天，可在设置页修改。
- [x] 设置最大日志占用空间。
- [ ] 清理时优先删除最旧的 DEBUG/TRACE 日志。
- [ ] 日志清理操作本身要写入 INFO。
- [ ] 磁盘写入失败不能导致主业务流程崩溃，但必须产生可见错误。

---

## 8. GUI 能力

- [x] 日期选择。
- [x] 级别筛选。
- [x] 组件和版本筛选。
- [x] 操作类型筛选。
- [x] 文本搜索。
- [x] 实时 tail。
- [x] 一键复制诊断信息。
- [x] 一键打开对应组件日志目录。
- [x] 大日志流式扫描，仅加载最后 N 条到 WebView。

---

## 9. 实施顺序

### [x] LOG-01：结构化 app logger

- [x] 用 `LogLevel` 枚举替代字符串常量。
- [x] 引入统一 `LogRecord` / `Logger`。
- [x] 支持级别过滤和字段序列化。
- [x] 日志写入失败返回错误，同时输出到 stderr。

### [x] LOG-02：脚本输出桥接

- [x] `process` 执行器接受任务日志上下文。
- [x] 自动分流 stdout/stderr。
- [x] 记录命令、耗时、退出码和超时。
- [ ] 统一记录脚本被取消的来源和终止状态。
- [x] 处理长行、乱码、空行和无换行输出。

### [x] LOG-03：轮转、保留和脱敏

- [x] 按天与大小轮转。
- [x] 保留期限和空间上限。
- [x] 参数及环境变量脱敏。
- [x] 增加清理和容量测试。
- [ ] 清理时优先删除最旧的 DEBUG/TRACE，而不是只按时间删除。

### [x] LOG-04：GUI 日志中心

- [x] 级别、组件、版本、操作和文本筛选。
- [x] 实时 tail。
- [x] 复制诊断信息。
- [x] 区分 app 日志与组件日志。
- [x] 在 app 日志中心直接打开当前组件的日志目录。

---

## 10. 回归测试

- [x] INFO 模式下不会输出 DEBUG/TRACE。
- [x] 脚本成功时 stdout/stderr 均可追踪。
- [x] 脚本失败和超时记录退出原因。
- [ ] 脚本取消来源和终止状态仍待统一。
- [x] 超长行被切块且不会无限占用内存。
- [x] 敏感参数不会出现在日志和错误消息中。
- [x] 并发任务的日志通过 `task_id` 正确关联。
- [x] 日志轮转使用进程内写锁，不会丢失正在执行的诊断上下文。
- [ ] 磁盘写满时主业务返回可见警告。
- [x] 组件运行日志和 app 操作日志不会互相覆盖。

---

## 11. 非目标

- [ ] 不在日志系统第一阶段引入远程日志服务。
- [ ] 不上传用户日志。
- [ ] 不把组件日志全文复制到 app 日志。
- [ ] 不让日志系统接管组件自身的日志框架。

---

## 12. 前置条件

- [x] FIX-01 的统一脚本执行接口已稳定。
- [x] FIX-04 的日志路径安全边界已稳定。
- [x] 生命周期操作具有稳定的操作名和任务 ID。
- [ ] 多环境模型确定后再决定 `environment_id` 是否进入日志一级维度。

---

## 13. 剩余增强项

- [ ] 统一脚本取消来源和终止状态。
- [ ] 单条日志最大长度改为可配置。
- [ ] 容量清理时优先淘汰 DEBUG/TRACE。
- [ ] 日志清理结果写入 INFO。
- [ ] 磁盘写满时向 GUI 返回可见警告。
- [ ] 决定 `environment_id` 是否进入日志一级维度。
