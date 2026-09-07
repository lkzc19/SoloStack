# SoloStack 产品需求文档（PRD）

- 版本：V1.0 MVP（修订版 Rev.12）
- 适配平台：macOS，优先 Apple Silicon（M 系列）。当前开发机即 Apple Silicon，MVP 在 aarch64 上原生验证
- 核心模式：纯本地进程管理 · 无虚拟化 / 无容器 / 无全局环境污染
- 产品 Slogan：Local BigData Stack Manager for macOS
- 更新时间：2026-08-19

---

## 一、项目背景与痛点

### 1.1 现有痛点

- 大数据开发本地测试需手动下载、解压、配置、启停 Hadoop、Kafka、Flink、Zookeeper 等组件，步骤繁琐、命令记忆成本高。
- Docker / 虚拟化方案在 Apple Silicon Mac 上适配差（Hadoop 官方镜像无 ARM 原生支持），且虚拟化资源开销大，不符合"本地快速验证 ETL 逻辑"的轻量诉求。
- 环境无隔离、无版本管理：换设备或环境变更后配置失效，无法一键复刻环境。
- 无统一可视化管理入口：启停、看日志、改配置、卸载分散在多个目录和脚本里，长期不操作后容易遗忘。
- 传统大数据运维工具（CDH、Datasophon 等）面向线上集群，过重且不适配本地开发场景。

### 1.2 项目初衷

原名 DevNest、CanoeStack，现定名 **SoloStack**。SoloStack 是一款 macOS 桌面端大数据组件本地管理器，让开发者**在本地一键恢复一套可运行的大数据测试环境**：从下载、配置、启停、看日志到卸载，全部可视化完成，彻底摆脱"上云才能做功能性测试"的依赖。核心目标是解决**本地环境可复现**与**操作零门槛**两个问题。

---

## 二、产品核心定位

### 2.1 核心定义

SoloStack 定位为 **本地大数据环境管家**，而非组件发行商。它**不自研、不自托管组件二进制包**，而是围绕官方组件包提供一套统一的"注册 → 配置 → 启停 → 日志 → 卸载 → 快照复刻"管理闭环。用户指定下载地址或本地已有目录，SoloStack 负责后续所有繁琐操作。

### 2.2 核心约束（关键差异化）

- **纯本地进程管理**：无 Docker、无虚拟机、无 K8s，所有组件以本地进程直接运行。
- **零系统全局污染**：不修改 `~/.zshrc` / `~/.bash_profile` 等全局配置，环境变量仅对托管进程临时注入，退出即失效。
- **环境与版本隔离**：所有组件、JDK 统一收纳在 `~/.solostack/` 下，多版本独立目录，互不冲突。
- **极简卸载**：卸载即删除组件完整安装目录并清理进程，无残留文件、缓存、僵尸进程。
- **环境可复刻**：一键导出/导入环境快照，换设备后恢复一致的可运行环境。
- **Mac 架构感知**：MVP 聚焦 aarch64（Apple Silicon）；自动识别当前架构，下载、校验、运行前给出架构适配提示与指引。架构感知逻辑在 core 层统一实现，为后续兼容 x86_64 预留扩展点。

### 2.3 目标用户与验证基准

- **目标用户**：macOS 平台的大数据开发工程师、大数据学习者：日常需要在本机测试 Hadoop、Kafka、Flink、Zookeeper 组件，受限于 Mac 适配、命令行繁琐、环境不可复刻等问题。
- **开发机与验证基准**：当前开发机为 **Apple Silicon（M 系列）**，MVP 聚焦 **aarch64** 架构，在 aarch64 上完成开发与原生验证。架构感知逻辑仍在 core 层统一实现，为后续兼容 x86_64 预留扩展点，但 MVP 仅支持 Apple Silicon。
- **演进节奏**：先满足开发者本人需求，跑通闭环后再对外发布、让其他开发者可用。

---

## 三、核心功能需求

> 以下为 MVP 的目标范围，按价值排序。各模块详细验收标准见文末"验收与边界"。

### 3.1 组件市场与本地注册

- 内置四大核心组件元数据：Zookeeper（可选）、Kafka、Flink、Hadoop。
- 组件元数据以 JSON 模板描述，包含：架构适配建议、推荐版本、下载相对路径、SHA256、启停脚本路径、配置文件路径、日志目录、默认端口、依赖 JDK 版本。
- **本地注册**：支持两种来源 —— ① 用户提供官方包下载地址，工具下载并校验后自动解压；② 用户指定本地已有的解压目录，工具直接接管。
- 自动识别当前 Mac 架构（MVP 聚焦 aarch64 / Apple Silicon），下载前给出架构匹配校验与提示。
- 多版本共存：同一组件可安装多个版本，互不干扰。
- 安装包 SHA256 校验，防篡改、防下载损坏。

