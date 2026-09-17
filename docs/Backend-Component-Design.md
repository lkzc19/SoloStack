# SoloStack-后端-大数据组件全生命周期设计

> 描述 SoloStack 后端如何管理一个大数据组件的**整个生命周期**：从安装到卸载。
> 活文档，随 `crates/core` 同步维护；如与代码冲突，以代码为准并回来更新本文件。
> 更新时间：2026-09-12

---

## 0. 工程结构

```
crates/core/src/
├── app/            SoloStack 自身：磁盘布局 paths / settings / app_log
├── platform/       机器与 OS 能力：arch / process（端口探活 + 托管子进程）/ jdk / app_scan
├── config/         配置文件读写（多格式键值 IO，零组件依赖）
│   └── mod / header / line / xml / properties / shell_env
├── component/      组件层
│   ├── mod / dto / install_config / fields / ports / registry / schema / instances / exec   ← 框架（怎么成为一个组件）
│   ├── hadoop/     内置组件实现（mod 声明 / config 配置 / runtime 运行）
│   └── kafka/      同上
├── lifecycle/      生命周期编排：install / uninstall / service / logs
└── package/        安装包获取：manifest(+json) / download / extract

src-tauri/src/
├── lib.rs          Tauri 装配（Builder + invoke_handler 注册）
└── commands/       命令层：app / component / install / logs（只做参数转换与错误映射）
```

**依赖只向下、不成环**：

```text
lifecycle ──► component ──► config / package / platform / app
```

三条维持该方向的纪律：

1. **`config/` 保持零组件依赖** —— 所以「配置字段调度」放在 `component/schema.rs` 而不是 `config/`：
   它要查组件注册表，放进去就会形成 `config ↔ component` 环。
2. **组件相关的 JDK/环境逻辑不放进 `platform/`** —— `platform::process::run_script_in`
   只拉进程（环境变量由调用方传入），「Java 组件该注入什么」在 `component/exec.rs`。
3. **安装参数 DTO 归组件契约** —— `component/install_config.rs`，因为它是
   `ConfigLifecycle::apply_install_config` 的入参。

**命名约定**：`<component>` 指**组件名**（hadoop / kafka），与 `package/manifest/<component>.json`、
`component/<component>/`、`registry::by_component(component)` 保持一致。
**不要用 `id` 指代组件名** —— `id` 在项目里另有用途（安装参数的 id、服务 id、实例标识等），
混用会让「组件」与「实例/服务」的边界模糊。

---

## 1. 核心分层原则：机制通用、内容独立

> **下载 / 解压 / 文件读写 / 状态探活这些「怎么做」全部组件共用；
> 「生成什么配置、怎么启动、有哪些配置字段」由组件自己实现。**

| 通用（框架，所有组件一致） | 组件独立（差异点，进 trait） |
|---|---|
| 路径 / 目录布局 | 配置**内容与语义**：写哪些键值、字段↔文件映射 |
| 下载（源解析 / 缓存 / 进度 / 取消 / 清理） | 配置布局 `config_layout`（官方配置目录 + 受管文件清单） |
| 解压（单层根目录处理、重装清旧目录） | 探活端口 `detect_ports`（从配置精确读）+ 端口避让 |
| InstallGuard 失败回滚（清已建实例目录） | 启停脚本序列 `start` / `stop` |
| 状态探活（进程身份 + 端口归属 = Running / 部分 = Partial / 冲突 = Error） | 配置字段 schema `field_values` / `plan_field_updates` |
| 配置布局校验（启动前确认受管文件都在） | 首启 init/format（`Runtime::init` 由 `service::start` 在启动前调用，见 §C） |
| JAVA_HOME 读写（写进组件声明的环境文件：官方优先，无官方文件的由 SoloStack 生成） | WebUI 入口 `web_uis` |
| 配置文件**读写机制**（`config/`：Xml / Properties / ShellEnv） | JAVA_HOME 落点声明 `java_env_file` |

