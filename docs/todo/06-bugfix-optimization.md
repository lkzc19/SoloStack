# 06 - Bug 修复、代码优化、UI 优化

本 TODO 聚焦已有功能的缺陷修复与体验打磨，不涉及新功能开发。

---

## BUG-01：多环境端口冲突导致状态误报

**严重程度**：中  
**影响范围**：设置 > 环境 tab 组件状态显示

**现象**：在环境 A 启动 Hadoop 后，查看环境 B 的 Hadoop 组件状态会显示"服务身份冲突"错误，但环境 B 的 Hadoop 实际并未运行。

**根因**：`classify_services`（`crates/core/src/platform/process.rs`）通过端口全局检测 + 命令行 environment_id 匹配判断进程归属。两个环境使用相同默认端口（如 Hadoop NameNode 9870），环境 A 占用端口后，环境 B 的状态检查会检测到端口被"他人"占用，判定为 Conflict。

**修复方案**：
- 短期：当组件未安装/未启动过时，跳过端口检测直接返回 Stopped
- 长期：多环境端口隔离（每个环境分配不同端口区间）

**修复状态**：✅ 已修复  
在展示层收敛「非活跃环境不展示状态」规则：
`lifecycle::environment::displayed_component_status(env, is_active, ..)`
对非活跃环境返回 `None`，`is_active` 显式传入，避免函数隐式依赖全局设置。  
环境概览命令的 `status` 字段对非活跃环境序列化为 `null`，前端不渲染状态徽标。  
`service::component_status` 保持纯净（只反映真实状态），停止路径仍用它预检，
确保能停掉非活跃环境中的遗留进程。  
新增测试：`displayed_status_is_absent_for_inactive_environment`。

---

## BUG-02：待发现

后续发现的 bug 记录在此。

---

# 代码优化（后端评审遗留）

来源：一次针对后端工程化/内聚性/耦合性/抽象恰当性的代码评审。
已修复原评审问题 1（`Status` 字符串映射重复）与问题 3（`component_status`
隐式全局依赖），其余按下表跟踪。

## CODE-01：安装并发与取消状态散落在命令层

**优先级**：中  
**位置**：`src-tauri/src/commands/install.rs`

**问题**：安装的互斥与取消状态由命令层持有（`static CANCEL_INSTALL` +
`static INSTALL_RUNNING`），而 core 已有 `lifecycle::lock`（环境操作锁）和
`lifecycle::install::InstallCancel`。并发控制策略被拆到两个层，命令层持有了
本属 core 的业务状态。

**建议**：core 暴露一个安装管理器（含"同一时刻只允许一个安装"与取消句柄），
命令层只做转发。与"实例级操作锁"是同一片区域，可一并考虑。

**修复状态**：✅ 已修复  
core 新增 `lifecycle::install::{InstallSession, begin_install_session,
cancel_active_install, install_running}`：会话创建时登记全局取消句柄，Drop 时
自动注销（RAII，去掉命令层「执行完手动复位静态量」的写法）。  
命令层删除 `static CANCEL_INSTALL` / `static INSTALL_RUNNING` / `InstallRunningGuard`，
`install_component` 只创建会话、跑阻塞安装；`cancel_install` 与
`delete_download_packages` 改为调 core。策略不变（全局单安装、取消语义一致）。  
新增测试：`install_session_is_exclusive_cancellable_and_self_cleaning`。

## CODE-02：`component/mod.rs` 契约与工具函数混装

**优先级**：中（TODO 07 加组件的前置）  
**位置**：`crates/core/src/component/mod.rs`

**问题**：同一个文件既定义对外 trait 契约（`FieldSchema` / `ConfigLifecycle` /
`Runtime` / `Component`），又堆放自由工具函数（`config_dir` / `config_path` /
`open_config` / `validate_install_params` / `prepare_config` / `validate_layout`）。

**建议**：把后者拆到独立的 `component/config_io.rs`（组件配置文件定位与打开），
`mod.rs` 只保留契约。建议在加 Flink/Spark 之前完成。

**修复状态**：✅ 已修复  
新增 `component/config_io.rs`，承接 `validate_layout` / `config_dir` /
`config_path` / `open_config` / `validate_install_params` / `prepare_config`
及其测试；`mod.rs` 从 260 行降到 121 行，只剩模块声明、re-export 与 4 个 trait。  
调用点统一改为 `component::config_io::*`（模块边界显式，不做 re-export 隐藏）。