### 3.2 可视化配置管理

- 图形化表单编辑组件核心配置，自动映射生成 `xml` / `properties` 等目标格式文件。
- 内置各组件官方配置模板、默认参数、端口规范。
- 支持配置预览、一键重置默认、保存生效。
- Hadoop 专属：可视化配置 `core-site.xml`、`hdfs-site.xml`、`yarn-site.xml` 核心参数。
- 保存配置后即时提示是否需要重启进程生效。

### 3.3 进程启停与状态监控

- 调用组件原生启停脚本，实现一键启动 / 停止 / 重启。
- 实时检测进程存活、端口占用、状态变化。
- 启动前预检：端口冲突检测、JDK 版本校验、配置合法性检查，异常弹窗提醒。
- 软件退出时自动销毁全部托管进程，杜绝僵尸进程残留。
- 一键打开组件原生 Web UI（Hadoop NameNode / YARN ResourceManager、Flink Web UI 等）。

### 3.4 实时日志管理

- 自动监听组件 logs 目录，实时滚动输出日志。
- 支持日志搜索、筛选、清空、导出。
- 区分运行日志与错误日志，快速定位启动异常。

### 3.5 纯净卸载管理

- 一键卸载指定组件 / 指定版本，递归删除安装目录。
- 同时清理对应托管进程与缓存文件，卸载后本机环境无残留。

### 3.6 内置 JDK 版本管理

- 自动下载并托管组件所需 JDK 版本，组件与 JDK 独立绑定，版本隔离不冲突。
- 仅为组件进程临时注入 JDK 环境变量，不修改系统全局 JDK。
- 若用户本机已存在兼容 JDK，允许手动指定，避免重复下载。

### 3.7 环境快照（复刻）

- 一键导出当前环境快照（JSON）：组件名称、版本、架构、配置、端口、依赖 JDK、数据目录说明。
- 一键导入快照：在新设备上按清单自动下载 / 校验 / 解压 / 写入配置，恢复到一致的可运行状态。
- 数据目录（如 HDFS NameNode/DataNode 存储、Kafka log.dirs）默认不随快照迁移，导入时提示重新初始化或格式化。

### 3.8 命令行工具（CLI）—— 一等公民

SoloStack 的 CLI 与 GUI **平级交付**，是正式产品形态，而非内部调试工具。核心逻辑（`crates/core`）由 GUI 与 CLI 共享，两端的启停、状态、配置、日志行为一致。

- **三种获取渠道**：
  1. **随 dmg 安装**：`SoloStack.dmg` 装完的 app 内嵌 CLI 二进制。
  2. **GitHub Releases 独立包**：单独发布 `solostack` 命令行压缩包（zip / tar.gz），解压即用，无需安装 app。
  3. **Homebrew 渠道（后期）**：提供 formula，`brew install` 一条命令安装。
- **PATH 接入**：安装 app 后，由 **app 首次启动引导用户配置**是否将 `solostack` 命令接入终端 PATH、以及链接到哪个位置（如 `~/.local/bin`），不静默修改任何系统文件；单独下载的 CLI 由用户自行放置到 PATH。
- **功能范围（MVP 最小可用）**：CLI 先覆盖**组件启停与状态查询**，作为开发与排查工具；配置编辑、快照、日志等重交互功能 V1.1 起逐步补齐，与 GUI 对齐。
- **内部一致性**：`crates/cli` 与 GUI 共用 `crates/core`，保证同一套下载 / 校验 / 配置 / 进程管理逻辑，行为不分裂。

### 3.9 下载源管理

- 组件、JDK 的下载地址不直接写死官方 URL，而是采用 **源（base URL） + 相对路径** 拼接：
  - 组件模板存相对路径（如 `hadoop/common/hadoop-3.5.0/hadoop-3.5.0-aarch64.tar.gz`）；
  - 下载时取当前生效的源 base URL 拼接成完整地址。
- **内置源**：
  - **官方源（默认）**：Apache / 各组件官方站。
  - **清华 TUNA 镜像**：`mirrors.tuna.tsinghua.edu.cn`，国内同步及时、覆盖全。
  - **自定义源**：用户可手动填入任意 base URL，适配任意镜像站 / 私有源。
