# SoloStack-数据迁移设计

> 描述 SoloStack 如何在版本演进中迁移磁盘布局而不丢数据：数据布局版本、版本门控、
> 步骤表与迁移步骤的内部约定。
> 活文档，随 `crates/core/src/app/migration/` 同步维护；如与代码冲突，以代码为准并回来更新本文件。
> 创建时间：2026-09-21

---

## 1. 定位

- **目标**：磁盘布局随版本变化时，老数据能被安全地迁移到新布局；升级路径可追溯、可跳过、
  失败不丢数据。
- **非目标**：这不是 app release 版本管理。app 版本（`package.json` / tag）与本文的
  「数据布局版本」是两件事，见 §2。

---

## 2. 为什么不用 app 版本做迁移键

迁移的键是**数据布局版本**，不是 app 的 release 版本。

- 不是每次发版都有迁移：补丁版通常零迁移，某个小版本可能一次带几步。
- 用户会跳版本升级（`0.1.0` → `0.4.0`），"中间该依次跑哪几步"本就是迁移系统要回答的问题。
- 语义版本排序不适合当判断条件，`if app_version < "0.3.0"` 会把版本号硬编码进逻辑。
- app 版本只适合写在注释/日志里做溯源（例如标注某步由哪个版本引入），不能当迁移键。

### 业界两种模型

| 模型 | 追踪方式 | 代表 |
|---|---|---|
| 单一当前版本整数 | 存一个 `version = N`，代码 `if version < N { step }` | SQLite `user_version`、Electron 桌面应用状态库 |
| 迁移账本 | 每条迁移有唯一 id，账本记录已应用集合，按序补跑未应用的 | Rails、Django、Flyway、Liquibase、Alembic、Knex |

Android Room 用 `Migration(from, to)` 描述"从某版到某版"，由框架选一条路径依次执行——本质
也是**数据库 schema 版本**，而非 app 版本。

### 本项目的选型

步骤少、各步幂等 → 用**单一当前版本整数 + 有序步骤表**（模型 1 的显式化）。若将来步骤
变多、出现非幂等步骤，再考虑升级为账本模型（§9）。

---

## 3. 版本号语义

| 版本 | 含义 |
|---|---|
| `0` | 旧版全局布局（无标记文件） |
| `1` | 多环境布局：`app/` `cache/` `environments/`，组件按环境隔离 |

开发者**手动 +1**，只在该布局真正变化时加。版本号一旦发布就不再改动。

---

## 4. 标记与门控

### 标记文件

- 路径：`~/.solostack/.migration-version`（`paths::migration_version_file()`）。
- 内容：一行整数。
- 读取规则（`migration/version.rs::current`）：
  - 文件不存在 → `0`；
  - 内容无法解析 → `0`（迁移步骤都幂等，宁可多跑一次空迁移，也不因标记损坏让升级失效）。

### 入口与算法

唯一入口 `app::migration::run()`（`src-tauri/src/lib.rs` 启动时调用，失败记日志）：

```rust
let current = version::current()?;
let latest = STEPS.last().map(|(t, _)| *t).unwrap_or(0);

if current > latest { return Err("数据布局版本高于程序支持…"); } // 降级保护，见 §7
if current == latest { return Ok(()); }                          // 已最新，直接返回

for (target, step) in STEPS {
    if current < *target { step()?; }     // 依次跑所有未应用的步骤
}
version::write(latest)                    // 全部成功后才写回
```

要点：**全部步骤成功后才写回版本**；任一步失败则保留旧版本，下次启动重试（各步幂等）。

---

## 5. 步骤表 `STEPS`

`app/migration.rs` 定义：

```rust
/// 一步迁移：目标版本 + 处理函数。
type MigrationStep = (u32, fn() -> Result<(), String>);

/// 新增布局变更时末尾追加一行并 +1；不要改动已发布步骤的版本号。
const STEPS: &[MigrationStep] = &[(1, migrate_to_v1)];
```

### 新增一步的做法

