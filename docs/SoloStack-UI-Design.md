# SoloStack 前端 UI 设计文档（Soft UI · 柔和界面风）

- 设计代号：**Soft UI 柔和界面风**（2026-08-23 起，替换 Minimalist Flat 极简扁平风；更早为「信标控制台 Beacon Console」深色风格）
- 首次成稿：2026-08-22
- 关联源码：
  - `src/routes/+page.svelte`（唯一页面，全部 UI）
  - `src/routes/+layout.ts`（SPA 配置 + 字体引入）
  - `src/app.html`（全局深色锁定、标题）
- 后端对接：`src-tauri/src/lib.rs`（Tauri 命令），`crates/core`（业务逻辑）

> 本文档是前端 UI 的**活文档**：对前端布局 / 样式 / 交互的任何调整，都需同步更新本文档。
>
> **沟通指位请用**：[`SoloStack-UI-Name-Reference.md`](./SoloStack-UI-Name-Reference.md)（UI 组件名称对照表，编号 T1…M3）。

---

## 1. 设计概念

SoloStack 是本地大数据组件管理器。UI 采用 **Soft UI 柔和界面风**：

- **大圆角（`14–28px`）**、**柔和彩色阴影**（带透明度、低饱和）、**浅色底**（slate-50 / 白卡片）。
- **低饱和主色（靛蓝 `#6366f1`）**，文字用灰阶（标题 gray-800、正文 slate-700、弱化 slate-500）。
- **悬浮上浮交互**：hover 卡片/按钮 `translateY(-2~4px)` + 阴影扩散，`transition 200–300ms ease-in-out`。
- **触感设计**：图标用圆形浅色底、active 轻微缩放（`scale-95`）按压反馈。
- 禁止纯黑、禁止硬边框（`border-black`）、禁止尖锐圆角、禁止高饱和纯色。

关键设计选择：

- **MVP 保持简单列表视图**（左节点列表 + 右详情），不做层级生态图。
- **低对比柔和**，亲和友好；尊重 `prefers-reduced-motion`。
- **零系统污染的产品性格**也体现在 UI 文案：安装/卸载/下载源处明确提示「不修改系统环境」。

---

## 2. 设计 Token

全部集中在 `+page.svelte` 的 `:global(.app)` CSS 变量块中，改设计先改这里。

### 2.1 颜色

| 变量 | 值 | 用途 |
|---|---|---|
| `--bg` | `#f8fafc` | 页面背景（slate-50 浅色） |
| `--panel` | `#ffffff` | 卡片 / 模态 / 顶栏底 |
| `--text` | `#334155` | 正文（slate-700） |
| `--text-hi` | `#1f2937` | 标题（gray-800） |
| `--muted` | `#64748b` | 弱化文字（slate-500） |
| `--primary` | `#6366f1` | **主色**（靛蓝 indigo-500） |
| `--primary-deep` | `#4f46e5` | 主色深（indigo-600，填充按钮） |
| `--accent` | `#f59e0b` | 强调（琥珀 amber-500，部分状态） |
| `--teal` | `#10b981` | 运行状态 |
| `--red` | `#ef4444` | 错误 / 危险 |
| `--blue` | `#3b82f6` | 链接 / 端口 / 路径信息 |
| `--line` | `#e2e8f0` | 细分隔线（slate-200） |

规则：**浅色底 + 低饱和主色 + 柔和彩色阴影**。大圆角（14–28px）、无硬边框、无纯黑、hover 上浮 + 阴影扩散，`transition 200–300ms ease-in-out`。

### 2.2 字体

| 变量 | 值 | 用途 |
|---|---|---|
| `--font-display` | `"Chakra Petch", "PingFang SC", sans-serif` | 标题 / 按钮 / 标签 / 数字 |
| `--font-mono` | `"SF Mono", "Menlo", ui-monospace, monospace` | 端口 / 版本 / 路径 / 日志 / 表单值 |

- **Chakra Petch** 由 `@fontsource/chakra-petch` 本地打包（离线可用），在 `+layout.ts` 引入 300/400/500/600/700。
- 中文正文走 `PingFang SC` 回退；日志等长文本用系统等宽（SF Mono / Menlo），不用 Chakra Petch。
- **图标库**：`lucide-svelte`（SVG、tree-shakable、跟随 `currentColor`）。当前用：顶栏齿轮 `Settings`、添加组件 `Plus`、返回 `ArrowLeft`、WebUI 跳转 `ExternalLink`、空态 `HardDrive`。

### 2.3 动效时长

| 场景 | 值 |
|---|---|
| 通用过渡 | `0.15s ease` |
| 页面/内容入场 | `0.25–0.4s` |
| 模态弹出 | `0.16–0.18s ease` |
| 状态灯呼吸 | `2.2s infinite` |