### 1.1 安装参数：通用载体 + 组件声明

安装参数**不再写进共享契约**（此前 `InstallConfig` 里内联 hadoop 的 4 个字段、嵌套 kafka 的
`KafkaOpts`，命令层还要再镜像一套 DTO 并逐字段手抄，漏抄即静默丢值）：

| 通用 | 组件独立 |
|---|---|
| 载体 `InstallParams`（`参数 id → 字符串`）、`component` / `version` / `source_id` / `jdk_version` | 有哪些参数、默认值多少（`ConfigLifecycle::install_params`） |
| 参数 id 校验（`component::validate_install_params`：提交了未声明的 id 直接报错，防前后端漂移后静默回退） | 如何解析校验、落进哪些配置（`apply_install_config`） |
| 命令层透传（无镜像 DTO、无手抄映射） | 表单布局与文案（前端按组件定制，**不做通用化**） |

前端只有「布局与文案」是写死的，**默认值来自组件声明**（`list_install_params`）：
参数留空即回退组件默认值，前端不再各写一份 9870/9092 之类的默认值。

配置拆两层看：**读写机制通用**（`config/`），**生成内容组件独立**（组件 impl）。
配置**就地写在组件官方配置文件里**，不另存副本 —— 详见 `docs/Config-File-Design.md`。

---

## 2. 生命周期：两层模型

把一个组件的生命周期拆成两层，**不要混成一个状态机**：

| 层 | 是什么 | 谁说了算 |
|---|---|---|
| **操作状态**（本应用管理） | 空闲 / 安装中 / 启动中 / 停止中 / 卸载中 | 我们自己（内存 / 落盘） |
| **派生状态**（展示用） | 运行中 / 部分运行 / 已停止 / 异常 | **外部事实**：端口 / 进程探活 |

原因：守护进程可能**自己崩溃或外部被杀**，运行状态必须从探活实时派生，
不能靠"我们以为它还在跑"。所以操作状态机只管「当前允不允许下一个操作」。

### 2.1 实例生命周期总览

```
未安装
  │  安装（下载 → 解压 → 生成配置，通用 + 组件独立拼接）
  ▼
已安装 / 已停止
  │  启动前 ensure 配置（幂等补齐）→（首次启动前若需 init/format）
  ▼
启动中 ──► 运行中 / 部分运行（探活派生）
  │                │
  │  停止（逆序、优雅）  │
  ▼                ▼
已停止 ◄─────────────┘
  │  卸载（先停；keep_data 决定是否保留 var/data）
  ▼
未安装（组件目录删除，数据按选项保留）
```

任一步失败进入 **error 态**：可观测、可重试、安装期可回滚。

---

## 3. 阶段与行为

> 每行标「通用」或「组件独立」，并尽量对应到现有 trait 方法。

### A. 安装期（一次性；失败回滚）
| 行为 | 归属 | 备注 / hook |
|---|---|---|
| 下载到缓存（源解析 / 复用 / 进度 / 取消） | 通用 | 缓存包保留，卸载不动 |
| 校验包（大小 / SHA） | 通用（可加） | 当前未做 |
| 解压到实例目录 | 通用 | 处理单层根目录、清旧目录 |
| 失败回滚（清已建实例目录） | 通用 | InstallGuard（配置在实例内，一并清掉） |
| 计算探活端口（含占用避让）并写进官方配置 | **组件独立** | 与「生成初始配置」同一处发生 |
| **生成初始配置** | **组件独立** | `apply_install_config(version, cfg)` |
| 写 JAVA_HOME | 通用 | 写进组件声明的环境文件（`java_env_file`）：hadoop 用官方 `hadoop-env.sh`，kafka 用 SoloStack 生成的 `solostack-env.sh` |

### B. 配置（三个时机，不是一次）
1. **安装时生成** —— `apply_install_config`
2. **启动前校验 / 补齐**（幂等）—— `ensure_config`：先校验布局文件都在（缺失报错），
   再从配置**读回**当前生效值合并写回（只补缺失键，不改已有值）
