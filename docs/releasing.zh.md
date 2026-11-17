# Nodara 发布指南

> English: [releasing.md](releasing.md)

本文说明 Nodara 的版本号如何管理、发布如何构建、验收与发布。“一条命令发布”
是快速路径，其余部分解释自动化在做什么，便于失败时定位问题而不是靠猜。

下文命令中的 `<version>` 是待发布版本的占位符（例如 `2.1.0`）。

## 版本模型

产品版本与三个契约版本相互独立：

| 版本 | 当前来源 | 含义 |
|---|---|---|
| 产品 `<version>` | `Nodara-Core/Cargo.toml` 的 `[workspace.package]` | 发布版本、安装器与所有可执行文件 |
| 工作流 `schema_version` `2.1` | `nodara-schema` | 工作流文档格式 |
| 插件 `protocol_version` `1` | `nodara-schema` | 基于 stdio 的 JSON-RPC |
| API `api_version` `v1` | `nodara-runtime` | HTTP 与 WebSocket 路由 |

产品发布不会隐式改变契约版本，契约变更也不必然要求产品发布。

## 一条命令发布

```powershell
# 在仓库根目录，master 干净且与 origin/master 一致
.\scripts\release.ps1 -Version <version> -Title "schema-driven content hints"
```

脚本会：

1. 拒绝在脏工作区、非 `master`、与 `origin/master` 不一致、tag 已存在或
   GitHub 上已有同名 Release 时继续。
2. 运行 `scripts/set-version.ps1`，改写所有版本位置。
3. 收尾 `CHANGELOG.md`：`## [Unreleased]` 变为
   `## [<version>] — … - YYYY-MM-DD`，并在其上方插入新的空 `## [Unreleased]`。
4. 运行 `scripts/check-version.ps1 -Version <version> -RequireChangelog`。
5. 提交 `chore(release): v<version>`，创建附注 tag `v<version>`，一次 push 推送
   `master` 与 tag。
6. 打印 Release 工作流地址。

常用参数：

| 参数 | 作用 |
|---|---|
| `-NoPush` | 只在本地提交和打 tag，打印推送命令 |
| `-TagOnly` | 只给当前 `HEAD` 打 tag，不升版本、不提交 |
| `-TagOnly -Force` | 把已有 tag 移到 `HEAD`（仅当 GitHub 上还没有 Release） |
| `-Date 2026-11-17` | 覆盖 CHANGELOG 日期 |
| `-Title "…"` | CHANGELOG 标题中版本号后的副标题 |

### 版本位置

`set-version.ps1` 与 `check-version.ps1` 覆盖同一份清单，不要手工修改：

- `Nodara-Core/Cargo.toml` — `[workspace.package].version`
- `Nodara-Core/Cargo.lock` — 八个 workspace crate
- `Nodara-Agent/Cargo.toml` — `[workspace.package].version`
- `Nodara-Agent/Cargo.lock` — `nodara-agent`、`nodara-schema`
- `Nodara-Studio/package.json`
- `Nodara-Studio/package-lock.json` — 根对象与 `""` workspace 条目
- `Nodara-Studio/src-tauri/Cargo.toml` — `[package].version`
- `Nodara-Studio/src-tauri/Cargo.lock` — `nodara-studio`
- `Nodara-Studio/src-tauri/tauri.conf.json`
- `Nodara-Core/plugins/nodara-platform/manifest.json`
- `Nodara-Core/plugins/nodara-vision/manifest.json`

协议示例与测试 fixture 中的 `2.0.0` 是刻意保留的示例，不是产品版本。

## CI 做什么

`.github/workflows/release.yml` 在任何 `v*` tag 上运行，包含两个 job。

### `build`（windows-latest）

| 步骤 | 门禁 |
|---|---|
| 版本清单 | `check-version.ps1 -Version … -Tag … -RequireChangelog` |
| 格式 | 两个 Rust workspace 的 `cargo fmt --all -- --check` |
| Lint | 两个 workspace 的 `cargo clippy --workspace --all-targets -- -D warnings` |
| Schema | 重新生成 `Nodara-Core/schema`，有 diff 即失败 |
| 测试 | 通过 `build-artifacts.ps1` 跑 Core、Agent、Studio 测试 |
| 构建 | release 二进制、Tauri NSIS + MSI、便携 ZIP |
| 产物验收 | `verify-package.ps1` 检查哈希、入口、CLI 冒烟与 runtime API |
| 上传 | `release-assets` workflow artifact，保留 7 天 |

