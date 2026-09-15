# SoloStack 日志体系后续 TODO

> 状态：下一版本处理，不进入 `fix/core-lifecycle-hardening` 或当前版本。
> 创建时间：2026-09-14
> 目标：让应用操作、组件脚本和运行进程具备统一、可追踪、可筛选的诊断日志。

---

## 1. 当前状态

- app 操作日志主要来自代码中的 `app_log::append`。
- 当前只有 `INFO / WARN / ERROR` 三个字符串常量，没有结构化级别枚举。
- 没有统一的组件、版本、操作、任务 ID、耗时和退出码字段。
- `platform::process` 已经捕获外部脚本的 stdout/stderr，但成功输出通常被调用方丢弃。
- 脚本失败时的输出可能进入前端错误消息，但不一定持久化到 app 日志。
- 组件自身写入 `var/log/<组件>/<版本>/` 的运行日志可以被日志页面列出。
- app 操作日志和组件运行日志目前没有统一的查看、筛选和关联方式。
- 多处 `let _ = app_log::append(...)` 会静默忽略写日志失败。

---

## 2. 目标

- [ ] 一次操作可以从开始、脚本执行、输出、失败到结束完整追踪。
- [ ] 日志有明确级别，支持按级别过滤。
- [ ] 外部脚本 stdout/stderr 自动记录，不依赖脚本自己写文件。
- [ ] 组件运行日志与 SoloStack 操作日志保持边界，但可以按组件/版本关联。
- [ ] 长输出不会撑爆单个日志文件或 UI。
- [ ] 敏感参数和值可以脱敏。
- [ ] 日志轮转和保留策略可配置。
- [ ] GUI 可以按日期、级别、组件、操作和文本筛选。

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

- [ ] 持久化格式使用可读文本还是 JSON Lines。
- [ ] 是否同时保留 JSONL 和面向 GUI 的格式化文本。
- [ ] 日志字段使用固定 schema 还是允许事件自定义字段。

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

- [ ] 发布版本默认 `INFO`。
- [ ] 开发模式默认 `DEBUG`。
- [ ] 设置页允许切换日志级别。
- [ ] 高频状态轮询不能写入 INFO。

---

## 5. 外部脚本日志

所有通过 `component::exec` 和 `platform::process` 执行的脚本必须自动记录。

### 执行前

- [ ] 记录组件、版本、操作名和 `task_id`。
- [ ] 记录脚本相对路径、参数、工作目录和超时。
- [ ] 记录经过白名单筛选的环境变量。
- [ ] 对密码、Token、Cookie 和认证参数脱敏。

### 执行中

- [ ] stdout 按行写入 `source=script.stdout`。
- [ ] stderr 按行写入 `source=script.stderr`。
- [ ] 没有换行的长输出也要按大小切块，避免无限缓冲。
- [ ] 单条日志最大长度可配置，超出部分标记截断。
- [ ] 取消安装时记录取消来源和脚本是否被终止。

### 执行后

- [ ] 记录退出码、duration、timeout、cancelled 和 process_group_killed。
- [ ] 失败时记录 stdout/stderr 的末尾摘要。
- [ ] 成功时 stdout 默认写 DEBUG，stderr 默认写 WARN。
- [ ] 出错时 stdout/stderr 至少提升到 ERROR 的关联摘要。

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

- [ ] 不把组件长时间运行日志复制进 app 操作日志。
- [ ] app 日志记录组件日志目录和关键文件引用。
- [ ] 日志页面可以关联显示 app 操作日志与组件运行日志。

---

## 7. 轮转、保留与容量

- [ ] 按天轮转。
- [ ] 单文件达到阈值后切分，例如 50 MB。
- [ ] 默认保留 14 天，可在设置页修改。
- [ ] 设置最大日志占用空间。
- [ ] 清理时优先删除最旧的 DEBUG/TRACE 日志。
- [ ] 日志清理操作本身要写入 INFO。
- [ ] 磁盘写入失败不能导致主业务流程崩溃，但必须产生可见错误。

---

## 8. GUI 能力

- [ ] 日期选择。
- [ ] 级别筛选。
- [ ] 组件和版本筛选。
- [ ] 操作类型筛选。
- [ ] 文本搜索。
- [ ] 实时 tail。
- [ ] 一键复制诊断信息。
- [ ] 一键打开对应组件日志目录。
- [ ] 大日志使用分页或增量加载，不整文件塞进 WebView。

---

## 9. 实施顺序

### [ ] LOG-01：结构化 app logger

- [ ] 用 `LogLevel` 枚举替代字符串常量。
- [ ] 引入统一 `LogRecord` / `Logger`。
- [ ] 支持级别过滤和字段序列化。
- [ ] 日志写入失败返回可观测错误。

### [ ] LOG-02：脚本输出桥接

- [ ] `process` 执行器接受 `task_id` 和日志上下文。
- [ ] 自动分流 stdout/stderr。
- [ ] 记录命令、耗时、退出码、超时和取消。
- [ ] 处理长行、乱码、空行和无换行输出。

### [ ] LOG-03：轮转、保留和脱敏

- [ ] 按天与大小轮转。
- [ ] 保留期限和空间上限。
- [ ] 参数及环境变量脱敏。
- [ ] 增加清理和容量测试。

### [ ] LOG-04：GUI 日志中心

- [ ] 级别、组件、版本、操作和文本筛选。
- [ ] 实时 tail。
- [ ] 复制诊断信息。
- [ ] 区分 app 日志与组件日志。

---

## 10. 回归测试

- [ ] INFO 模式下不会输出 DEBUG/TRACE。
- [ ] 脚本成功时 stdout/stderr 均可追踪。
- [ ] 脚本失败、超时和取消都记录退出原因。
- [ ] 超长行被截断且不会无限占用内存。
- [ ] 敏感参数不会出现在日志和错误消息中。
- [ ] 并发任务的日志通过 `task_id` 正确关联。
- [ ] 日志轮转不会丢失正在执行的诊断上下文。
- [ ] 磁盘写满时主业务返回可见警告。
- [ ] 组件运行日志和 app 操作日志不会互相覆盖。

---

## 11. 非目标

- [ ] 不在日志系统第一阶段引入远程日志服务。
- [ ] 不上传用户日志。
- [ ] 不把组件日志全文复制到 app 日志。
- [ ] 不让日志系统接管组件自身的日志框架。

---

## 12. 前置条件

- [ ] FIX-01 的统一脚本执行接口已稳定。
- [ ] FIX-04 的日志路径安全边界已稳定。
- [ ] 生命周期操作具有稳定的操作名和任务 ID。
- [ ] 多环境模型确定后再决定 `environment_id` 是否进入日志一级维度。