## CODE-03：`app/app_log.rs` 单文件过大

**优先级**：低  
**位置**：`crates/core/src/app/app_log.rs`（966 行）

**问题**：单文件承担 6 类职责：日志级别、结构化记录、文件定位/轮转/裁剪、
查询分页、脱敏、`Operation` 生命周期跟踪。

**建议**：拆成 `log/{record,store,query,redact}.rs`。纯可维护性，不阻塞功能。

**修复状态**：✅ 已修复  
`app_log.rs` 拆为 `app_log/` 下 4 个模块：`record`（数据模型）、`store`
（写入/分片/轮转/清理）、`redact`（脱敏）、
`operation`（操作作用域）。`app_log.rs` 变为薄门面：模块声明 + `pub use`
re-export + 跨模块集成测试（988 行 → 实现分散，单文件均 < 300 行）。  
对外 API 路径不变（`app_log::*`），调用方零改动。

## CODE-04：`app/migration.rs` 过大且文档未标注例外

**优先级**：低  
**位置**：`crates/core/src/app/migration.rs`（794 行）

**问题**：它是 app 层唯一深knows组件实例磁盘布局的模块，与 `app/mod.rs`
文档"不含组件概念"的声明相冲突。

**建议**：迁移天然是历史兼容的例外，可保留，但应在 `app/mod.rs` 文档中显式
标注这一例外；体量与 CODE-03 同属大文件问题。

**修复状态**：✅ 已修复，后续按最低支持版本收敛
最终确认 `v0.2.0` 是最低支持版本，因此删除旧 `components/`、`var/` 等历史布局迁移
实现，不再保留 `app_layout`、`environment_layout`、`legacy`、`recovery`、`fs_ops`。

保留轻量版本门控：入口 `migration::run` 读取 `~/.solostack/.migration-version`，
当前布局基线为 `1`，`STEPS` 为空；未来布局变化时再追加具体迁移步骤。数据版本高于
程序支持时仍直接报错，保留降级保护。

## CODE-05：组件 `config.rs` 接近 800 行

**优先级**：低  
**位置**：`crates/core/src/component/hadoop/config.rs`（776 行）、
`component/kafka/config.rs`（783 行）

**问题**：每个组件把 `field_values` / `plan_field_updates` / `layout` /
`install_params` / `apply_install_config` 都塞进一个 `config.rs`。按此模式，
新增 Flink/Spark 会让每个组件文件继续逼近千行。

**建议**：与 CODE-02 同一处，可在加组件前把"字段定义 / 读写 / 安装落盘"再拆分。

**修复状态**：✅ 已修复  
hadoop / kafka 的 `config.rs` 各拆为 `config/` 下 5 个模块：
`effective`（生效值 `Effective`）、`schema`（`ConfigLifecycle` / `FieldSchema` 实现）、
`generate`（生成落盘）、`read`（精确读取与路径解析）、`mod.rs`（常量 + re-export +
集成测试）。实现文件均 ≤ 200 行；对外只经 `config::` 的 re-export 暴露（公开路径不变）。  
顺带修正：kafka runtime 测试改用生产同款 `configured_log_dir`（原用受管默认路径），
`managed_log_dir` 收回到 config 内部。

## CODE-06：`inspect_services` 每次调用都全量 `ps`

**优先级**：低  
**位置**：`crates/core/src/platform/process.rs:96`、
`crates/core/src/lifecycle/service.rs:72`

**问题**：每个 `ServiceSpec` 集合都会跑一次 `ps -axo`。多组件概览时对活跃环境
重复扫描进程表。

**建议**：同一次概览调用内复用 `scan_processes` 结果。BUG-01 修复后非活跃环境
已短路，影响已缓解。

**修复状态**：✅ 已修复  
`platform::process` 新增 `ProcessSnapshot`（一次 `ps`，可被多组 `ServiceSpec` 复用）；
`service::component_status_in` 接受快照，`component_status` 内部仍自行扫描。  
`lifecycle::environment::displayed_component_statuses(env, is_active)` 批量求状态：
非活跃环境直接全 `None`（不碰进程表），活跃环境只扫一次进程表。环境概览改用它
（原先是逐组件各扫一次）。  
顺带修正：无 service specs 的组件（未知组件 / 仅端口探活）不扫描进程表，保持原有的
惰性路径。

## CODE-07：命令层 Mutex 用 `lock().unwrap()`

**优先级**：低  
**位置**：命令层（原 `install.rs` 3 处 + `log_stream.rs` 3 处）