- **切换方式**：手动选择（MVP），用户可在 GUI 设置页或 CLI 中切换；切换对 Hadoop / JDK / Kafka / Flink 全部生效，无需改动组件模板。
- **持久化**：当前生效的源选择写入 `~/.solostack/` 下的用户设置文件，下次启动沿用。
- **默认官方**：首次使用默认官方源，用户可按需切换到国内镜像或自定义源。
- 自动测速选源留作 V1.1+ 迭代规划（需要探测、超时、误差处理，MVP 不做）。

---

## 四、迭代规划

### V1.0 MVP（优先落地）

- **Hadoop 3.5.0 单机（伪分布式）** 跑通最小闭环：注册 → 下载/校验 → 配置 → 启停 → 日志 → 卸载 → 快照。
  - MVP 聚焦 Apple Silicon：下载官方 3.4+ aarch64 包（官方已支持 Apple Silicon 构建）并原生验证。
  - 伪分布式涉及：`core-site.xml` / `hdfs-site.xml` / `yarn-site.xml` 写入、JDK 17 临时注入、`hdfs namenode -format`（带重复格式化保护）、NameNode / DataNode 启动与端口健康检查（9870/9864）。
  - 若对旧版本（3.1.x / 3.2.x）有需求，仅提供社区 ARM 适配包的指引入口，不自行维护编译流水线。
- 基础 JDK 版本自动适配、端口预检、进程守护。
- **下载源管理**：官方（默认）+ 清华 TUNA + 自定义源，手动切换，GUI 设置页 / CLI 均可操作。
- 极简 UI：组件实例卡片 + 核心操作入口。
- **CLI MVP 最小可用**：`solostack start/stop/status <component>` 可独立下载使用（GitHub Releases），并与 GUI 共用 core。
- 环境快照（单组件级）落地，验证"换机复刻"体验。
- 前端组件库 MVP 阶段暂不引入，用 Svelte 原生样式验证闭环，V1.1 再评估 shadcn-svelte / Skeleton UI。

### V1.1 功能迭代

- 接入 Kafka（KRaft 单机模式），支持启停、Web UI 跳转。
- 完善日志检索与配置模板；支持 Zookeeper 作为独立可选组件。
- **CLI 补齐配置、日志、快照子命令**，与 GUI 功能对齐。
- **Homebrew 渠道**：发布 `solostack` formula。
- 快照升级为多组件组合导出 / 导入。
- 评估并引入前端组件库（优先 shadcn-svelte）。

### V1.2 核心难点突破

- 接入 Flink，支持 Session 集群启停、Web UI 跳转。
- 适配 Apple Silicon 上的 Hadoop：接入官方 3.4+ 版本，引导式完成伪分布式初始化、SSH 免密配置、格式化操作。
- HDFS 数据目录与快照导入的冲突处理。

### V2.0 进阶增强（可选）

- HDFS 可视化文件浏览器（封装原生 `hdfs` 命令）。
- Kafka 可视化消息生产 / 消费面板。
- 组件升级路径管理、健康巡检报告。

---

## 五、技术架构规范

### 5.1 技术栈

- 桌面框架：**Tauri**（轻量、macOS 权限友好、dmg 打包）。
- 后端核心：**Rust**（进程管理、文件处理、下载解压、日志监听、配置解析、下载源管理）。采用 workspace 结构：`src-tauri`（GUI）+ `crates/core`（核心逻辑，GUI 与 CLI 共享）+ `crates/cli`（命令行工具，与 GUI 平级交付）。
- 前端：**Svelte**（编译型、产物轻、贴合轻量定位）。MVP 阶段不绑定组件库，用原生样式；V1.1 再评估 shadcn-svelte / Skeleton UI。
- 配置解析：原生解析 `xml` / `properties`，组件配置以 JSON 模板规范描述。

### 5.2 目录统一规范

所有组件统一安装在用户主目录下的隐藏目录 `~/.solostack/`：

```text
~/.solostack/
├─ settings.json              # 用户设置（下载源、PATH 等）
├─ components/                # 组件解压目录（纯净，可重建；对应 Homebrew Cellar）
│  ├─ hadoop/hadoop-3.5.0/    # 实例目录：<组件>/<组件>-<版本>，多版本共存
│  └─ jdk/jdk-17/             # JDK 也是组件，同样收纳在 components/ 下
├─ etc/                       # 配置副本（对应 Homebrew etc/）
│  └─ hadoop/hadoop-3.5.0/    # core-site.xml / hdfs-site.xml / yarn-site.xml
├─ var/                       # 可变运行时数据（对应 Homebrew var/）
│  ├─ data/hadoop/hadoop-3.5.0/  # 持久数据（HDFS 存储，快照不迁移）
│  ├─ log/hadoop/hadoop-3.5.0/  # 运行日志（对应 var/log/）
│  └─ run/hadoop-3.5.0.json     # 进程 PID 记录（对应 var/run/）
├─ downloads/                 # 下载缓存（tar.gz，保留可复用，用户可手动清理）
├─ .templates/                # 组件元数据 JSON 模板（用户可扩展）
└─ snapshots/                 # 快照导出/导入
   └─ solostack-20260819.json
```