3. **页面修改字段** → 重启生效 —— `save_fields` → `plan_field_updates` → `ConfigPlan`

三个时机作用于**同一份官方配置文件**，没有副本、没有 install.json，配置文件就是唯一事实源。

### C. 首启 init/format

FIX-08 已把首启初始化拆为 `Runtime::init(version)`，由 `service::start` 在真正启动进程前调用。

| 组件 | 首启动作 | 幂等信号（**官方产物**，不造标记文件） | 目标目录来源 |
|---|---|---|---|
| hadoop | `bin/hdfs namenode -format -force` | `dfs.namenode.name.dir/current/VERSION` | 配置精确读 |
| kafka | `kafka-storage.sh random-uuid` → `format --standalone -t <uuid> -c server.properties` | `log.dirs/meta.properties` | 配置精确读 |

两条共同规则（2026-09-13 统一）：

1. **幂等信号只用官方产物**。曾经 hadoop 还额外写一个 `.formatted` 标记文件，已删除 ——
   官方格式化产物本身就是最可靠的「已初始化」证据，自己造标记属于多余的侧车文件。
2. **判断的目标目录必须从配置精确读**，不能拼默认路径。否则用户手改
   `dfs.namenode.name.dir` / `log.dirs` 之后，判断位置与实际落盘位置错位：
   hadoop 会**每次启动都跑一次 `-format -force`（`-force` 会重格式化，抹掉命名空间数据）**；
   kafka 则会每次启动都尝试格式化一个已格式化的目录而报错。
   **但「写入」相反**：数据目录键由 SoloStack 拥有，始终写受管路径 —— 否则官方模板里的
   占位值会被回声采纳（Kafka 的 `/tmp/kraft-combined-logs`，系统清理 /tmp 后即丢数据）。

- 统一时序为：`prepare_config → Runtime::init → Runtime::start`。
- `init` 必须幂等；每次启动都可以调用，组件根据官方产物决定是否真正格式化。
- 两个组件的 init 内容不同：Hadoop 是一条 format 命令；Kafka 是获取 UUID 后再执行
  `format --standalone`。框架只统一调用时机，不强行统一内部命令模板。
- `init` 失败时 `start` 不得执行，避免启动未初始化的组件。

### D. 运行期（可反复、有依赖序）
| 行为 | 归属 |
|---|---|
| 启动：多子进程按序拉起 | 组件独立 `start` |
| 等待就绪 / 健康检查 | 通用（`detect_ports` 精确读端口 + 探活） |
| 停止：逆序、优雅 | 组件独立 `stop` |
| 状态查询 / 日志跟随 | 通用（端口 + 日志目录） |

### E. 卸载 / 清理
| 行为 | 归属 |
|---|---|
| 先优雅停止 | 通用（探活存活才调 stop） |
| 删除实例（含配置）+ 运行日志 + pid | 通用（幂等） |
| `keep_data` 保留 `var/data` | 通用 |
| 缓存清理（downloads） | 独立于组件卸载 |

### F. 横切
- **环境隔离**：`(environment_id, component)` 定位实例；同一环境中的同一种组件只能有一个版本，不同环境的数据和运行实例完全隔离。
- **禁止重复安装**：同一环境中的同一种组件只要已安装，安装页就不能再次安装；必须卸载后才能重新安装。恢复默认配置作为独立维护能力另行设计，不复用安装流程。
- **error / 重试语义**：启动失败后实例停在什么状态、能否重试。
- **快照 / 复刻**：远期（V1.1）。

---

## 4. 状态机设计（薄，不建议引框架）

状态少，Rust `enum` + 显式迁移函数即可：

```rust
/// 操作状态：本应用正在对这个实例做什么
enum OpState { Idle, Installing, Starting, Stopping, Uninstalling, Error }

fn try_start(s: &mut OpState) -> Result<(), String> {
    match s {
        OpState::Idle => { *s = OpState::Starting; Ok(()) }
        _ => Err("当前状态不允许启动".into()),
    }
}
```

