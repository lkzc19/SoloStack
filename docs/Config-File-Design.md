# SoloStack-配置文件读写设计

> 描述 SoloStack 如何读写大数据组件的配置文件：抽象分层、多格式支持、写入语义、精确读。
> 活文档，随 `crates/core/src/config/` 同步维护；如与代码冲突，以代码为准并回来更新本文件。
> 创建时间：2026-09-12

---

## 1. 三条设计原则（用户拍板）

1. **不建影子副本**：直接改解压包里的官方配置文件，`etc/` 副本机制废弃。
2. **不留侧车记录文件**：探活端口、JAVA_HOME 等一律从配置文件**精确读**，`.detect-ports` / `.java-home` 废弃。
   JAVA_HOME 的落点优先用组件官方环境文件（hadoop 的 `hadoop-env.sh`）；组件没有官方文件时（Kafka），
   由 SoloStack 生成一个**明文可见**的 `solostack-env.sh`，而不是隐藏的单值记录文件 —— 见 §7.3。
3. **写入即全量覆盖**：由 SoloStack 管理的键值会被整体重写，因此每个受管文件都要在文件内**声明这件事**，避免用户手改后被静默覆盖。
   （落地形态见 §6.1：写入统一为「受管键合并 + 需要移除时显式删除」，而非整文件替换 —— 效果对受管键而言等同于全量覆盖。）

---

## 2. 为什么重构

### 2.1 现状的四个问题

| 问题 | 现状 | 后果 |
|---|---|---|
| 模块命名歧义 | `conf.rs` 与 `config.rs` 并存 | 分不清哪个是「文件格式 IO」哪个是「路径/配置副本逻辑」 |
| 影子副本 | `etc/<组件>-<版本>/` 复制一份配置，`HADOOP_CONF_DIR` 指向副本 | 同一份配置两个位置；副本不完整要靠 `RUNTIME_CONFIG_FILES` 补丁（只补 4 个文件）；用户看官方目录以为改这里生效 |
| 侧车记录文件 | `.detect-ports`（端口）、`.java-home`（JDK） | 与配置文件**可能不一致**（手改配置后探活端口还是旧的）；文件系统里散布隐藏文件 |
| 无语义说明 | 配置副本里的文件没有来源与覆盖规则说明 | 用户手改后在下一次应用内修改时被静默覆盖 |

### 2.2 关键事实（2026-09-12 实机核对 `components/hadoop/hadoop-3.5.0`）

- 官方 `etc/hadoop/` 自带 `core-site.xml` / `hdfs-site.xml` / `yarn-site.xml` / `mapred-site.xml`（`<configuration>` 空、带 Apache license 头）、`workers`（内容 `localhost`）、`hadoop-env.sh`、`log4j.properties`、`capacity-scheduler.xml` —— **原地改即可，无需拷贝、无需补齐**。
- `hadoop-env.sh`（434 行）中 `JAVA_HOME` / `HADOOP_LOG_DIR` / `HADOOP_PID_DIR` **全部是注释行**（`# export JAVA_HOME=`），写我方 `export` 不存在覆盖冲突。
- Kafka 4.3.1 官方布局为 `config/server.properties`（**无 `kraft/` 子目录**，4.0 起 KRaft 唯一后官方已铺平；4.1.x 同形）；
  broker 端口在 `listeners` 里，对外公布地址在 `advertised.listeners` 里（两者端口必须一致）。
  官方 quorum 引导键是 `controller.quorum.bootstrap.servers`（旧键 `controller.quorum.voters` 已被 KIP-853 取代）。

---

## 3. 模块划分

```
crates/core/src/
├── config/                 ← 配置文件读写（纯 IO，不依赖组件）
│   ├── mod.rs              ← Entry / Format / ConfigFile trait / open / 原子写收口
│   ├── header.rs           ← 受管说明头的渲染、剥离、注入
│   ├── line.rs             ← 行式格式共享算法（合并 / 删除 / 精确读）
│   ├── transaction.rs      ← ConfigPlan 与多文件保存事务
│   ├── xml.rs              ← <configuration><property> 语义
│   ├── properties.rs       ← key=value 行式语义（server.properties）
│   └── shell_env.rs        ← export KEY=VALUE 语义（hadoop-env.sh）
└── component/
    ├── schema.rs           ← 字段调度（list_fields / save_fields / ConfigPlan）
    └── ...                 ← 组件抽象、各组件实现、实例发现
```

