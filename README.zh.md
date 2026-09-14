# Nodara

> English: [README.md](README.md)

Nodara 是一套面向 Windows 桌面的插件化工作流自动化平台。工作流是 JSON 图，
键盘、窗口、OCR、截图以及第三方能力都以独立进程和 manifest 的形式提供。Studio
和 AI Agent 都是同一个 runtime API 的客户端。

```text
Nodara-Core/      核心 runtime、Schema、插件协议、SDK、CLI
Nodara-Studio/    Tauri + Web 可视化工作流编辑器
Nodara-Agent/     自然语言规划与操作器
examples/         可直接运行的示例工作流
```

## 快速验证

如果只需要测试预编译产物，推荐先阅读 [QUICKSTART.zh.md](QUICKSTART.zh.md)：

```powershell
Set-Location .\artifacts\release
Expand-Archive .\Nodara-2.0.0-windows-x86_64.zip -DestinationPath .\test-release
Set-Location .\test-release\Nodara-2.0.0-windows-x86_64

.\nodara-cli.exe validate .\examples\hello-world.json
.\nodara-cli.exe run .\examples\hello-world.json
.\nodara-runtime.exe
```

保持 runtime 运行，再从另一个终端启动 `.\nodara-studio.exe`。

从源码运行时需要 Rust 1.75+；Studio 还需要 Node.js 20+：

```powershell
Set-Location .\Nodara-Core
cargo run -p nodara-cli -- validate ..\examples\hello-world.json
cargo run -p nodara-cli -- simulate ..\examples\hello-world.json
cargo run -p nodara-cli -- run ..\examples\hello-world.json

Set-Location ..\Nodara-Studio
npm ci
npm run dev
```

## 文档

| 文档 | 内容 |
|---|---|
| [QUICKSTART.zh.md](QUICKSTART.zh.md) | 预编译包五分钟上手 |
| [docs/user-manual.zh.md](docs/user-manual.zh.md) | CLI、runtime、Studio、Agent、API 和安全 |
| [docs/testing.zh.md](docs/testing.zh.md) | debug/release 验收与回归清单 |
| [docs/artifacts.zh.md](docs/artifacts.zh.md) | 目录结构、校验、构建和交付差异 |
| [docs/README.zh.md](docs/README.zh.md) | 完整中文文档索引 |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 仓库边界与依赖方向 |
| [CHANGELOG.md](CHANGELOG.md) | 版本历史 |

## 构建产物

Windows x64 的 debug 和 release 包可通过以下命令一次生成：

```powershell
.\scripts\build-artifacts.ps1 -Configuration All
```

脚本执行 Core、Agent、Studio 测试，构建两组 Rust 二进制，生成 Tauri 桌面程序和
NSIS/MSI 安装包，并输出 ZIP、SHA-256 和构建元数据。生成位置：

```text
artifacts\debug\
artifacts\release\
```

## 核心契约

| 契约 | 字段 | 当前版本 |
|---|---|---|
| 工作流文档 | `schema_version` | `2.0` |
| 插件/runtime 线协议 | `protocol_version` | `1` |
| 公开 HTTP API | `api_version` | `v1` |

节点类型使用命名空间，例如 `core.Start`、`windows.Input.Keyboard`、
`vision.Ocr`。插件描述符还声明权限，runtime 在执行前做策略判断并写入审计日志。

## 安全边界

Studio 和 Agent 不直接执行能力；所有调用都通过 runtime。Agent 只依赖公开 Schema
契约并通过 HTTP/WebSocket 访问 runtime，因此无法绕过策略层。发布包默认自动批准
特权节点，只应在受信任的测试环境运行来源可靠的工作流。