`verify-package.ps1` 解压 ZIP 并检查：

- ZIP 哈希与 `.sha256` 一致，`SHA256SUMS.txt` 每一行一致；
- CLI、runtime、Agent、Studio、两个插件、`studio-web/`、`schema/`、`examples/`、
  `docs/` 与两个安装器都存在；
- `build-info.json` 的版本、`Release`、commit 与 tag 一致；
- `nodara-cli --version`、`validate`、`simulate`、`run`、`inspect`、
  `extensions --json` 均成功；
- `nodara-runtime.exe` 的 `/api/v1/health` 返回 2 个插件、19 个节点类型，
  `/api/v1/plugins` 无 failure，`/api/v1/extensions` 非空，
  `/api/v1/schema/workflow` 返回 schema。

### `publish`（windows-latest，仅 tag）

从 CHANGELOG 小节生成带文档链接的发布说明，用
`--verify-tag --latest` 创建 GitHub Release（带 `-后缀` 的版本用
`--prerelease`），上传资产后再读取一次 Release，断言资产列表与标志。

发布资产：

| 资产 | 内容 |
|---|---|
| `Nodara-<version>-windows-x86_64.zip` | 便携包 |
| `Nodara-<version>-windows-x86_64.zip.sha256` | ZIP 校验和 |
| `Nodara-Studio-<version>-x64-setup.exe` | NSIS 安装器 |
| `Nodara-Studio-<version>-x64.msi` | MSI 安装器 |
| `Nodara-<version>-build-info.json` | 构建信息 |
| `SHA256SUMS.txt` | 上述资产的校验和 |

## 手动与 dry run

手动运行 Release 工作流（`workflow_dispatch`，输入 `version`）会跑同样的门禁
并上传 `release-assets`，但不会创建 Release。适合在打 tag 前验证流水线改动。

本地也能产出同样的包：

```powershell
.\scripts\build-artifacts.ps1 -Configuration Release
.\scripts\verify-package.ps1 `
  -ZipPath .\artifacts\release\Nodara-<version>-windows-x86_64.zip `
  -Version <version>
```

`-Configuration Release` 只构建 release。`-SkipTests` 与 `-SkipBuild` 可用于
快速迭代，但正式发布绝不使用跳过测试的包。

## 验收已发布的 Release

```powershell
gh release download v<version> -D .\artifacts\tmp\release-verify
.\scripts\verify-package.ps1 `
  -ZipPath .\artifacts\tmp\release-verify\Nodara-<version>-windows-x86_64.zip `
  -Version <version>
```

发布说明里也给出了用户需要的两条命令：

```powershell
(Get-FileHash .\Nodara-<version>-windows-x86_64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
Get-Content .\Nodara-<version>-windows-x86_64.zip.sha256
```

## 故障恢复

| 情况 | 恢复方式 |
|---|---|
| build job 失败，未创建 Release | 先取消该次运行（`gh run cancel <run-id>`），在 `master` 修复后执行 `.\scripts\release.ps1 -Version <version> -TagOnly -Force` |
| build 成功但 publish 失败 | `gh run rerun <run-id> --failed`；workflow artifact 保留 7 天 |
| Release 已创建但内容有误 | `gh release delete v<version> --yes`，修复后 `-TagOnly -Force` |
| tag 存在但 commit 不对 | `git push origin :refs/tags/v<version>`，修复后 `-TagOnly -Force` |
| 版本清单漂移 | `.\scripts\check-version.ps1 -Version <version> -Tag v<version> -RequireChangelog` |
| CHANGELOG 缺少小节 | 补 `## [<version>]` 小节（或重跑 `release.ps1` 自动写入） |

`-TagOnly -Force` 在 GitHub Release 已存在时会拒绝执行，因此不会静默覆盖已发布版本。

## 预发布与热修复

- `v2.2.0-rc.1` 是合法 tag：工作流会标记 `--prerelease`，不移动 `latest`。
- 热修复是从 `master` 发出的普通补丁版本（`2.1.1`），不重写 tag。用户靠校验和
  验证下载，因此不支持在同一版本号下替换资产。

## 安全说明

- 产物**未签名**。首次启动可能触发 SmartScreen，发布说明要求用户先校验哈希。
- publish job 使用仓库的 `GITHUB_TOKEN`（`contents: write`），仓库中不保存长期
  个人 token。
- 目前没有签名密钥、公证或自动更新源。增加这些能力意味着新增 workflow 步骤和
  secret，而不是改变本流程。