**命名理由**：`config/` 以模块内主类型 `ConfigFile` 为名（Rust 惯例）；原 `conf.rs` / `config.rs` 两个相近名字的问题，随 `config.rs` 的删除彻底消失。字段调度放在 `component/schema.rs` 而不是 `config/`，是为了让 `config/` 保持「零组件依赖」的叶子层——`schema` 要查组件注册表，放进来会造成 `config ↔ component` 循环依赖。

### 3.1 职责边界

| 层 | 知道什么 | 不知道什么 |
|---|---|---|
| `config/*` | 文件路径、格式、键值、注释样式、原子写 | 不知道「hadoop」「端口」「JDK」等业务概念 |
| `component/*` | 某组件的配置目录、受管文件、端口语义、JAVA_HOME 落点 | 不知道 XML/properties 怎么解析 |
| `component::schema` / `lifecycle::*` | 编排流程，只经组件 trait 调能力 | 不写组件专属分支 |

---

## 4. `ConfigFile` 抽象

### 4.1 语义单位

```rust
/// 一个键值条目（不同格式的公共语义单位）。
pub struct Entry { pub key: String, pub value: String }
```

### 4.2 trait

**只提供合并写，没有「整体替换」接口** —— 这个限制是刻意的：一旦存在「用传入条目
覆盖整个文件」的方法，调用方一个手滑就会抹掉官方模板里的几十行注释与默认值
（Kafka 的 `server.properties` 就是典型）。需要移除的键显式 `remove`。

```rust
pub trait ConfigFile {
    fn path(&self) -> &Path;
    fn format(&self) -> Format;
    /// 该文件的注释语法；None = 不能写注释（如未来 JSON），此时跳过受管头部。
    fn comment_style(&self) -> Option<CommentStyle>;   // 默认由 format 决定

    /// 读全部键值（不含注释与受管头部；文件不存在视为空）。
    fn read_entries(&self) -> Result<Vec<Entry>, String>;

    /// 精确读单个键；不存在返回 None。
    fn get(&self, key: &str) -> Result<Option<String>, String>;

    /// 在内存中完成合并与删除，返回待提交的完整内容。
    fn render_entries(&self, additions: &[Entry], removals: &[&str])
        -> Result<String, String>;

    /// 单文件便捷写：render_entries + commit。
    fn update_entries(&self, additions: &[Entry]) -> Result<(), String>;
    fn delete_entries(&self, keys: &[&str]) -> Result<(), String>;

    // 便捷封装
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn remove(&self, key: &str) -> Result<(), String>;
}

/// 按扩展名分发；未知扩展名报错（宁可报错也不猜错格式）。
pub fn open(path: impl Into<PathBuf>) -> Result<Box<dyn ConfigFile>, String>;
/// 显式指定格式（扩展名不可靠时用）。
pub fn open_as(path: impl Into<PathBuf>, format: Format) -> Box<dyn ConfigFile>;
```

### 4.3 格式与能力矩阵

```rust
pub enum Format { Xml, Properties, ShellEnv }
pub enum CommentStyle { Xml, Hash }   // 注释语法
```

| Format | 扩展名 | 注释样式 | 解析语义 | 更新保真策略 |
|---|---|---|---|---|
| `Xml` | `.xml` | `<!-- -->` | `<configuration>` 内 `<property><name>/<value>` | 文本级 patch：只替换命中 `<property>` 块，保留块间注释与其它块 |
| `Properties` | `.properties` `.props` `.conf` `.cfg` | `#` / `!` | `key=value` 行 | 行级替换命中行，保留注释/空行/顺序，新键追加末尾 |
| `ShellEnv` | `.sh` | `#` | `export KEY=VALUE` / `KEY=VALUE` 行（忽略注释行） | 行级替换，新键追加**文件末尾**（shell 后者生效） |

**ShellEnv 的键追加在末尾**：shell 是「后者覆盖前者」，追加末尾才能确保我们写的值生效；
说明块仍按统一规则在文件顶部，二者分离是有意的（见 §5.3 的剥离安全性讨论）。

**非键值文件不硬套格式**：hadoop 的 `workers` 是纯主机列表，不走 `ConfigFile`，
只做「确保非空」（官方模板自带 `localhost`，不覆盖用户改动），因此它不会有受管说明块。

