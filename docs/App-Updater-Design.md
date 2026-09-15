# SoloStack 应用更新设计

## 1. 版本与发布

- 应用版本唯一来源：`package.json`。
- `src-tauri/tauri.conf.json` 的 `version` 指向 `../package.json`。
- 关于页通过 `@tauri-apps/api/app` 的 `getVersion()` 显示当前版本。
- 发布 tag 必须严格等于 `v{package.json.version}`，由 `scripts/check-app-version.mjs` 校验。

## 2. 更新链路

客户端使用 Tauri Updater：

1. 用户点击“检查更新”。
2. 应用读取 GitHub Release 中发布的 `latest.json`。
3. 远端版本高于当前版本时，显示新版本号、发布日期和更新说明。
4. 用户点击“下载并安装”。
5. Tauri 下载更新包并校验更新签名。
6. 安装完成后应用自动重启。

更新地址：

```text
https://github.com/lkzc19/SoloStack/releases/latest/download/latest.json
```

## 3. 签名密钥

更新签名与 Apple 签名是两套独立机制。更新包必须使用 Tauri 更新私钥签名。

当前本机密钥：

```text
~/.tauri/solostack.key
~/.tauri/solostack.key.pub
```

公钥已经写入 `src-tauri/tauri.conf.json`。私钥不得提交到 Git，需要备份到安全位置。

GitHub Actions 需要配置：

```text
TAURI_SIGNING_PRIVATE_KEY          私钥文件内容
TAURI_SIGNING_PRIVATE_KEY_PASSWORD 可选；当前生成的是无密码私钥
```

仓库 Secret 内容可通过以下命令复制：

```bash
pbcopy < ~/.tauri/solostack.key
```

私钥丢失后，已发布版本无法验证新更新包，只能让用户重新下载安装。

首次包含 Updater 的版本仍需要手动安装；从该版本之后发布的新版本才能被应用内更新。

## 4. Release 产物

release workflow 设置：

```text
bundle.createUpdaterArtifacts = true
includeUpdaterJson = true
```

发布 Release 时至少包含：

```text
SoloStack.app.tar.gz
SoloStack.app.tar.gz.sig
latest.json
SoloStack_0.x.x_aarch64.dmg
```

DMG 用于首次安装；`.app.tar.gz + .sig + latest.json` 用于应用内更新。

当前 Release 默认是草稿。草稿不会被 `/releases/latest` 发现，确认产物后再手动发布。

## 5. 客户端状态

设置页“关于”提供手动检查更新，不自动下载或强制更新。

状态包括：

- 空闲
- 检查中
- 已是最新版本
- 发现可更新版本
- 下载中
- 安装中
- 更新失败

用户确认后执行下载、签名校验、安装和重启。应用数据位于 `~/.solostack`，不会因更新被删除。
