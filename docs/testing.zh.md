# Nodara 测试与验收手册

> English: [testing.md](testing.md)

本文给出预编译 debug/release 产物的可重复验收步骤。记录结果时请保留
`build-info.json`、`SHA256SUMS.txt` 和失败现场。

## 1. 验收对象与通过标准

| 编号 | 检查项 | 通过标准 |
|---|---|---|
| A01 | 包完整性 | ZIP SHA-256 与同名 `.sha256` 文件一致，`SHA256SUMS.txt` 校验全部通过 |
| A02 | 版本 | CLI、runtime、Agent、Studio 均为 2.0.0；构建信息含正确 Git commit |
| A03 | CLI 核心流程 | `validate`、`simulate`、`run` 成功，`extensions --json` 返回统一注册列表 |
| A04 | 插件进程模式 | runtime 报告 2 个插件、16 个节点 |
| A05 | HTTP API | health、plugins、extensions、node-types、schema 端点可访问 |
| A06 | Studio 桌面版 | 能连接 runtime，显示 16 个节点，导入/校验/运行 hello-world 成功 |
| A07 | Agent mock | 无 API Key 时可规划并运行最小工作流 |
| A08 | 运行历史、事件与审计 | Runs 可重新打开历史运行，Events/Audit 或 API 可看到对应记录 |
| A09 | 调试产物 | debug 包包含与 exe 对应的 PDB；程序可运行 |
| A10 | 发布产物 | release 包包含 NSIS 和 MSI；优化后的 exe 可运行 |
| A11 | Studio 调试交互 | 空闲时可单步启动，后续每次只执行一个节点；截图可在模态图中框选并写回 X/Y/宽度/高度；截图和 Log 图片预览可见 |

任一 A 级检查失败，应保留日志并停止发布验收；恢复后从失败步骤重新执行。

## 2. 环境记录

开始前记录：

```powershell
$buildRoot = Resolve-Path .\artifacts\release
Get-ChildItem $buildRoot -File | Select-Object Name,Length,LastWriteTime
Get-Content "$buildRoot\Nodara-2.0.0-windows-x86_64.zip.sha256"
Get-Content (Join-Path $buildRoot "Nodara-2.0.0-windows-x86_64\build-info.json")
[Environment]::OSVersion.VersionString
$PSVersionTable.PSVersion
```

测试环境建议：

- Windows 10 22H2 或 Windows 11 x64；
- 默认 Windows DPI，另测一次 150% 缩放；
- 默认中文和英文用户名路径各测一次；
- 有 WebView2、VC++ 2015-2022 x64、Microsoft Defender 开启；
- 空闲端口 8710；
- 不使用生产 API Key。

## 3. 完整性检查

在 `artifacts\release` 执行：

```powershell
$zip = ".\Nodara-2.0.0-windows-x86_64.zip"
$expected = (Get-Content "$zip.sha256").Split()[0]
$actual = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "ZIP hash mismatch" }

Expand-Archive $zip -DestinationPath .\verify-release -Force
Set-Location .\verify-release\Nodara-2.0.0-windows-x86_64

$bad = @()
Get-Content .\SHA256SUMS.txt | ForEach-Object {
  $parts = $_ -split '  ', 2
  $hash = (Get-FileHash -LiteralPath $parts[1] -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($hash -ne $parts[0]) { $bad += $parts[1] }
}
if ($bad.Count) { throw "hash mismatch: $($bad -join ', ')" }
"integrity ok"
```

检查关键文件：

```powershell
Get-ChildItem -Recurse -File | Select-Object FullName,Length
```

release 必须包含：

```text
nodara-cli.exe
nodara-runtime.exe
nodara-agent.exe
nodara-studio.exe
plugins\nodara-platform\{manifest.json,nodara-platform-plugin.exe}
plugins\nodara-vision\{manifest.json,nodara-vision-plugin.exe}
studio-web\index.html
installers\Nodara-Studio-2.0.0-x64-setup.exe
installers\Nodara-Studio-2.0.0-x64.msi
```

debug 还必须包含：