**注释能力是显式能力位**：`comment_style()` 返回 `Option<CommentStyle>`，`None` 表示该格式无法承载注释（如未来的 JSON），此时**跳过**受管头部注入而不是报错。

> JSON / YAML / INI 等后续格式按同一模式加：实现 `ConfigFile` + 在 `Format::from_path` 注册扩展名 + 声明注释样式。JSON 的 `comment_style()` 返回 `None`。

---

## 5. 受管头部注释

### 5.1 目的

用户打开配置文件时，第一眼就知道：**这个文件被 SoloStack 管着、哪些行为会发生、手改会不会丢**。

### 5.2 内容

XML 样式：

```xml
<!-- SoloStack:begin · auto-managed block, do not edit -->
<!--
  本文件由 SoloStack 写入与管理。SoloStack 是 macOS 本地大数据组件管理器，
  它直接读写组件实例内的配置文件，不另存副本。

  写入语义：整体重写。SoloStack 每次在应用内修改配置，都会重写本文件中
  由它管理的键值。请勿手动编辑这些键值；你的改动会在下次应用内修改时被覆盖。
  需要长期保留的自定义配置，请记录在 SoloStack 之外。

  区块边界为下方 SoloStack:begin 至 SoloStack:end。删改标记会导致重新注入。
-->
<!-- SoloStack:end -->
```

Hash 样式（properties / shell_env）：

```
# SoloStack:begin · auto-managed block, do not edit
# 本文件由 SoloStack 写入与管理。SoloStack 是 macOS 本地大数据组件管理器，
# 它直接读写组件实例内的配置文件，不另存副本。
#
# 写入语义：整体重写。SoloStack 每次在应用内修改配置，都会重写本文件中
# 由它管理的键值。请勿手动编辑这些键值；你的改动会在下次应用内修改时被覆盖。
# 需要长期保留的自定义配置，请记录在 SoloStack 之外。
#
# 区块边界为上方 SoloStack:begin 至下方 SoloStack:end。删改标记会导致重新注入。
# SoloStack:end
```

### 5.3 注入规则

- **锚点位置**：XML 插在最后一个处理指令（`<?xml ...?>` / `<?xml-stylesheet ...?>`）之后；Hash 格式插在文件最顶部。
- **幂等**：每次写入先**剥离**已有区块（`SoloStack:begin` 到 `SoloStack:end`），再注入新块。重复写入不会堆积。
- **残缺标记只摘标记行，绝不连坐删内容**：只有 begin、只有 end、或 end 早于 begin 时，
  仅删除标记行本身，其它内容全部保留。这是安全底线 —— shell 环境文件的说明块在顶部、
  受管键在末尾，若沿用「只有 begin 就剥到末尾」的直觉规则，用户删掉一个 end 标记就会
  连带清空整份官方脚本。
- **文案不得含哨兵字面量**：说明正文里不能出现 `SoloStack:begin`/`SoloStack:end`，
  否则剥离逻辑会把说明文字误认成边界。XML 文案还不得含 `--`（非法注释）。两条都有测试守着。
- **解析时注释区不可见**（XML）：扫描前先把 `<!-- … -->` 区域屏蔽成等长空白，因此被注释掉的
  `<property>` 块既不会被当成生效配置读出来（否则探活会按一个不存在的端口探测），
  也不会被 `patch`/`remove` 改写或删除。把覆盖项注释掉是配置文件的常规用法，必须支持。
- **不注入的格式**：`comment_style() == None` 时跳过（不能往 JSON 里塞注释）。
- **不注入的文件**：SoloStack 从未写过的官方文件（如 `log4j.properties`、`capacity-scheduler.xml`）保持原样 —— 说明只出现在真正受管的文件里，避免误导。同理，`mapred-site.xml` 只在历史服务器**开启过**时才落笔删键，不会为「本来就没这个键」的文件白贴说明。
- 头部**不列举**具体受管键：键集合随配置变化，列举会让头部反复变动、制造 diff 噪音。头部只声明规则。

---

## 6. 写入语义与原子性

### 6.1 写入语义：合并 + 显式删除

| 语义 | 方法 | 用途 |
|---|---|---|
| 批量规划 | `ConfigPlan::set/remove/replace_text` | 组件将整组字段变更解析成文件计划 |
| 事务提交 | `config::apply_plan` | 安装、配置页保存、启动前补齐、JDK 写入统一走这一条 |
| 格式渲染 | `ConfigFile::render_entries` | 在内存中完成合并/删除，不直接写盘 |