- 非法迁移直接返回 `Err`，天然给前端"按钮禁用 + 防重入"。
- 展示用状态（running/partial/stopped/error）由端口探活单独派生，不进这个 enum。

---

## 5. 现有 trait 与生命周期的对应

| trait 方法 | 对应阶段 |
|---|---|
| `FieldSchema::field_values / plan_field_updates` | 配置读 / 批量规划（B3） |
| `ConfigLifecycle::config_layout` | 官方配置目录 + 受管文件清单 |
| `ConfigLifecycle::detect_ports` | 探活端口（从配置精确读） |
| `ConfigLifecycle::apply_install_config` | 安装生成初始配置 + 端口（A） |
| `ConfigLifecycle::ensure_config` | 启动前校验 / 补齐（B2） |
| `ConfigLifecycle::java_env_file` | JAVA_HOME 落点声明（A） |
| `Runtime::web_uis` | WebUI |
| `Runtime::init / start / stop` | 首启初始化（幂等）与运行期（D） |

---

## 6. 待议 / 待核实

- [x] ~~Kafka KRaft 首启是否需要 `kafka-storage.sh format`~~ → 需要，已实现（见 §C）
- [x] ~~Kafka 配置目录是否仍是 `config/kraft/`~~ → 否，已改为 `config`（4.1.0 核实、4.3.1 复核同形）
- [x] ~~**Kafka 尚未实机装过**~~ → 2026-09-14 已手动完成真实安装、KRaft format、启动和停止验证；未来回归由 FIX-08 的 CI smoke test 持续覆盖
- [ ] 清单版本会随镜像漂移（dlcdn/清华只留各线最新补丁版，旧版 404）—— 上架版本时逐源核对，见 `docs/Config-File-Design.md` §9.1
- [x] ~~是否给 trait 增加首启 `init` hook~~ → 需要：Hadoop/Kafka 已共有该阶段，FIX-08 统一为 `Runtime::init`，不再等待第三个组件
- [x] ~~安装参数是否 schema 化（移除 `HadoopPorts` / `KafkaConfig` 每组件 DTO）~~ → 已完成（见 §1.1）：通用 `InstallParams` + 组件 `install_params` 声明，命令层零镜像 DTO
- [ ] `category` 字段删除 or 作分组预留（已在 `ComponentInfo` 中移除）
- [x] ~~配置保存从「逐字段串行」改为一次批量原子提交~~ → 已完成：`save_config_fields` + `ConfigPlan` + 运行时失败回滚

---

## 7. 决策记录