**问题**：core 层普遍用 `unwrap_or_else(|e| e.into_inner())` 降级处理锁中毒，
命令层未对齐；持锁线程 panic 会中毒并二次 panic。

**建议**：与 core 统一写法。

**修复状态**：✅ 已修复  
`install.rs` 的 3 处已随 CODE-01（安装会话收口 core）一并消失。`log_stream.rs`
余下 4 处统一走 `stream_registry()` 辅助函数，锁中毒时取回内部数据
（`unwrap_or_else(|poisoned| poisoned.into_inner())`），与 core 一致。

## CODE-08：错误类型统一为 `Result<T, String>`

**优先级**：记录，暂不改  
**位置**：全项目

**说明**：丢掉了结构化错误，前端无法区分"未找到"与"IO 失败"做不同处理，
只能整串展示。属有意简化，个人应用规模下可接受，仅作记录。

**处置**：保持不改（约 190 处签名，且前端把错误当不透明文案展示，暂无按类型
分支的消费方）。但已消除该设计唯一"咬人"的点：删除 `app_log::Operation::finish`
里 `error.contains("取消")` 的文案判断 —— 取消改由显式的 `finish_cancelled`
标记（WARN），其余失败一律 ERROR。新增测试
`operation_level_depends_on_explicit_finish_not_message_text`。  
若将来前端需要按错误类型分支，再按 §「折中」从边界做 kind 映射，不重构 core 签名。

---

# 前端代码优化（前端复查遗留）

来源：一次针对前端（Svelte 5 / SvelteKit）的代码复查。语法层面已完成 Svelte 5
迁移（无 `export let` / `$:` / `on:click` / `createEventDispatcher`），以下为工程
卫生与用户可见缺陷。

## FE-01：`flashSuccess()` 是空转，成功提示从未显示

**优先级**：高（用户可见）  
**位置**：`src/lib/stores.svelte.ts:36,74`、调用点约 20 处

**问题**：`flashSuccess()` 写 `store.successMsg` 并定时清空，但**全仓库模板没有任何
地方渲染 `successMsg`**；查 git 历史，从初版 `2d10fa0` 起它就只写不读。
"配置已保存""环境已创建""已删除 N 个缓存包""日志设置已更新""停止命令已执行"、
安装完成、卸载完成等提示全部静默丢弃。配置页只渲染 `errorMsg`。

**建议**：统一改走已有的 toast 系统（`notifications.svelte.ts::showToast`），
避免同时存在两套成功提示机制；确实要保留内联提示再补全局渲染。

**修复状态**：✅ 已修复  
`notifications.svelte.ts` 新增 `toastSuccess(message)`（直接弹 toast，不入通知中心、
不受通知类型开关影响）；删除 `store.successMsg` 与 `flashSuccess()`，全部约 20 处
调用改为 `toastSuccess(...)`。现在"配置已保存""环境已创建"等提示会真正弹出。

## FE-02：`Dropdown.svelte` 是死代码

**优先级**：中  
**位置**：`src/lib/Dropdown.svelte`（329 行）

**问题**：全仓库零引用，已被 `src/lib/components/ui/select/select.svelte` 取代
（props 几乎一一对应：`value`/`items`/`placeholder`/`onChange↔onSelect`/`class`）。
Select 用 bits-ui `Popover`，Dropdown 手算 `getBoundingClientRect` 定位菜单。

**建议**：删除。

**修复状态**：✅ 已修复（已删除）

## FE-03：`date-picker.svelte` 是死代码，而 `/logs` 用原生日期输入

**优先级**：中  
**位置**：`src/lib/components/date-picker.svelte`（189 行）、`src/routes/logs/+page.svelte:111`

**问题**：自绘月历组件（bits-ui Popover）零引用；`/logs` 用的是原生
`<input type="date">`，与 app 内其它自定义下拉风格不一致。

**建议**：二选一 —— 把 DatePicker 接到 `/logs`（视觉统一），或删除该组件改用原生。

**修复状态**：✅ 已修复（选择"接上组件"）  
`/logs` 的日期选择改用 `date-picker.svelte`（Popover + 自绘月历），替换原生
`<input type="date">`；给 DatePicker 补了可选的 `min` / `max`，越界日期置灰不可选，
保留原来"只挑有日志的日期区间"的约束。选到今天就回到实时跟随。

## FE-04：通知未读状态仍在用 legacy store