底层只提供合并和显式删除，刻意不提供「整体重写」：安装时官方模板的
`<configuration>` 是空的，合并写等价于生成；而一旦允许整体重写，改一个端口
就会顺手抹掉模板里的注释与其它默认值。上层通过 `ConfigPlan` 聚合这些变更，
再统一交给 `apply_plan` 事务提交。

**对用户的承诺「受管键会被重写」是精确的**：应用内改配置会替换该键的值，缺失的受管键会被补齐；
未受管的键、注释、格式结构一律不动。头部的措辞与此一致，不夸大也不含糊。

### 6.2 配置文件是唯一事实源

安装选项**不另存**（没有 install.json、没有 install-config），全部落在官方配置文件里：

- 安装：`InstallConfig` → 算出生效值（含端口避让）→ 写进配置文件
- 启动前 / 打开配置页：**从配置文件精确读回**生效值 → 合并写回（幂等的自愈，不动已有值）
- 配置页改字段：直接改配置文件里对应的键

因此「手改配置」和「应用内改配置」作用于同一份数据，不存在两份事实打架。副作用是：
手动改过的端口**不会**被启动流程覆盖（启动读到的就是手改值，写回同一个值）。

**唯一的例外是「数据目录键」**（`dfs.namenode.name.dir` / `dfs.datanode.data.dir` / `log.dirs`）：
它们决定组件数据落在哪里，而卸载的 `keep_data`、`is_within_root` 等机制都建立在
`var/data/<组件>/<版本>/` 之下，因此**始终写入 SoloStack 受管路径**，不回读、不回声 ——
官方模板里的占位值（Kafka 的 `/tmp/kraft-combined-logs`）绝不能被采纳。
但**判断「是否已格式化」时仍读配置**（`configured_*`），并解析 `file://` 前缀、多目录与
相对路径（`config::resolve_path`），保证判断位置与组件实际落盘位置一致。

### 6.3 原子写

所有落盘统一走一个收口函数：

```rust
/// 剥离旧头部 → 注入新头部 → 同目录临时文件 + rename 原子替换。
fn commit(path: &Path, style: Option<CommentStyle>, body: &str) -> Result<(), String>
```

- 临时文件与目标**同目录**（避免跨设备 rename 失败），命名 `.<文件名>.solostack.tmp`。
- rename 前**继承原文件权限**（不被降级为默认 644）；失败时清理临时文件。
- 收益：进程崩溃/磁盘满不会留下被截断的配置文件 —— XML 被截断会让组件直接起不来。

### 6.4 一次保存的配置事务

配置页只调用一个入口：

```text
save_config_fields(component, updates[])
  → 组件 plan_field_updates()
  → ConfigPlan
  → config::apply_plan()
```

- 组件先校验全部字段，再把变更按文件聚合为 `ConfigPlan`；规划阶段不写盘。
- 同一个文件的多处修改只渲染一次、只提交一次，说明头也只注入一次。
- `apply_plan` 先完成全部文件渲染和同目录暂存，再依次替换正式文件。
- 任一提交失败时，使用事务开始前的文件快照恢复已经替换的文件。
- 恢复失败会作为高优先级错误返回，并由字段调度层写入 app 错误日志。
- 当前保证的是进程内失败回滚；崩溃级事务恢复需要后续引入 `.solostack.txn` journal。

---

## 7. 精确读：侧车文件的替代

### 7.1 探活端口

```rust
// component::ConfigLifecycle
/// 探活端口：从本组件配置文件读出**实际生效**的端口（单项读不到回退该项默认值，故不返回空）。
fn detect_ports(&self, version: &str) -> Vec<u16>;
```

端口 API 只剩这一个方法：

- 删除 `compute_detect_ports` —— 安装时的「算端口 + 占用避让」并入 `apply_install_config`，
  **写配置的地方就是决定端口的唯一地方**。
- 删除 `default_ports`（从 trait 降级为各组件私有常量）—— 兜底只发生在组件内部
  `Effective::from_config` 的单键回退里，框架不需要这个 API。
- 默认端口常量仍留在组件内（`NN_WEB_DEFAULT` 等），只在配置文件读不到该键时兜底。

各组件用一个 `Effective` 结构统一「安装时算出的」与「启动时读回的」两套值，
两条路径共用同一个写函数，并有测试断言 `install → 写盘 → 读回` 完全一致
（这正是能取消 `.detect-ports` 的前提）。