---

## 3. 全局

- **`app.html`**：`color-scheme: dark`，`html,body` 背景 `#070d17`、`overflow: hidden`（桌面 app 无页面滚动，滚动发生在分区内）。
- **背景质感**（`.app`）：
  1. 顶部偏右琥珀径向光晕 + 左下蓝色微光（`radial-gradient` ×2）
  2. 30px 细网格线（`linear-gradient` ×2，1px 线条、低透明度）
  3. `.app::after` 覆盖一层 SVG 噪点颗粒（`feTurbulence`，`opacity: 0.03`）
  - 内容容器（`.topbar/.content/.statusbar`）设 `z-index: 1` 浮于背景之上。

---

## 4. 布局结构（组件树）

```
┌─────────────────────────────────────────────────────────────┐
│ ① topbar 顶栏                                                │
│    brand（SoloStack + ▌光标） · spacer · 下载源 · arch 徽章      │
├──────────────┬──────────────────────────────────────────────┤
│ ② rail 左栏   │ ③ stage 右栏（详情控制台）                       │
│   rail-head   │   ③-1 detail-head 详情头                       │
│   node-list   │      title-row · meta-row · action-row · msg  │
│               │   ③-2 tab-bar（概览/配置/日志）                  │
│               │   ③-3 tab-pane                                │
│               │      overview │ config │ logs                 │
├──────────────┴──────────────────────────────────────────────┤
│ ④ statusbar 底部状态栏                                        │
└─────────────────────────────────────────────────────────────┘
浮层：install 安装模态 · uninstall 卸载确认模态
```

各区域类名即代码标识，调整时可直接按名定位。

---

## 5. 各部分设计明细

### 5.1 ① header 顶栏（`.topbar`）
- 高 56px，白底 + 细分隔线（Soft UI：浅色、无硬边框）。
- **brand**：纯文字 `SoloStack`（Chakra Petch 600）。
- **齿轮（H2）**：品牌紧右侧，圆形浅靛蓝底，点击进全局配置页（view=settings）。
- **添加组件按钮（H3）**：顶栏最右侧 "+"，圆形浅靛蓝底，点击进安装页（view=install）。
- 下载源切换入口在全局配置页 CFG3；架构信息在 footer F1。

### 5.2 ② main 组件列表（`.content` / `.comp-list`）
- main 是**组件列表**（不再左右分栏），只显示已安装组件（`list_component_templates` 已过滤 installed）。
- **M2 组件行**（`.comp-row`）：白卡片 + 大圆角 + 柔和阴影，hover 上浮 + 阴影扩散；flex 排列：
  - `led` 状态灯 + `comp-info`（`comp-name` 名称 + `comp-meta` 版本·状态）+ `row-actions` 右侧按钮组。
- **M2.5 操作按钮组**：运行中/部分 → [停止]；已停止 → [启动]；[配置] → CC 配置页；[日志] → CL 日志页。
- 无组件时显示空态（HardDrive 图标 + "点击右上角 + 安装组件"）。
- 列表 max-width 760px 居中；行入场 stagger `fadeUp`。

### 5.3 ③ footer 状态栏（`.statusbar`）
- 高 32px，浅底 + 上分隔线；从左到右：`arch` · `rootDir` · `sourceLabel` · spacer · `RUNNING` 计数 · `clock`。

### 5.4 浮层（模态）

- **卸载确认模态**（唯一保留）：`.overlay`（半透明遮罩 + 轻 blur）+ `.modal`（白卡片、大圆角、柔和阴影、pop 入场）。
  - 红标题 + 警告说明 + `keepData` 勾选（保留 `var/data` 持久数据）+ 取消 / 确认卸载。
  - 由组件配置页 CC5 [卸载组件] 触发；模态位于 app 根级（全局可用）。

### 5.5 组件配置页 / 日志页（独立整页）

- **组件配置页 CC**（view=comp-config）：`page-header`（返回 + `{组件名} · 配置`）+ 内容：
  - CC2 文件切换 chips → CC3 属性表单（`prop-name` + `prop-value` 输入）→ CC4 [保存配置] → CC5 [卸载组件]。
  - 复用 `loadConfig` / `saveConfig` / `selectConfigFile`。
- **组件日志页 CL**（view=comp-logs）：`page-header`（返回 + `{组件名} · 日志`）+ 内容：
  - CL2 工具栏（文件下拉 + 刷新 + 自动刷新 + 行数）→ CL3 日志框。
  - 自动刷新：仅 CL 页 + 组件运行/部分运行 + 开关开启，每 3s 拉取。

### 5.6 视图状态机（onboarding / main / settings / install / comp-config / comp-logs）

