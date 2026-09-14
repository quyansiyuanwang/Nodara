# Nodara 快速开始

> English: [QUICKSTART.md](QUICKSTART.md)

本文面向拿到预编译包、希望尽快验证 Nodara 的测试人员。整个流程只需要
Windows 10/11 x64，不需要安装 Rust、Node.js 或 Visual Studio。

## 1. 解压发布包

建议先测试 release 包：

```powershell
Set-Location .\artifacts\release
Expand-Archive .\Nodara-2.0.0-windows-x86_64.zip -DestinationPath .\test-release
Set-Location .\test-release\Nodara-2.0.0-windows-x86_64
```

先检查文件完整性。目录内已经包含 `SHA256SUMS.txt`：

```powershell
Get-FileHash .\nodara-cli.exe -Algorithm SHA256
.\nodara-cli.exe --version
```

预期版本为 `nodara-cli 2.0.0`。

> release 包面向验收；debug 包包含 PDB 调试符号，体积更大，适合复现问题。
> 两者的目录结构和启动命令相同。

## 2. 运行第一个工作流

以下命令会在当前终端输出策略决策、日志和运行结果：

```powershell
.\nodara-cli.exe validate .\examples\hello-world.json
.\nodara-cli.exe simulate .\examples\hello-world.json
.\nodara-cli.exe run .\examples\hello-world.json
```

预期 `run` 的输出以类似内容结束：

```text
Hello, World!
run completed: 5 node(s) in 3ms
audit: 20 record(s) in 4ms wall clock
```

覆盖变量：

```powershell
.\nodara-cli.exe run .\examples\hello-world.json --var name=Codex
```

检查分支和延迟示例：

```powershell
.\nodara-cli.exe inspect .\examples\branching.json
.\nodara-cli.exe simulate .\examples\branching.json
.\nodara-cli.exe run .\examples\delayed-log.json
```

## 3. 启动 runtime

`nodara-runtime.exe` 是 Studio 和 Agent 共用的 HTTP/WebSocket 服务。它会从
自身旁边的 `plugins\` 目录加载官方插件。

在第一个 PowerShell 窗口运行：

```powershell
.\nodara-runtime.exe
```

预期输出：

```text
runtime listening on http://127.0.0.1:8710/api/v1 (16 node type(s), 2 plugin(s))
```

在第二个 PowerShell 窗口检查服务：

```powershell
Invoke-RestMethod http://127.0.0.1:8710/api/v1/health
Invoke-RestMethod http://127.0.0.1:8710/api/v1/plugins
(Invoke-RestMethod http://127.0.0.1:8710/api/v1/node-types).node_types | Select-Object node_type
```

健康检查应报告 `status=ok`、`node_types=16`。端口冲突时可在启动前设置：

```powershell
$env:NODARA_RUNTIME_PORT = "8720"
.\nodara-runtime.exe
```

固定端口的 `nodara-studio.exe` 和默认 Agent 连接 `8710`，因此完整联调时应使用
默认端口。

## 4. 打开 Studio 桌面版

保持 runtime 运行，在第二个窗口执行：

```powershell
.\nodara-studio.exe
```

打开窗口后应看到右上角连接标记显示类似 `16 node types · 2 plugin(s)`。随后：

1. 点击 **Import**，选择 `examples\hello-world.json`；
2. 确认画布显示 `Start → Log → End`；
3. 点击 **Validate**，应无错误；
4. 点击 **Run**，在 **Events** 页观察事件；
5. 打开 **Audit** 页，确认策略决策和节点结果已记录。

如果连接标记显示 `runtime unreachable`，先回到上一节确认健康检查成功，再等待
Studio 的五秒自动重连。

### 浏览器版 Studio

发布包中的 `studio-web\` 是同一套前端。浏览器版要求由 Vite 或反向代理把
`/api` 转发到 `http://127.0.0.1:8710`：

```powershell
Set-Location <仓库路径>\Nodara-Studio
npm ci
npm run dev
```

日常人工验收优先使用 `nodara-studio.exe`，它不需要前端构建工具。

## 5. 测试 Agent（无需真实 API Key）

保持 runtime 运行，在发布包根目录打开第三个 PowerShell 窗口。先检查 Agent
能够发现 runtime 的能力：

```powershell
.\nodara-agent.exe capabilities
```

使用内置 mock provider 生成并运行一个最小工作流：

```powershell
$mock = '{"schema_version":"2.0","id":"wf.hello","metadata":{"name":"Hello"},"nodes":[{"id":"start","type":"core.Start"},{"id":"log","type":"core.Log","config":{"message":"hello from agent"}},{"id":"end","type":"core.End"}],"edges":[{"id":"e1","source":"start","target":"log"},{"id":"e2","source":"log","target":"end"}]}'

.\nodara-agent.exe plan "输出 hello" --mock $mock --out .\agent-hello.json
.\nodara-agent.exe run "输出 hello" --mock $mock --variables '{}' --timeout 30
```

真正调用模型时设置：

```powershell
$env:NODARA_LLM_API_KEY = "sk-..."
$env:NODARA_LLM_ENDPOINT = "https://api.openai.com/v1/chat/completions"
$env:NODARA_LLM_MODEL = "gpt-4o-mini"
.\nodara-agent.exe plan "打开记事本并输入 hello" --safe
```

Agent 的护栏不能替代 runtime 策略。所有实际执行仍由 runtime 审批并写入审计日志。

## 6. 安全提醒

发布包中的 runtime 默认使用 `DefaultPolicy`，并自动批准需要审批的能力。这适合
受信任的本地测试，但意味着键鼠、窗口、截图、剪贴板和视觉节点可以立即执行。

- 不要运行来源不明的 JSON 工作流。
- 先用 `nodara-cli simulate` 查看会执行哪些节点及所需权限。
- Agent 测试优先加 `--safe` 或 `--allow <节点类型>`。
- 需要人工审批时改用源码工作区启动：
  `cargo run -p nodara-cli -- serve --plugin-dir plugins --require-approval`。
- 测试完成后关闭 runtime、Studio 和 Agent 进程。

## 7. 下一步

| 目标 | 文档 |
|---|---|
| 逐个功能验收、记录结果 | [testing.zh.md](docs/testing.zh.md) |
| 完整命令、配置和排障 | [user-manual.zh.md](docs/user-manual.zh.md) |
| 产物目录与校验规则 | [artifacts.zh.md](docs/artifacts.zh.md) |
| 从源码构建 | [README.md](README.md) 与 `scripts/build-artifacts.ps1` |