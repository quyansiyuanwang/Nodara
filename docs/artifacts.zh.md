# Nodara 构建产物说明

> English: [artifacts.md](artifacts.md)

本文说明 2.0.0 Windows x64 产物的布局、调试与发布差异、校验方法及重建方式。

## 1. 产物位置

默认打包命令：

```powershell
.\scripts\build-artifacts.ps1 -Configuration All
```

生成：

```text
artifacts/
├── debug/
│   ├── Nodara-2.0.0-windows-x86_64-debug/
│   ├── Nodara-2.0.0-windows-x86_64-debug.zip
│   └── Nodara-2.0.0-windows-x86_64-debug.zip.sha256
└── release/
    ├── Nodara-2.0.0-windows-x86_64/
    ├── Nodara-2.0.0-windows-x86_64.zip
    └── Nodara-2.0.0-windows-x86_64.zip.sha256
```

`artifacts/` 是本地生成目录，不提交到 Git。脚本支持：

| 参数 | 作用 |
|---|---|
| `-Configuration Debug` | 只构建和归档 debug |
| `-Configuration Release` | 只构建和归档 release |
| `-Configuration All` | 构建两套，默认值 |
| `-SkipTests` | 跳过测试，只构建；仅用于已经完成测试后的重打包 |
| `-SkipBuild` | 不改动目标目录，只重新整理现有构建输出 |

## 2. 可运行目录结构

两套包使用同一布局：

```text
Nodara-2.0.0-windows-x86_64[-debug]/
├── nodara-cli.exe
├── nodara-runtime.exe
├── nodara-agent.exe
├── nodara-studio.exe
├── build-info.json
├── SHA256SUMS.txt
├── plugins/
│   ├── nodara-platform/
│   │   ├── manifest.json
│   │   └── nodara-platform-plugin.exe
│   └── nodara-vision/
│       ├── manifest.json
│       └── nodara-vision-plugin.exe
├── examples/
│   ├── hello-world.json
│   ├── delayed-log.json
│   ├── branching.json
│   └── window-find.json
├── schema/
│   ├── workflow.schema.json
│   └── ...
├── studio-web/
│   ├── index.html
│   └── assets/
├── docs/
│   ├── user-manual.zh.md
│   ├── testing.zh.md
│   └── artifacts.zh.md
└── installers/                  # 仅 release
    ├── Nodara-Studio-2.0.0-x64-setup.exe
    └── Nodara-Studio-2.0.0-x64.msi
```

debug 额外包含：

```text
symbols/
├── nodara_cli.pdb
├── nodara_runtime.pdb
├── nodara_platform_plugin.pdb
├── nodara_vision_plugin.pdb
├── nodara_agent.pdb
└── nodara_studio.pdb
```

`nodara-runtime.exe` 从自身同级 `plugins\` 自动加载插件。直接解压后在该目录运行即可，
无需设置 `NODARA_PLUGIN_DIRS`。

## 3. Debug 与 Release

| 项目 | Debug | Release |
|---|---|---|
| Cargo profile | `dev`，`opt-level=1` | `opt-level=3`、LTO、`codegen-units=1`、strip |
| 调试符号 | 独立 PDB，随包提供 | 不随包分发 |
| 运行速度 | 较慢 | 面向最终验收和分发 |
| 可执行文件体积 | 较大 | 较小 |
| Studio 安装包 | 不生成 | NSIS 与 MSI |
| 推荐用途 | 缺陷复现、附加调试器、符号定位 | 功能测试、性能测试、安装/卸载验收 |

两套包应具有相同功能行为。不要用 debug 包性能结果推断 release 性能，也不要用
release PDB 缺失判断构建失败。

## 4. 完整性校验

ZIP 级校验：

```powershell
$zip = ".\artifacts\release\Nodara-2.0.0-windows-x86_64.zip"
$expected = (Get-Content "$zip.sha256").Split()[0]
$actual = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
$expected -eq $actual
```

解压后的文件级校验：

```powershell
Set-Location .\artifacts\release\Nodara-2.0.0-windows-x86_64
Get-Content .\SHA256SUMS.txt
```

`SHA256SUMS.txt` 覆盖包内除自身外的全部文件。测试和归档时应同时保存 ZIP 的
`.sha256` 和包内校验文件。

## 5. 构建元数据

`build-info.json` 示例：

```json
{
  "product": "Nodara",
  "version": "2.0.0",
  "configuration": "Release",
  "target": "x86_64-pc-windows-msvc",
  "built_at_utc": "2026-09-14T00:00:00.0000000Z",
  "source_commit": "abc1234",
  "rust": "rustc 1.92.0",
  "node": "v24.11.1",
  "tests_run": true,
  "entrypoints": [
    "nodara-cli.exe",
    "nodara-runtime.exe",
    "nodara-agent.exe",
    "nodara-studio.exe",
    "studio-web\\index.html"
  ]
}
```

实际内容以生成文件为准。`tests_run` 只表示打包脚本运行了测试命令，不代替测试
报告；失败测试会使脚本立即退出，不会生成新的完整包。

## 6. 单独重建

只重建 release：

```powershell
.\scripts\build-artifacts.ps1 -Configuration Release
```

跳过测试但仍执行全部构建：

```powershell
.\scripts\build-artifacts.ps1 -Configuration All -SkipTests
```

已有目标目录时只重新整理和压缩：

```powershell
.\scripts\build-artifacts.ps1 -Configuration All -SkipBuild
```

底层命令：

```powershell
# Core
Set-Location .\Nodara-Core
cargo build --workspace
cargo build --workspace --release

# Agent
Set-Location ..\Nodara-Agent
cargo build --workspace
cargo build --workspace --release

# Studio Web + Tauri
Set-Location ..\Nodara-Studio
npm ci
npm test
npm run build
.\node_modules\.bin\tauri.cmd build --debug --no-bundle --ci
.\node_modules\.bin\tauri.cmd build --bundles nsis msi --ci
```

## 7. 构建要求和可复现性

- Windows x64 / MSVC 工具链；
- Rust stable，最低仓库声明版本 1.75；
- Node.js 20 或更高版本；
- WebView2 Runtime；
- MSVC C++ 构建工具；
- Tauri 2 CLI；
- 首次构建 MSI 时 Tauri 会下载 WiX 3 工具链，网络不可用时 MSI 构建会失败。

`Cargo.lock` 和 `package-lock.json` 已提交。生成正式验收包时不得手改版本、lockfile 或
产物内容；版本应来自工作区版本号，提交号应写入 `build-info.json`。

## 8. 信任边界

产物不包含代码签名证书。Windows SmartScreen 可能显示未知发布者警告。当前没有
Apple/Linux 产物，也没有 portable-to-installed 迁移机制。若后续加入代码签名，
应在此文档补充证书指纹、时间戳服务和验证命令。