### 7.2 落点表

| 组件 | 配置目录（相对实例根） | 受管文件 | 探活端口来源 | JAVA_HOME 落点 |
|---|---|---|---|---|
| hadoop | `etc/hadoop` | `core-site.xml`、`hdfs-site.xml`、`yarn-site.xml`、`mapred-site.xml`、`hadoop-env.sh`、`workers`（纯列表） | `hdfs-site.xml`: `dfs.namenode.http-address`、`dfs.datanode.http.address`；`yarn-site.xml`: `yarn.resourcemanager.webapp.address`、`yarn.nodemanager.webapp.address`；`mapred-site.xml`: `mapreduce.jobhistory.webapp.address`（存在才探） | `etc/hadoop/hadoop-env.sh` 的 `JAVA_HOME` |
| kafka | `config` | `server.properties`（官方产物） | `server.properties`: `listeners` 里的 `PLAINTEXT` 端口 | `config/solostack-env.sh`（SoloStack 生成，见 §7.3） |

`fs.defaultFS` 固定用 NameNode RPC 端口 8020（**不是** WebUI 9870，两者混用会导致 HDFS 连不上）。
`dfs.namenode.secondary.http-address`（50090）固定、不参与探活。

### 7.3 JAVA_HOME

```rust
// component::ConfigLifecycle
/// Java 组件的 JAVA_HOME 落点（相对该组件配置目录的环境文件）。
/// 优先声明组件自带的官方环境文件；官方没有的（Kafka）由 SoloStack 生成一个。
fn java_env_file(&self) -> Option<&'static str> { None }
```

| 组件 | 落点 | 性质 |
|---|---|---|
| hadoop | `etc/hadoop/hadoop-env.sh` | **官方**环境文件，Hadoop 自己也读它 |
| kafka | `config/solostack-env.sh` | Kafka **没有**官方环境文件，由 SoloStack 生成（见下） |

**为什么 Kafka 要生成一个文件**：`bin/kafka-run-class.sh` 只用环境变量决定 java 可执行文件
（`if [ -z "$JAVA_HOME" ]; then JAVA="java"; else JAVA="$JAVA_HOME/bin/java"; fi`），
`bin/` 下没有任何 `source`/`kafka-env.sh` 约定 —— 即 **JAVA_HOME 在 Kafka 侧没有官方落盘位置**。
要持久化「用户选的 JDK」只能自己落一个文件。它与被废弃的 `.java-home` 的本质区别：

- **明文、可见、可读**：是一份正常的 `export KEY=VALUE` shell 文件（走 `Format::ShellEnv`），
  带受管说明头，用户可直接 `source` 它在命令行复现同一套 Java 环境；
- 隐藏的单值记录文件 `.java-home` 则既不可读也无从复用。

代价要说清楚：**Kafka 自己不读这个文件**，JAVA_HOME 是由 SoloStack 启动时读它、再注入子进程环境
（`component::exec::resolve_java_home`）。因此它只解决「持久化用户选择」，不改变 Kafka 的读取路径。

- 精确读：`config::ConfigFile::get(env_file, "JAVA_HOME")` → 反查本机 JDK 目录名，供配置页回显。
- 解析链（**全项目唯一一份**，在 `component/exec.rs`）：用户显式指定（安装时选的 JDK）
  → 环境文件里的 JAVA_HOME（路径已不存在则跳过）→ manifest `java_support` 支持版本里本机已装的
  → 任意本机 JDK（**仅运行期**）。
- 安装期与运行期的唯一差别是「找不到匹配 JDK 时是否兜底」：安装期报错（不让组件装在一个
  跑不起来的 JDK 上），运行期允许兜底到任意本机 JDK。此前两处各写一份，策略已经分歧。
- 删除 `apply_jdk_home` 钩子（Hadoop 写 `hadoop-env.sh` 的行为被通用化，不再需要组件专属钩子）。

---

## 8. 受影响的既有机制（删除清单）