前端为单页 SPA，用 `view` 状态切换六个**独立整页**（`"onboarding" | "main" | "settings" | "install" | "comp-config" | "comp-logs"`）：

- **首次引导 Onboarding**（未初始化时）：全屏居中卡片，两步（O1 存放位置只读 + O2 下载源选择）→ `initialize_app(source_id)` → 主界面。
- **主界面 Main**：header + main 组件列表 + footer 三段式；齿轮 → settings，"+ " → install。
- **全局配置页 Settings**（view=settings）：page-header + CFG1–CFG4（存放位置 / 下载源 / 关于）。
- **安装页 Install**（view=install）：page-header + **组件横条 tab（INST1，顺序写死 hadoop/kafka）** + 下载源/版本（来自 config json）+ **JDK 必选**（显示本机所有 JDK，仅 `java_support` 支持的可选，其余置灰）+ [开始安装]。**无端口配置**。
- **组件配置页 CC / 日志页 CL**（view=comp-config / comp-logs）：由 M2.5 [配置]/[日志] 进入，返回回主界面。

---

## 6. 交互与动效

| 场景 | 实现 |
|---|---|
| 列表入场 | 逐项 stagger `fadeUp`（38ms 间隔） |
| 状态灯 running | 青绿呼吸（`breathe` 2.2s，opacity 1↔0.45） |
| 状态灯 error | 红色闪烁（`blinkFast` 0.85s steps） |
| 状态灯 stopped | 静态灰 |
| 状态灯 not_installed | 暗灰虚线环 |
| 按钮 hover | 琥珀边框 + 文字 + 微光 + `translateY(-1px)` |
| 主按钮 hover | 琥珀 glow 加强 |
| 危险按钮 hover | 红色 glow |
| tab 切换 | 内容区 `fadeUp` 淡入 |
| 模态 | `overlay` fadeIn + `modal` pop（scale + translateY） |
| 品牌光标 | `blink` 1.1s steps（闪烁） |
| 空态 | `▓▒░` 字符呼吸动画 + 引导文案 |

---

## 7. 前端状态与后端命令映射

### 7.1 前端状态（Svelte 5 runes，`+page.svelte` script）

| 状态 | 说明 |
|---|---|
| `components: UiComponent[]` | 模板列表 + 实时状态（含 `installed`） |
| `selectedName` | 当前选中组件 |
| `tab` | `overview` / `config` / `logs` |
| `arch` / `rootDir` / `sourceId` / `sourceLabel` | 全局环境信息 |
| `configFile` / `configProps` | 配置 tab 当前文件与属性 |
| `logFiles` / `activeLog` / `logContent` / `logLines` / `autoRefresh` | 日志 tab |
| `dirs` | 概览目录路径 |
| `showInstall` / `showUninstall` / `keepData` / `installBusy` | 模态与安装中 |
| `errorMsg` / `successMsg` / `busy` | 提示与操作锁 |

轮询：启动后每 6s 刷新组件状态；时钟 1s；日志自动刷新 3s（条件触发）。

### 7.2 调用的 Tauri 命令

| 命令 | 用途 |
|---|---|
| `list_component_templates` | 组件模板 + 安装状态 |
| `get_component_status` | 运行状态（结构化：整体 `status` + 各 `services` 状态） |
| `start_component` / `stop_component` | 启停（`service` 可选：指定服务，空则全启/逆序全停） |
| `install_component` / `uninstall_component` | 安装（配置驱动：component+version+ports）/ 卸载（后台线程） |
| `get_component_config` / `set_component_config` | 配置读写 |
| `list_component_logs` / `read_component_log_tail` | 日志列表（按 `service` 过滤）/ tail |
| `get_component_dirs` | 托管目录路径 |
| `get_arch` / `get_root_dir` / `get_download_source(_label)` / `set_source` | 环境与下载源 |
| `get_init_status` / `initialize_app` | 首次引导：初始化状态判断 / 完成初始化（建目录+写源文件+默认源） |
| `list_sources` / `get_settings` | 预置源列表 / 当前设置（配置页回显） |

---

## 8. 已知事项 / TODO

- [ ] **安装进度**：当前安装是阻塞 loading（spinner），无下载进度条；后续可在 core 加进度回调经事件推送给前端。
- [x] **YARN 接入**（2026-08-22）：模板引入 `services` 抽象，hadoop 拆分 hdfs/yarn 两服务，可分别/全部启停与状态检测；端口聚合 `[9870, 9864, 8088, 8042]`。
- [ ] **日志归集**：当前读组件实例内 `logs/`；按 PRD 规范应把 `HADOOP_LOG_DIR` 注入到 `var/log/<组件>-<版本>/`。
- [ ] **卸载确认强度**：当前仅一个确认勾选；可考虑危险组件二次输入组件名确认（V1.1 评估）。
- [ ] **前端组件库**：MVP 用原生样式，V1.1 再评估 shadcn-svelte。
- [ ] **多组件模板**：目前内置模板仅 hadoop；Kafka/Flink/ZK 接入后需扩展分类与品牌色映射。