**设计对齐 Homebrew：**

| SoloStack 目录 | 对齐 Homebrew | 职责 |
|---|---|---|
| `components/` | `Cellar/` | 安装物（版本化目录，纯净可重建） |
| `etc/` | `etc/` | 配置文件（我们的配置副本） |
| `var/data/` | `var/lib` | 持久运行时数据（HDFS 存储等） |
| `var/log/` | `var/log/` | 运行日志 |
| `var/run/` | `var/run/` | 进程 PID / 状态记录 |
| `settings.json` | `var/homebrew/` 等 | 自身状态 |

**核心设计原则：**

- **组件目录纯净**：`components/` 只存放解压后的官方包；SoloStack 生成的配置不写入组件内，而是复制到 `etc/` 目录，运行时通过环境变量（如 Hadoop 的 `HADOOP_CONF_DIR`）指向。卸载 = 删除组件目录，无残留。
- **实例标识统一**：组件实例目录统一为 `<组件>-<版本>`（如 `hadoop-3.5.0`），多版本共存、互不干扰，是版本隔离的物理基础。
- **JDK 作为组件**：JDK 与普通组件同构（同样需要下载、校验、解压、版本管理），统一收纳在 `components/jdk/jdk-<版本>/` 下，不做独立目录；复用同一套组件管理逻辑，仅在"仅注入环境变量、不常驻进程"上与普通组件有别。
- **可变数据集中**：运行时数据统一收在 `var/` 下（`var/data` 持久数据、`var/log` 日志、`var/run` 进程记录），与安装物（`components/`）分离——升级、重装、卸载组件都不影响已有数据。
- **数据与组件分离**：HDFS NameNode/DataNode 存储等持久数据放 `var/data/`，快照默认不迁移数据，导入时提示重新初始化或格式化。
- **下载缓存保留**：`downloads/` 保留安装包，重装 / 多版本复用可跳过重复下载；提供手动清理能力（UI 入口 V1.1 起完善）。
- **快照与模板独立生命周期**：`snapshots/` 与 `.templates/` 不随组件卸载删除。

**卸载时的目录处理：**

| 目录 | 卸载组件时 | 说明 |
|---|---|---|
| `components/<组件>-<版本>/` | 删 | 组件本体 |
| `etc/<组件>-<版本>/` | 删 | 配置副本，随组件 |
| `var/run/` 对应记录 | 删 | 进程记录 |
| `var/log/<组件>-<版本>/` | 删 | 运行日志，随组件 |
| `var/data/<组件>-<版本>/` | 可选保留 | 提示"是否保留数据" |
| `downloads/` 对应安装包 | 默认保留 | 用户可手动清理 |
| `snapshots/` | 永不删 | 独立于组件生命周期 |
| `.templates/` | 永不删 | 用户扩展模板 |

### 5.3 组件模板规范（JSON Schema 核心字段）

```jsonc
{
  "name": "kafka",
  "arch": ["aarch64", "x86_64"],    // 支持的架构列表
  "version": "4.x",
  "downloadPath": "kafka/4.1.0/kafka_2.13-4.1.0.tgz",  // 相对源 base URL 的路径（含文件名）
  "sha256": "...",                  // 安装包校验值
  "jdkRequirement": "17",           // 依赖 JDK 版本
  "defaultPorts": [9092],
  "configFiles": ["config/server.properties"],  // 需托管的配置文件
  "configTemplate": "properties",   // 配置格式映射
  "startScript": "bin/kafka-server-start.sh",
  "stopScript": "bin/kafka-server-stop.sh",
  "logDir": "logs",
  "webUis": []                      // 原生 Web UI 地址模板
}
```

下载完整地址 = 当前生效的源 base URL + `downloadPath`（如清华镜像则 `https://mirrors.tuna.tsinghua.edu.cn/apache/kafka/...`）。`downloadPath` 需保证官方源与镜像源的目录结构一致（镜像同步 Apache 目录树）。

