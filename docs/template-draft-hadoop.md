# hadoop.json 模板字段草案

基于 PRD 5.3 Schema，用 Hadoop 3.5.0 压测通用字段。标注每字段的用途与取值来源。

## 草案

```jsonc
{
  // ── 基本标识 ──
  "name": "hadoop",                     // 组件名（路径用，<组件>-<版本>）
  "version": "3.5.0",                   // 版本号
  "displayName": "Hadoop",              // GUI 显示名（可与人读的正式名不同）
  "category": "hadoop",                 // 分类（hadoop/kafka/flink/zk/jdk），用于归组

  // ── 架构与下载 ──
  "arch": ["aarch64"],                  // 支持的架构（MVP 仅 aarch64）
  "download": {
    "path": "hadoop/common/hadoop-3.5.0/hadoop-3.5.0-aarch64.tar.gz",  // 相对源 base URL 的路径
    "sha256": "",                        // 校验值，先留空（Apache aarch64 校验文件标错，需下载后回填）
    "archiveFormat": "tar.gz"            // 解压格式，决定解压器
  },

  // ── 依赖与运行 ──
  "jdkRequirement": "17",               // 需要注入的 JDK 版本（components/jdk/jdk-17）
  "defaultPorts": [9870, 9864],         // NameNode / DataNode 端口，预检+状态监控用

  // ── 配置 ──
  "config": {
    "sourceDir": "etc/hadoop",           // 组件解压目录内，模板配置的源位置
    "configFiles": [                     // 需要复制的配置文件（相对 sourceDir）
      "core-site.xml",
      "hdfs-site.xml",
      "yarn-site.xml"
    ],
    "templateFormat": "xml"              // 配置格式（xml/properties），决定解析/生成器
  },

  // ── 脚本 ──
  "scripts": {
    "start": "sbin/start-dfs.sh",        // 相对组件实例目录的启动脚本
    "stop": "sbin/stop-dfs.sh",          // 停止脚本
    "envFile": "etc/hadoop/hadoop-env.sh" // 需注入 JAVA_HOME 等的脚本（可选）
  },

  // ── 日志 ──
  "logDir": "logs",                     // 组件实例目录内日志相对路径（若不用 var/log）

  // ── Web UI ──
  "webUis": [                           // 原生 Web UI 地址模板
    { "name": "NameNode", "url": "http://localhost:9870" },
    { "name": "ResourceManager", "url": "http://localhost:8088" }
  ]
}
```

## 说明

### 字段按用途分四组
1. **标识**：`name` / `version` / `displayName` / `category` → 决定目录名、GUI 展示、归组
2. **下载**：`download.path` + `source` 源 → 拼 URL；`sha256` → 校验；`archiveFormat` → 解压
3. **运行**：`jdkRequirement` / `defaultPorts` → 预检与状态监控
4. **配置与脚本**：`config.*` → 配置副本机制；`scripts.*` → 启停调用

### 需要你拍板的分叉

**A. `download.path` 里带不带版本号？**
现在写死 `hadoop-3.5.0/` 在路径里。若将来同一模板想表达"该组件多个版本"，路径需参数化（如 `hadoop/common/hadoop-{version}/hadoop-{version}-aarch64.tar.gz`）。MVP 先写死（一个模板=一个版本），V1.1 多版本再参数化。**我建议先写死。**

**B. `sha256` 现在空着，下载后回填**
这有个副作用：**首次下载无法校验**（没有参照值）。两种处理：
- 下载时跳过校验，完成后本地计算并写回模板 → 第二次起才有校验。可接受，但首次无保护。
- 用 `.sha512` 文件（Apache 官方提供）做校验 → 更规范，但要实现 sha512 校验逻辑，且 aarch64 的 sha512 标错（见核实结果）。
**我建议 MVP 先实现 sha256，从 archive 人工核实真值后回填模板；** 或者退一步 MVP 先不校验，下载即用，校验留 V1.1。

**C. 配置副本目录放 `etc/` 后，`config.sourceDir` 意义**
`config/`（Homebrew 对齐后是 `etc/`）存放配置副本。模板里 `config.sourceDir` 指定**从组件解压目录哪里拷贝源配置**（`etc/hadoop/`），拷到 `etc/hadoop/hadoop-3.5.0/` 后再改。确认这个字段含义对。

### 我的建议倾向
A 写死版本、B 先用 sha256 人工回填、C 字段含义如上。但都等你确认。

Sources:
- [Apache Hadoop Download](https://hadoop.apache.org/releases.html)
- [Apache Archive hadoop 3.5.0 目录](https://archive.apache.org/dist/hadoop/common/hadoop-3.5.0/)