1. 在 `STEPS` 末尾追加 `(N, migrate_to_vN)`（`N` = 上一步 +1）。
2. 实现 `fn migrate_to_vN() -> Result<(), String>`：只做"从 `N-1` 布局到 `N` 布局"的增量。
3. 在函数文档注释里标注该步的引入背景（对应哪次布局变更；如有对应版本再写上）。
4. 更新 §3 的版本语义表。
5. 补一个"老版本数据 + 空标记 → `run()` 后到达 `N`"的测试。

**不要**修改已发布步骤的版本号：老数据里记录的版本就是按这些编号写下的。

---

## 6. 迁移步骤的内部约定

每个步骤（见 `app_layout.rs` / `environment_layout.rs`）遵循：

- **幂等**：可重复执行不产生副作用；这是"失败重试"与"版本标记损坏按 0"的前提。
- **staging + 提交**：先在 `environments/.migration-<id>/` 里完成移动，最后整体 rename 提交。
- **回滚**：中途失败时按 `moves` 记录逆序移回原位（`fs_ops::move_if_exists` / `restore_move`）。
- **异常退出恢复**：启动时扫描残留的 `.migration-*` staging，把数据移回原位而非删除
  （`recovery::recover_stale_migration`）。
- **不安全就报错，不静默处理**：目标已存在、环境目录与旧组件目录并存等情况一律报错，
  要求人工介入，绝不覆盖或删除数据。

### 破坏性清理的注意

`app_layout.rs` 会删除 `installs/`（根与 `var/` 两处）`etc/` `snapshots/` `.templates/`
（旧版本废弃目录）。这是安全的**前提是这些名字不被复用**。若未来某个版本要用到同名
目录，必须先收紧这段逻辑（加判断或删除该清理），否则老迁移会把新数据删掉。

---

## 7. 降级保护

当标记里的版本 **高于** 程序支持的 `latest`（用新版程序写过数据、又用旧版打开）：

```
数据布局版本为 {current}，高于当前程序支持的 {latest}；
可能是用更新版程序写过数据后又在旧版打开，请升级程序后再试
```

此时 `run()` 直接返回错误，不再往下走。否则旧程序会按旧规则解释新数据，可能损坏。

---

## 8. 现有迁移清单

| 版本 | 引入 | 内容 |
|---|---|---|
| 1 | 多环境改造 | 旧全局布局 → 多环境布局：`app_layout`（settings/日志/下载缓存搬迁 + 废弃目录清理）+ `environment_layout`（旧 `components/` + `var/` → `environments/<id>/`） |

---

## 9. 什么时候删除一个迁移步骤

条件是**不再支持从该步对应的旧布局升级**，而不是"我已经迁移过了"。删早了会让仍停留在
旧布局的安装（含从旧备份恢复、换机器打开老数据）直接读不到数据。

删除时：从 `STEPS` 移除该步，并把 §3 的版本语义表下限抬高。

---

## 10. 模块结构

```
crates/core/src/app/migration.rs      门面：模块声明 + re-export + run() + STEPS + 集成测试
crates/core/src/app/migration/
  app_layout.rs          应用级目录迁移
  environment_layout.rs  旧 components/var → 环境目录（编排 + 分组 + migrate_group）
  legacy.rs              旧实例扫描
  recovery.rs            失败回滚与 staging 恢复
  fs_ops.rs              移动原语（move_if_exists / restore_move）
  version.rs             迁移版本标记读写
```

`app/mod.rs` 与 `migration.rs` 文档中标注了**分层例外**：`migration` 是 `app` 层唯一
了解组件实例磁盘布局的地方，属历史兼容的刻意例外，新代码不应据此在 `app` 层引入组件概念。

---

## 11. 测试

- `version_gate_migrates_once_then_skips`：无标记时迁移并写回版本；已应用后即使又出现旧
  目录也不再迁移。
- `rejects_data_newer_than_program`：标记版本高于程序支持时报错。
- 各迁移步骤的行为与回滚/恢复：见 `app/migration.rs` 的集成测试。

---

## 12. 未来演进

- 步骤变多时，可把 `STEPS` 扩展为带 id 与描述的迁移列表，或在标记文件旁记一份已应用
  集合（账本模型），以支持非幂等步骤与精确回溯。
- 若出现无法自动迁移、必须人工介入的情况，可在报错信息里给出具体数据目录与操作指引。