```text
symbols\nodara_cli.pdb
symbols\nodara_runtime.pdb
symbols\nodara_agent.pdb
symbols\nodara_studio.pdb
```

## 4. CLI 冒烟测试

```powershell
.\nodara-cli.exe --version
.\nodara-cli.exe validate .\examples\hello-world.json
.\nodara-cli.exe simulate .\examples\hello-world.json
.\nodara-cli.exe run .\examples\delayed-log.json
.\nodara-cli.exe validate .\examples\branching.json --json
.\nodara-cli.exe inspect .\examples\hello-world.json --json
```

检查项：

- `run` 退出码为 0；
- 输出包含 `run completed`；
- `--json` 输出可被 `ConvertFrom-Json` 解析；
- 失败场景返回非 0，并在 stderr 给出结构化错误；
- `simulate` 只展示计划，不实际执行节点。

退出码检查：

```powershell
.\nodara-cli.exe run .\examples\hello-world.json --quiet
"exit code: $LASTEXITCODE"
```

## 5. Runtime 与插件测试

启动：

```powershell
.\nodara-runtime.exe
```

在另一窗口执行：

```powershell
$base = "http://127.0.0.1:8710/api/v1"
$health = Invoke-RestMethod "$base/health"
$plugins = Invoke-RestMethod "$base/plugins"
$types = Invoke-RestMethod "$base/node-types"
$schema = Invoke-RestMethod "$base/schema/workflow"

$health
$plugins.plugins | Select-Object id,version
"node types: $($types.node_types.Count)"
"plugin failures: $($plugins.failures.Count)"
"schema has core.Log: $($schema.definitions.NodeType.enum -contains 'core.Log')"
```

通过标准：

- `status` 为 `ok`；
- plugin 数为 2，failure 数为 0；
- node type 数为 16；
- 返回的 Schema 节点枚举包含当前插件节点。

重启后再次检查，确认插件可以稳定发现，不依赖上一次进程状态。

## 6. Studio 验收

保持 runtime 运行并启动 `nodara-studio.exe`。

### 6.1 连接与发现

- 右上角显示 `16 node types · 2 plugin(s)`；
- 左侧分类包含 Core、System、Input、Window、Desktop、Vision；
- 不存在插件加载失败提示。

### 6.2 工作流编辑

1. Import `examples\hello-world.json`；
2. 检查画布 `Start → Log → End`；
3. 修改 Log 的 message；
4. Export，确认 JSON 可解析且保留 `$schema`；
5. Validate，确认无 error。

### 6.3 运行与事件

1. Run；
2. Events 页最终显示 `run_completed`；
3. Runs 页刷新后能看到该运行，点击“打开”可重新载入完整事件；
4. `examples/failure-branch.json` 校验无错误，Calculate 节点失败后通过 failure 连线进入恢复分支；
5. `Variables…` 可设置临时变量且不修改工作流默认值，JSON 无效时 Run 保持禁用；
6. `Extensions` 页列出内置扩展以及已发现/已加载的插件、节点数和状态；
7. `examples/capture-preview.json` 在 Capture 事件下直接显示 PNG 预览，并能通过运行 artifact API 下载。
8. `examples/result-mapping.json` 将 Delay 的 `out` 发布为 `waited_ms`，后续 Log 能正确渲染。
9. 画布运行状态与事件一致；
10. Pause/Resume 对延迟工作流有效；
11. Cancel 能终止延迟节点；
12. 空闲、完成后或取消后点击 **Step**，运行时进入第一节点并暂停；连续点击时 `nodes_executed` 每次只增加 1，状态回到 `paused`；
13. 选中 `windows.Desktop.Capture` 后点击 **拖框选择截图区域**，Studio 隐藏并恢复，截图中不含选择弹窗；拖框读数和写回节点的 X/Y/宽度/高度一致；
14. `core.Log` 的消息为 artifact JSON（例如 `{{screenshot}}`）时，Events 页同样显示图片预览。

### 6.4 Audit

- Audit 页显示 capability decision、node finished 和 run completed；
- 过滤当前 run 后只剩对应记录；
- 刷新页面后历史记录仍由 runtime 返回。