组件元数据模板与内置模板同构、可被用户扩展：MVP 版本内置模板随应用分发（只读）；后续版本允许用户向 `~/.solostack/.templates/` 添加自定义模板，注册新组件（同 Schema 即兼容）。

### 5.4 进程管理设计

- 统一记录组件 PID 与启动命令到 `~/.solostack/.runtime/`，供状态查询、停止、异常清理使用。
- 启动采用"临时环境变量注入"方式：子进程继承注入后的环境，不修改任何全局配置文件。
- 应用退出钩子统一清理托管进程；支持端口级强杀兜底。
- CLI 与 GUI 共用同一套进程管理逻辑（`crates/core`），终端运行 CLI 触发启停与在 GUI 中操作行为一致。

### 5.5 CLI 分发设计

- **单一二进制**：`crates/cli` 产出单一 `solostack` 可执行文件，不依赖运行时。
- **内嵌 dmg**：app 打包时把 CLI 二进制放入 `.app/Contents/MacOS/` 同级或 Resources 目录，随应用分发。
- **GitHub Releases**：CI 构建产出 `solostack-<version>-<arch>.zip` / `.tar.gz`，解压即用（含 `*.sh` 执行权限）。
- **PATH 引导**：app 首次启动时引导用户选择是否、以及如何将 `solostack` 接入 PATH（提供 `~/.local/bin` 等建议位置），不静默写入系统文件。

---

## 六、核心避坑规范（Mac 专属）

- **Hadoop 不采用自研 ARM 编译包**：优先使用官方 3.4+ 版本（官方已支持 Apple Silicon 构建）；仅对确需旧版本的用户提供社区适配包指引，不自行维护编译流水线。
- **Kafka 默认 KRaft 模式**：Kafka 4.x 已移除 ZooKeeper 依赖，单进程即可运行，避免引入废弃架构。
- **进程管控**：全程记录 PID，应用退出强制 kill 托管进程，杜绝后台僵尸进程。
- **权限自动修复**：解压后自动为 `*.sh` 脚本赋予执行权限，无需用户手动操作。
- **版本强隔离**：组件、JDK 均独立版本目录，杜绝版本冲突。
- **无系统污染**：所有环境变量仅对进程临时生效，不修改任何系统全局配置文件。

---

## 七、验收与边界

### 7.1 MVP 验收标准（以 Hadoop 伪分布式为例）

1. 首次使用：从注册到成功启动，全程 GUI 完成，无需打开终端。
2. 停止、重启、状态检测准确；退出应用后无残留 Hadoop 进程。
3. 端口冲突 / JDK 缺失 / 配置非法时给出明确提示而非静默失败。
4. 日志实时滚动、可搜索、可导出。
5. 卸载后 `~/.solostack/hadoop/` 对应目录与进程、缓存完全清理。
6. 快照导出 → 在另一台设备导入 → 启动成功（含 JDK 自动补齐）。
7. **CLI 可用**：`solostack start hadoop` / `stop` / `status` 在终端可用，行为与 GUI 一致；GitHub Releases 单独下载的 CLI 解压后无需 app 即可使用。
8. **下载源切换**：默认官方源；切换到清华 TUNA / 自定义源后，Hadoop / JDK 下载走新源，无需改组件模板；切换选择持久化，重启沿用。

### 7.2 明确不做（Out of Scope）

- 不做线上集群、高可用、生产运维能力（非本地单机测试场景）。
- 不自研、不自托管组件二进制发行（含 Hadoop ARM 包）。
- 不做多机分布式编排。
- **不做 GUI 专属逻辑**：CLI 与 GUI 必须共用 core，不做仅 GUI 能用的功能（避免行为分裂）。

---

## 八、项目命名

- 项目正式名：**SoloStack**
- 仓库名：`solostack`
- 安装包名：`SoloStack.dmg`
- CLI 命令名：`solostack`
- 应用 identifier（Bundle ID）：`xyz.lkzc19.solostack`
- 命名避坑：全程规避 Hive、Cluster 等易歧义词汇，纯本地开发工具定位清晰。

---

## 九、对外发布（后续阶段，非 MVP 范围）

- 产品成熟前由开发者本人使用，跑通闭环并稳定后再考虑对外发布。
- 对外发布前需要补充：README 使用说明、图标与品牌资源、签名与公证（notarization）、dmg 打包、隐私与权限说明。
- 组件元数据模板的扩展文档，供第三方组件接入。
- CLI 分发基建：GitHub Releases 自动化（构建、校验、上传独立包）、Homebrew formula（V1.1）。