| 删除对象 | 位置 | 替代 |
|---|---|---|
| `etc/` 配置副本目录 | `paths::{ETC_DIR, etc_dir, etc_instance_dir}` | 实例目录内的官方配置目录（`component::config_path`） |
| `.detect-ports` | `config::{read,write}_detect_ports` | `ConfigLifecycle::detect_ports` 精确读 |
| `.java-home` | `config::{read,write}_java_home` | 组件 env 文件精确读写（§7.3） |
| `RUNTIME_CONFIG_FILES` 补拷贝 | `config.rs` | 官方目录本来就完整，无需补 |
| `compute_detect_ports` / `default_ports` | component trait + hadoop/kafka | 并入 `apply_install_config` / 组件私有常量 |
| `apply_jdk_home` 钩子 | component trait + hadoop | 通用 env 文件读写 |
| `ConfigSource`（含复制文件清单语义） | component dto | `ConfigLayout`（仅目录 + 校验清单） |
| `conf.rs` | core | 拆为 `config/`（`mod` + `header` + `line` + 各格式），并改名消除与 `config.rs` 的歧义 |
| `config.rs` | core | 职责拆分到 `config` / `component` / `app::paths` |
| `config_schema.rs` | core | `component/schema.rs`（让 `config/` 保持零组件依赖） |

连带的简化：
- `InstallGuard` 不再需要单独清配置副本（配置就在实例目录内）；卸载同样少删一处。
- `ConfigLifecycle::ensure_config` 默认从「拷贝配置副本」变成「校验布局文件存在」，
  缺失即报错（而不是静默用空配置启动），并提示可重装修复。

迁移：`migration::migrate_app_layout()` 增加清理旧 `etc/` 目录（幂等），并继续清理历史 `installs/`、`snapshots/`、`.templates/`。

---

## 9. 待决 / 待核实

- [x] ~~**Kafka 的 `config/kraft/server.properties` 在 4.1.0 是否存在**~~ —— 已核实（2026-09-12，查官方 4.1.0 仓库）：
  `config/` 下**没有** `kraft/` 子目录，官方文件是 `config/server.properties`；`config_layout` 已改为 `config`。
- [x] ~~**Kafka KRaft 首启是否需要 `kafka-storage.sh format`**~~ —— 已确认需要，且已实现（2026-09-12）：
  Hadoop 与 Kafka 都把首启格式化实现在 `Runtime::init`，由 `service::start` 在启动前调用
  （与 hadoop 的 `namenode -format` 同一套生命周期时序）：
  `kafka-storage.sh random-uuid` → `format --standalone -t <uuid> -c <server.properties>`。
  **`--standalone` 不能省**：组合模式（`process.roles=broker,controller`）单节点未配 quorum voters 时，
  StorageTool 要求三选一（`--standalone` / `--initial-controllers` / `--no-initial-controllers`）。
  幂等信号用官方产物 `log.dirs/meta.properties`，不另造标记文件。
- [x] ~~**Kafka 的 JDK 选择无处持久化**~~ —— 已解决（2026-09-12）：生成 `config/solostack-env.sh` 作为落点，见 §7.3。
  顺带修正：4.x 的 quorum 引导键是 `controller.quorum.bootstrap.servers`（旧的 `controller.quorum.voters`
  在 3.9 起被 KIP-853 取代）；`advertised.listeners` 须与 `listeners` 端口同步，否则改端口后客户端会拿到旧端口。
- [x] ~~**Kafka 仍未实机装过**~~ → 2026-09-14 已完成真实安装和启停验证；源码/模板结论与实际运行一致。

### 9.1 清单维护（血的教训）

`package/manifest/<组件>.json` 里存的是**每个源各自的完整 URL 与 SHA256**，所以「某版本在某个源上还在不在」必须逐个核对：

- `dlcdn.apache.org` **只保留各版本线的最新补丁版**，旧版会被移出；清华镜像策略相同。
  实测（2026-09-13）：Kafka 4.1.0 在 dlcdn 与清华**双双 404**，而 4.1.2 / 4.2.1 / 4.3.1 两边都有。
- 因此**上架或改动版本时必须逐源 `curl -I` 核对**，并优先选「各源当前都还有的最新补丁版」——它留在镜像上的时间最长。
- 旧版本被镜像撤下后，清单只能改指向当前仍可用的版本（Apache 另有 `archive.apache.org` 永久归档，本项目未纳入清单）。
- [ ] `mapred-site.xml` 是否需要写 `mapreduce.framework.name=yarn`（当前未写，MapReduce on YARN 可能不完整）。
- [ ] 配置文件是否需要 checksum 校验以识别「用户手改过」，从而在覆盖前提示。
- [ ] 后续格式：JSON（无注释能力，`comment_style()` 返回 `None`）、YAML（`#`）、INI（`;`/`#`）按 §4.3 扩展。
- [ ] 多实例/多版本下的并发写入（当前每次写入都是「读-改-写」，同一文件无锁）。

