# Nodara 构建产物说明

> English: [artifacts.md](artifacts.md)

`scripts/build-artifacts.ps1 -Configuration All` 构建**当前宿主**平台的包。脚本自动
识别 Windows、macOS 或 Linux，产物命名为 `Nodara-<version>-<platform>-<arch>`，
Windows 输出 `.zip`，macOS/Linux 输出 `.tar.gz`：

```text
artifacts/
├── debug/
│   ├── Nodara-2.3.0-windows-x86_64-debug/
│   ├── Nodara-2.3.0-windows-x86_64-debug.zip
│   └── Nodara-2.3.0-windows-x86_64-debug.zip.sha256
└── release/
    ├── Nodara-2.3.0-windows-x86_64/
    ├── Nodara-2.3.0-windows-x86_64.zip
    └── Nodara-2.3.0-windows-x86_64.zip.sha256
```

| 宿主 | 便携归档 | 安装器 |
|---|---|---|
| Windows x64 | `Nodara-<version>-windows-x86_64.zip` | NSIS `.exe`、MSI `.msi` |
| Linux x64 | `Nodara-<version>-linux-x86_64.tar.gz` | DEB、AppImage |
| macOS Apple Silicon | `Nodara-<version>-macos-aarch64.tar.gz` | DMG |
| macOS Intel | `Nodara-<version>-macos-x86_64.tar.gz` | DMG |

正式发布在各自的原生 runner 上构建，Release 工作流把四个包合并到一个 GitHub
Release。`artifacts/` 是本地生成目录，不提交到 Git。

## 包内容

每个包都包含 CLI、独立 runtime、Agent、Studio 可执行文件、Web 构建、两个插件、
示例、Schema、文档与 `build-info.json`。Windows debug 包额外包含 `symbols/` 目录
和 PDB 文件。

`SHA256SUMS.txt` 覆盖包内除自身外的全部文件。release 安装器会把 runtime 与两个
官方插件一并安装到应用旁边。

## 脚本参数

| 参数 | 作用 |
|---|---|
| `-Configuration Debug` | 只构建和归档 debug |
| `-Configuration Release` | 只构建和归档 release |
| `-Configuration All` | 构建两套，默认值 |
| `-SkipTests` | 跳过 Core/Agent/Studio 测试；仅用于已测试过的重打包 |
| `-SkipBuild` | 不改动目标目录，只重新整理现有构建输出 |

`-Configuration Release` 只构建 release。正常构建会先运行 Core、Agent、Studio
测试套件。

## 完整性校验

先校验归档：

```powershell
$archive = ".\artifacts\release\Nodara-2.3.0-windows-x86_64.zip"  # 或 .tar.gz
$expected = (Get-Content "$archive.sha256").Split()[0]
$actual = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
$expected -eq $actual
```

macOS/Linux：

```bash
shasum -a 256 -c Nodara-2.3.0-linux-x86_64.tar.gz.sha256
# 或：sha256sum -c Nodara-2.3.0-linux-x86_64.tar.gz.sha256
```

再校验包内文件：

```powershell
Set-Location .\artifacts\release\Nodara-2.3.0-windows-x86_64
Get-Content .\SHA256SUMS.txt
```

`scripts/verify-package.ps1` 会自动完成上述检查并追加 CLI/runtime 冒烟测试；
Release 工作流在上传前运行它。

## 构建元数据

`build-info.json` 除版本、commit 和工具链外，还记录平台与架构：

```json
{
  "product": "Nodara",
  "version": "2.3.0",
  "configuration": "Release",
  "platform": "windows",
  "arch": "x86_64",
  "source_commit": "abc1234",
  "installers": ["Nodara-Studio-2.3.0-x64-setup.exe", "Nodara-Studio-2.3.0-x64.msi"],
  "entrypoints": ["nodara-cli.exe", "nodara-runtime.exe", "nodara-agent.exe", "nodara-studio.exe", "studio-web/index.html"]
}
```

`tests_run` 只表示打包脚本运行了测试命令；测试失败会立即终止脚本，不会写出新包。

## GitHub Releases 发布

推送 `v*` tag 会在 Windows、Linux、macOS 上触发 `.github/workflows/release.yml`，
用 `scripts/verify-package.ps1` 验收每个归档，然后把四个便携包、对应的 `.sha256`、
各平台安装器、构建元数据和资产级 `SHA256SUMS.txt` 发布到 GitHub Releases。完整
流程与恢复方式见 [releasing.zh.md](releasing.zh.md)。

下载并验收已发布的包：

```powershell
gh release download v2.3.0 -D .\artifacts\release
.\scripts\verify-package.ps1 `
  -ZipPath .\artifacts\release\Nodara-2.3.0-windows-x86_64.zip `
  -Version 2.3.0
```

## 构建要求与可复现性

- Windows x64 + MSVC、macOS 12+ + Xcode 命令行工具，或带 Tauri WebKitGTK 4.1
  开发包的 Linux x64。
- Rust 1.88 或更高；Studio 需要 Node.js 20 或更高。
- Windows 使用 WebView2，macOS 使用 WKWebView，Linux 使用 WebKitGTK 4.1。
- 首次构建 MSI 时 Tauri 会下载 WiX 3；Linux AppImage 会下载 `linuxdeploy`；
  两者都需要网络。

`Cargo.lock` 和 `package-lock.json` 已提交。正式验收包不得手改版本、lockfile 或
产物内容；版本来自 workspace，commit 写入 `build-info.json`。

## 信任边界

产物不包含代码签名证书。Windows SmartScreen 与 macOS Gatekeeper 可能显示警告；
macOS 的输入与截图还需要辅助功能和屏幕录制权限。Linux 包同样未签名。当前没有
自动更新源；若后续加入，需要新增 workflow 步骤和 secret，而不改变上述布局。