### 6.5 安装包

分别测试 NSIS 和 MSI 中至少一个：

1. 双击 `installers\Nodara-Studio-2.0.0-x64-setup.exe`；
2. 完成当前用户安装；
3. 从开始菜单启动；
4. 连接 runtime 并执行 hello-world；
5. 从 Windows“设置 → 应用”卸载；
6. 重复安装，确认无重复入口和残留阻塞。

## 7. Agent 测试

### 7.1 能力发现和 mock 流程

保持 runtime 运行：

```powershell
.\nodara-agent.exe capabilities
$mock = '{"schema_version":"2.0","id":"wf.test","metadata":{"name":"Test"},"nodes":[{"id":"start","type":"core.Start"},{"id":"log","type":"core.Log","config":{"message":"agent ok"}},{"id":"end","type":"core.End"}],"edges":[{"id":"e1","source":"start","target":"log"},{"id":"e2","source":"log","target":"end"}]}'
.\nodara-agent.exe plan "log agent ok" --mock $mock --out .\agent-test.json
.\nodara-agent.exe run "log agent ok" --mock $mock --report
```

通过标准：规划文件通过 CLI 校验，运行成功并返回报告。

### 7.2 Guardrail

```powershell
.\nodara-agent.exe plan "按键 A" --safe --mock $mock
```

当 mock 计划包含未被 `--safe` 允许的节点时，必须拒绝而不是降级执行。真实 LLM
测试应使用测试账号并设置低预算，禁止使用生产凭据。

### 7.3 Trace 与 Replay

```powershell
.\nodara-agent.exe plan "log replay" --mock $mock --trace .\trace.jsonl
.\nodara-agent.exe replay .\trace.jsonl
```

trace 必须是逐行 JSON，replay 输出应包含目标、模型响应、验证和 guardrail 结果。

## 8. 权限与审批测试

只建议在专用测试机执行桌面控制节点。先执行 `simulate`，再用
`nodara-cli serve --require-approval` 启动带头部参数的 runtime，通过 Agent 发起绑定
会话的计划，并在 Studio Agent 页分别测试批准和拒绝：

- 批准后运行继续，审计记录 decision 和操作者；
- 拒绝后节点不执行，运行结束状态明确；
- 超时等价于拒绝；
- 未绑定会话的 gated run 被拒绝。

测试截图、剪贴板和键鼠前，先关闭包含未保存内容的窗口，并准备可恢复的系统快照。

## 9. Debug/Release 差异

| 检查 | Debug | Release |
|---|---|---|
| 优化 | 基础优化 + 完整调试信息 | `opt-level=3`、LTO、strip |
| PDB | 包含于 `symbols\` | 不包含 |
| 用途 | 开发复现、符号分析 | 用户验收、安装分发 |
| Studio 安装包 | 无 | NSIS + MSI |
| 程序行为 | 应与 release 一致 | 应与 debug 一致 |

两包的自动测试结果都应相同；行为和状态差异应视为缺陷。性能只以 release 报告。

## 10. 回归与结果记录

每次提交至少执行：

```powershell
.\scripts\build-artifacts.ps1 -Configuration All
```

脚本会先运行：

- `cargo test --workspace --no-fail-fast`（Core）；
- `cargo test --workspace --no-fail-fast`（Agent）；
- `npm test`（Studio）；
- TypeScript 检查和生产构建；
- Tauri debug/release 构建；
- NSIS/MSI 打包、SHA-256 和 ZIP 归档。

建议记录模板：

```text
构建：Nodara 2.0.0 / <debug|release> / <commit>
环境：Windows 版本、DPI、Defender、WebView2、VC++ 版本
结果：A01-A10 pass/fail
失败步骤：
实际结果：
预期结果：
附件：build-info.json、SHA256SUMS.txt、终端日志、trace/audit
```

## 11. 测试完成后的清理

停止 Studio、Agent、runtime 和控制台进程，删除 `verify-release` 临时目录。若测试过
桌面控制节点，恢复窗口和系统状态；若记录机密变量，先脱敏再提交 trace。