---

## 10. 参考的 Rust 实践

| 参考 | 借鉴什么 | 不照搬什么 |
|---|---|---|
| `toml_edit` | **无损编辑**理念：编辑已知键、保留未知内容与注释（本设计对 XML 用文本级 patch、对行式格式用行级替换） | 不引入其 document/span 模型 —— XML/properties 的键值语义比 TOML 简单得多，`Entry` 向量足够 |
| `config` crate | `Format` 枚举 + 按来源分发、格式与数据源解耦 | 不做多层配置合并（bigdata 组件的配置文件是权威源，不需要 layering） |
| `tempfile` / `rustfmt` | 原子替换惯用法：同目录临时文件 + `rename` | 不引入 `tempfile` 依赖，实现只需 15 行；避免为小功能加 crate |
| `java-properties` | Java properties 语义（`#` 注释、`=` 分隔） | 不实现转义/续行等完整语义 —— 我们只写自己生成的键，读取容忍即可 |
| `quick-xml` / `roxmltree` | XML 解析正确性意识（本设计用最小文本扫描，仅识别 `<property>` 块） | 不引入解析器：需求是「改键值且保注释」，解析为 DOM 再序列化会丢注释，正是要避免的 |
| `serde` 生态 | 未来 YAML/JSON 格式直接复用 `serde_yaml`/`serde_json` 做解析 | 不在当前阶段为「可能的格式」预先抽象序列化层 |

**为什么自研而不直接用 crate**：需求是「对官方配置文件做最小侵入的键值编辑并保留其余内容」，而 XML 无损编辑在 Rust 生态没有成熟 crate（`toml_edit` 只覆盖 TOML），引入解析器反而会丢注释。自研部分被限制在一个 trait + 三个薄实现内。

---

## 11. 变更记录

| 日期 | 变更 |
|---|---|
| 2026-09-12 | 初稿：确立三原则、`config_file` 模块划分、`ConfigFile` 抽象、受管头部注释、精确读取代侧车文件；废弃 `etc/` 副本、`.detect-ports`、`.java-home`。 |
| 2026-09-12 | 后端工程化重组：`config_file/` → `config/`（拆出 `header.rs` / `line.rs`），字段调度移到 `component/schema.rs`，模块路径同步更新。 |
| 2026-09-13 | **代码审查后的修复批次**：数据目录键改为「写受管路径、判断读配置」（修 Kafka 数据落 /tmp）；端口校验拒绝 0 与特权端口、跨字段判重；JobHistory 关闭后改端口空操作；XML 注释区屏蔽；路径校验拒绝 `..`；下载改 `.part` + 成功后 rename；解压暂存目录改名（不再吃掉版本号）；JDK 策略收口到 `component/exec`；配置页打开时执行 `ensure_config`；清理死代码与过期注释。 |
| 2026-09-13 | **Kafka 上架版本 4.1.0 → 4.3.1**：4.1.0 已被 dlcdn 与清华镜像淘汰（双双 404，仅 archive 有），改为各源都还有的最新补丁版；`java_support` 由 `[11,17,21]` 修正为 `[17,21,25]`（官方 README：broker 最低 Java 17，仅 clients/streams 允许 11 —— 原清单会让用户选到装完起不来的 JDK 11）；新增结构不变量测试（各源版本集一致 + 每个版本都有 java_support）。 |
| 2026-09-12 | **Kafka 配置按 4.1.0 官方事实修正**：配置目录 `config/kraft` → `config`；quorum 键换为 `controller.quorum.bootstrap.servers`；受管 `advertised.listeners` 并与端口同步；端口解析支持带 host 形式（`PLAINTEXT://localhost:9092`）。新增 `solostack-env.sh` 作为 Kafka 的 JAVA_HOME 落点（§7.3），Kafka 恢复可配 JDK。 |
| 2026-09-12 | 定稿：砍掉 `write_entries`（只保留合并写）、`default_ports` 降级为组件私有常量、头部残缺标记改为「只摘标记行不连坐删内容」、明确 ShellEnv 键追加末尾、补「配置文件是唯一事实源」一节。实现完成，`cargo test` 78 通过。 |