---

## 9. 变更记录

| 日期 | 版本 | 说明 |
|---|---|---|
| 2026-08-22 | v1.0 | 初稿：信标控制台整体设计 + MVP 闭环（安装/卸载/配置/日志/下载源）落地，与代码一致 |
| 2026-08-22 | v1.1 | T 区简化：移除品牌闪烁光标、下载源弹层、架构徽章；下载源入口待移至别处（后端命令保留），架构改由 S1 状态栏展示 |
| 2026-08-22 | v2.0 | D/V 区重构 + YARN 真实接入：引入服务（Service）抽象，hadoop 拆 HDFS/YARN 分段显示；D 区瘦身（去重复按钮、端口下沉到服务卡）；V1.5 服务分段栏、V3 服务状态卡、日志按分段过滤；状态新增 partial |
| 2026-08-22 | v3.0 | 视图状态机（onboarding/main/settings）：首次引导 O1 存放位置（默认路径只读）+ O2 下载源选择；全局配置页 CFG1–CFG4（存放位置/下载源/关于）；T2 齿轮按钮；下载源文件化（`sources.json`，SoloStack 维护，用户不可自定义）；`set_download_source` 更名 `set_source` |
| 2026-08-22 | v3.1 | 顶栏调整：T2 齿轮移到品牌紧右侧；新增 T3 添加组件按钮（最右侧 "+"，弹"添加组件"模态） |
| 2026-08-22 | v3.2 | 引入 lucide-svelte 图标库替换字符图标（齿轮/加/返回/外链/空态） |
| 2026-08-22 | v4.0 | 安装流程重构：侧边栏只显示已安装（推翻模板货架）；独立安装页（组件/版本下拉联动下载源 + 端口配置）；配置驱动安装（install.json、configgen 端口参数化、状态检测用实际端口）；移除添加/安装模态 |
| 2026-08-22 | v4.1 | 视图改为独立整页：settings/install 各带 `page-header`（返回 + 标题），不再共享主界面顶栏；主界面 topbar 恒为品牌+齿轮+加号 |
| 2026-08-23 | v5.0 | **风格迁移为 Minimalist Flat 极简扁平风**：白底黑字 + 琥珀强调、锐利圆角、`2px` 黑边框、无阴影/渐变/玻璃态、hover 高对比反色、瞬时过渡；保留全部类名与布局结构 |
| 2026-08-23 | v6.0 | **风格迁移为 Soft UI 柔和界面风**：低饱和靛蓝主色、大圆角、柔和彩色阴影、卡片上浮悬浮交互、圆形图标底、浅色底；移除硬边框/纯黑/锐利圆角；保留全部类名与布局结构 |
| 2026-08-23 | v7.0 | **主页布局重构**：header/main/footer 三段式语义结构；main = 组件列表（M2 行 + M2.5 启停/配置/日志按钮）；配置/日志抽为独立整页（CC/CL）；移除 rail/stage/tab/服务卡；暂去掉服务级（HDFS/YARN 单独启停） |
| 2026-08-23 | v8.0 | **安装页重构 + kafka 接入**：组件改为横条 tab（hadoop/kafka，每 tab 配置不同）；下载源改为安装时选择（引导/配置页去源，`install_component` 加 `source_id`）；版本随源联动；后端新增 kafka 源/模板/简化 KRaft 配置 |
| 2026-08-23 | v9.0 | **config json 驱动 + 删端口配置**：下载源/JDK 支持迁至 `config_defs/*.json`（每组件自带 source + java_support）；组件顺序前端写死；JDK 必选（显示所有本机 JDK，仅 java_support 支持的可选）；删除安装页端口配置 UI 与后端端口逻辑（默认端口） |
| 2026-08-23 | v10.0 | **安装流程重构**：临时 install.json（`installs/<组件>-<4位Id>-install.json`，装完删）；探活端口自动避让（默认被占用 +1/+2）；hadoop 可配置两个 WebUI 端口（NameNode/YARN RM）；JAVA_HOME 写 `hadoop-env.sh`/`.java-home`；探活轮询 10s |
| 2026-08-23 | v10.1 | **安装进度页**：点击安装跳 `install-progress` 页；后端 `install` 发进度事件（下载字节/总量、解压、配置、完成），Tauri `emit "install-progress"`，前端监听显示进度条 + 节点日志，完成后回主界面 |