| 日期 | 决策 |
|---|---|
| 2026-09-17 | **引入多环境模型**：环境使用稳定 UUID 和用户可见名称，所有组件、配置、日志、PID 与数据按 `environment_id` 隔离；同一环境中的同一种组件仅允许一个版本，切换环境前必须停止并验证旧环境全部组件。安装包缓存改为 `cache/downloads/` 的应用级共享缓存。 |
| 2026-09-14 | **系统边界收紧**：组件日志只允许当前实例目录内的真实 `log/out/err` 文件；app 日志日期严格校验；Tauri 启用生产/开发 CSP；opener 仅允许本地 WebUI 和 `~/.solostack` 路径。 |
| 2026-09-14 | **运行状态加入进程身份验证**：当前实例以 `component + version` 标识，组件声明预期 `ServiceKey`、进程命令行特征和监听端口；状态由匹配进程和端口归属共同推导，不再只看端口开放。 |
| 2026-09-14 | **前端组件适配注册表**：前端以类型安全 `ComponentAdapter` 集中声明 Logo、安装表单、配置表单和字段契约；后端 manifest、registry 与前端 adapter 集合必须完全一致，未完整适配的组件不进入发布支持列表。 |
| 2026-09-14 | **托盘错误可观测**：窗口显示/隐藏和 Dock 切换不再吞错，失败写入 app 日志；托盘图标经观感评估后保留原应用图标。 |
| 2026-09-08 | 组件差异多 trait 化（FieldSchema / ConfigLifecycle / Runtime + 静态注册表）；流程层不再写组件 match。 |
| 2026-09-08 | XML/properties 读写收敛到 `conf.rs` 的 `ConfigFile`（read/write/update_entries）。 |
| 2026-09-09 | 全生命周期设计采用「机制通用、内容独立」分层 + 「操作状态 / 派生状态」两层模型。 |
| 2026-09-12 | **取消配置副本**：配置就地写在组件官方配置文件里，`etc/`、`config.rs` 删除；`conf.rs` → `config/`（Xml/Properties/ShellEnv），写入只做合并 + 显式删除，并在受管文件注入说明头。 |
| 2026-09-12 | **取消侧车记录文件**：`.detect-ports` / `.java-home` 删除；探活端口与 JAVA_HOME 一律从官方配置文件精确读（`detect_ports` / `java_env_file`），`compute_detect_ports` / `apply_jdk_home` 随之删除。详见 `docs/Config-File-Design.md`。 |
| 2026-09-12 | **后端工程化重组**：`crates/core/src` 由 20 个平铺文件收成 `app` / `platform` / `config` / `component` / `lifecycle` / `package` 六层，依赖严格单向；顺带消除两处循环依赖（字段调度移入 `component`、组件 JDK/环境逻辑移出 `platform`）。`src-tauri` 命令层由单文件拆为 `commands/{app,component,install,logs}`。 |
| 2026-09-13 | **Kafka 上架版本改 4.3.1**（4.1.0 已被 dlcdn/清华淘汰，双源 404）；`java_support` 修正为 `[17,21,25]`（broker 最低 Java 17）。 |
| 2026-09-13 | **安装参数 schema 化**：`InstallConfig` 收成 `{component, version, source_id, jdk_version, params}`；组件用 `install_params(version)` 声明参数与默认值、`apply_install_config` 自行解析校验；命令层删除 `HadoopPorts`/`KafkaConfig` 与手抄映射，安装页默认值改由 `list_install_params` 提供（表单布局仍按组件定制，不做通用化）。 |
| 2026-09-13 | **审查后修复批次**：数据目录写受管路径/判断读配置、端口校验（0、特权端口、同组件判重）、JobHistory 关闭后改端口空操作、XML 注释区屏蔽、路径校验拒绝 `..`、下载 `.part` 原子落盘、解压暂存目录命名、JDK 策略唯一化、配置页执行 `ensure_config`。详见 `docs/Config-File-Design.md`。 |
| 2026-09-13 | **首启 init 统一为「官方产物 + 配置精确读」**：删掉 hadoop 的 `.formatted` 标记文件，改用 `dfs.namenode.name.dir/current/VERSION`；两个组件的判断目录都改为从配置精确读（原按默认路径拼，手改配置后会错位，hadoop 侧会导致每次启动重格式化）。 |
| 2026-09-14 | **首启初始化独立为 `Runtime::init`**：`service::start` 固定执行 `prepare_config → init → start`；init 失败不会启动常驻进程。新增真实 Hadoop/Kafka 生命周期 smoke test 和发布前 CI 验证。 |
| 2026-09-12 | **Kafka 按 4.1.0 官方事实修正**：配置目录 `config/kraft`→`config`、quorum 键 `voters`→`bootstrap.servers`、受管 `advertised.listeners`、首启补 `kafka-storage.sh format --standalone`；Kafka 的 JAVA_HOME 落盘在 SoloStack 生成的 `config/solostack-env.sh`（无官方环境文件）。 |
| 2026-09-12 | **组件实现改为每组件一个目录**：`component/<component>/{mod,config,runtime}.rs` —— 框架文件与组件实现分离，组件内部按框架的两条主线（配置 / 运行）分文件。新增组件路径更新为 `package/manifest/<component>.json` + `component/<component>/` + registry 加一行。 |