**优先级**：中  
**位置**：`src/lib/notifications.svelte.ts:2,14`

**问题**：`import { writable, get } from "svelte/store"` +
`hasUnreadStore = writable(false)`，而全项目其余（含同文件的 `notifications`、
`toastConfig`）都用 `$state`。当初是为绕开 Svelte 5 的
"Cannot export state from a module if it is reassigned"。

**建议**：改成导出的 `$state` 对象并改属性（`export const unread = $state({ value: false })`），
与同文件其余写法统一，去掉 `writable`/`get`。

**修复状态**：✅ 已修复  
`hasUnreadStore = writable(false)` → `unread = $state({ value: false })`；
`NotificationBell.svelte` 改为直接读 `unread.value`，去掉 `subscribe` 与本地镜像状态。

## FE-05：`<svelte:component>` 已废弃（2 处）

**优先级**：低  
**位置**：`src/routes/notifications/+page.svelte:49`、`src/lib/ToastContainer.svelte:16`

**问题**：Svelte 5 已废弃 `<svelte:component>`。

**建议**：改 `{@const Icon = levelIcon[level]}<Icon size={16} />`。

**修复状态**：✅ 已修复（两处均改为 `{@const Icon = ...}` + `<Icon />`）

## FE-06：状态文案重复映射

**优先级**：低  
**位置**：`src/lib/stores.svelte.ts:47`、`src/routes/settings/+page.svelte:280`

**问题**：`statusText: Record<Status, string>` 与 `componentStatusText()`
是同一份状态文案的两份实现，改文案要改两处。

**建议**：收敛到一处（`stores` 导出映射，或提到 `types.ts` 旁的工具）。

**修复状态**：✅ 已修复  
`stores/state.svelte.ts` 导出 `STATUS_TEXT` 与 `statusTextFor(raw)`（唯一一份映射）；
settings 环境 tab 删除本地 `componentStatusText`，改用 `statusTextFor(item.status)`。

## FE-07：两个超大文件

**优先级**：低  
**位置**：`src/routes/settings/+page.svelte`（1239 行）、`src/lib/stores.svelte.ts`（454 行）

**问题**：settings 页把 general / environments / cache / advanced / about 五个 tab
全放在一个文件；stores 把主题、环境、组件列表、安装进度、通用忙状态混在一个全局单例。

**建议**：settings 按 tab 拆子组件；stores 按域拆分，或改为 class + `$state` 字段
（可分组实例化）。与后端拆分 `app_log.rs` / `config.rs` 同类问题。

**修复状态**：✅ 已修复  
- `settings/+page.svelte` 1232 → 58 行（只留 tab 外壳），五个 tab 拆到
  `settings/tabs/{General,Environments,Cache,Advanced,About}Tab.svelte`，
  各自自管状态与加载。
- `stores.svelte.ts` 454 → 19 行门面，实现拆到 `lib/stores/`：
  `state`（store + 纯取值）/ `theme` / `environment` / `component` / `install` /
  `main`；调用方路径不变（`$lib/stores.svelte.ts`）。

## FE-08：两套样式体系并存

**优先级**：低  
**位置**：`src/app.css`（1967 行）、`src/lib/components/ui/*`

**问题**：业务样式几乎全是手写类 + CSS 变量；Tailwind v4 只服务 `button` /
`switch` / `select` 三个 UI 原语。不是错，但缺少明确约定。

**建议**：写明约定（UI 原语用 Tailwind，业务布局用变量 + 手写类），或择一收敛。

**修复状态**：✅ 已处理（在 `app.css` 顶部写明两套体系的分工与判断标准）

## FE-09：`+layout.svelte` 的监听竞态与无条件轮询

**优先级**：低  
**位置**：`src/routes/+layout.svelte`

**问题**：`listen(...).then(u => (unlisten = u))` 与返回的清理函数之间存在竞态 ——
若在 promise resolve 前卸载，`unlisten` 仍为 `undefined`，监听器泄漏；另外每 10 秒
无条件 `refreshComponents()`，在设置页、日志页等无关页面也在轮询。

**建议**：加 `cancelled` 标记（resolve 时若已卸载立即 unlisten）；轮询按路由条件启停。

**修复状态**：✅ 已修复  
监听注册统一走 `track(promise)`，resolve 时若已卸载立即 unlisten，否则收集进
`unlisteners`；轮询加 `needsComponentPolling(pathname)`，只在 `/`、`/component*`、
`/install*` 生效。
