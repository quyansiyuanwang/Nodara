# Nodara 使用手册

> English: [user-manual.md](user-manual.md)

本文说明如何使用预编译的 Nodara 2.0.0 Windows x64 产物完成日常操作、接口调用、
权限控制和故障排查。开发与打包流程见根目录 `README.md`、`QUICKSTART.zh.md` 和
`scripts/build-artifacts.ps1`。

## 1. 组件与运行关系

| 组件 | 可执行文件 | 作用 |
|---|---|---|
| CLI | `nodara-cli.exe` | 校验、模拟、运行、迁移、导出 Schema、启动 runtime 的开发入口 |
| Runtime | `nodara-runtime.exe` | 无界面 HTTP/WebSocket 服务，Studio 与 Agent 的共同服务端 |
| Platform 插件 | `nodara-platform-plugin.exe` | 键鼠、窗口、截图、剪贴板 |
| Vision 插件 | `nodara-vision-plugin.exe` | 模板匹配与 OCR |
| Agent | `nodara-agent.exe` | 将自然语言目标规划为工作流，并通过 runtime 观察执行 |
| Studio 桌面版 | `nodara-studio.exe` | 图形化工作流编辑、运行观察、审批和审计 |
| Studio Web | `studio-web\` | 相同前端的生产构建，需要反向代理 `/api` |

数据流是：

```text
Studio / Agent / CLI
          |
          v
nodara-runtime.exe  --->  plugins/*/*.exe
          |
          v
events + audit + run state
```

Studio 和 Agent 都不直接执行节点。runtime 是能力发现、策略判断、执行和审计的
唯一入口。

## 2. 启动方式

### 2.1 直接运行示例

不需要启动服务：

```powershell
.\nodara-cli.exe validate .\examples\hello-world.json
.\nodara-cli.exe simulate .\examples\hello-world.json
.\nodara-cli.exe run .\examples\hello-world.json
```

常用选项：

| 选项 | 说明 |
|---|---|
| `--var name=value` | 覆盖工作流变量，可重复 |
| `--plugin-dir DIR` | 加载指定目录中的插件，可重复 |
| `--in-process` | 将官方能力注册进当前进程，便于开发调试 |
| `--allow CAPABILITY` | 只允许指定能力或权限，可重复 |
| `--allow-all` | 允许全部能力，仅用于完全可信环境 |
| `--audit FILE` | 将审计记录以 JSON Lines 追加写入文件 |
| `--quiet` | 不打印实时事件 |
| `--json` | 输出结构化最终结果 |

### 2.2 启动 runtime

推荐在发布包根目录启动：

```powershell
.\nodara-runtime.exe
```

默认监听 `http://127.0.0.1:8710/api/v1`，自动扫描：

1. `nodara-runtime.exe` 同级的 `plugins\`；
2. 当前工作目录下的 `plugins\`；
3. `NODARA_PLUGIN_DIRS` 中按分号分隔的附加目录。

可用环境变量：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `NODARA_RUNTIME_PORT` | `8710` | 监听端口 |
| `NODARA_PLUGIN_DIRS` | 空 | 附加插件目录，多个目录用 `;` 分隔 |
| `RUST_LOG` | 由 tracing 默认值决定 | Rust 日志过滤条件，如 `info`、`nodara_runtime=debug` |

示例：

```powershell
$env:NODARA_RUNTIME_PORT = "8710"
$env:NODARA_PLUGIN_DIRS = "D:\NodaraPlugins;D:\TeamPlugins"
$env:RUST_LOG = "info"
.\nodara-runtime.exe
```

`nodara-runtime.exe` 只提供环境变量配置；需要 `--require-approval`、
`--allow`、`--audit` 等完整参数时使用源码构建的 CLI：

```powershell
cargo run -p nodara-cli -- serve --plugin-dir plugins --require-approval --audit audit.jsonl
```

### 2.3 启动 Studio（推荐）

直接执行：

```powershell
.\nodara-studio.exe
```

桌面版固定连接 `http://127.0.0.1:8710`。启动时它会：

1. 探测 8710 端口；已有 runtime 时直接复用；
2. 没有 runtime 时，启动与 `nodara-studio.exe` 同目录的
   `nodara-runtime.exe`，等待最多 8 秒；
3. runtime 启动后自动发现节点类型和插件；
4. 退出 Studio 时关闭由它启动的 runtime；手工启动的 runtime 不受影响。

debug 版 Studio 会故意保留控制台窗口，用于显示 runtime 日志和启动错误；自动启动
的 runtime 不会额外创建第二个控制台窗口。

如果连接标记显示 `runtime unreachable`，优先检查
`nodara-runtime.exe` 是否与 Studio 位于同一目录、8710 是否被其他程序占用。
Studio 每五秒自动重连。若不希望自动启动，可设置 `NODARA_RUNTIME_BIN` 指向其他
runtime 可执行文件，或先手工运行 runtime。

#### 界面语言

点击工具栏中的 `中文` / `English` 按钮切换语言。Studio 会记住选择，并翻译界面、节点名称、分类以及已知的配置字段标题和说明。

#### Studio 画布操作

| 操作 | 方法 |
|---|---|
| 添加节点 | 在左侧列表中**单击**节点，或拖到画布指定位置；每个工作流只允许一个 `core.Start` |
| 移动节点 | 按住节点拖动 |
| 创建连线 | 从输出端口拖动到输入端口 |
| 删除节点或连线 | 选中后按 `Delete`，或右键目标并选择删除 |
| 编辑连线 | 选中连线后，在属性面板中编辑标签或执行条件 |
| 自动校验 | 每次修改后自动执行；`Validate` 旁显示结果，有错误时会禁用 `Run` |
| 调整布局 | 拖动左右面板之间或底部面板上方的分隔条；双击恢复默认尺寸 |

#### 工作流设置与变量

未选中节点或连线时，属性面板可编辑工作流 ID、名称、描述和标签，并可新增、编辑或删除工作流变量，包括变量说明和敏感值标记。

#### 节点通用执行设置

属性面板中的“执行设置”适用于所有节点，并会写入工作流文档、由 runtime 实际执行：

| 设置 | 作用 |
|---|---|
| 启用 | 禁用后跳过该节点，并让入站分支通过出站连线继续执行 |
| 前置延时（毫秒） | 执行节点前等待 |
| 后置延时（毫秒） | 节点成功后再等待，然后激活后续分支 |
| 失败重试次数 | 首次失败后的额外执行次数 |
| 重试间隔（毫秒） | 两次失败尝试之间的等待时间 |

节点右键菜单提供“启用/禁用节点”快捷操作。

发布包中的 Web 版不能在 `file://` 下直接打开。开发时在源码仓库执行：

```powershell
Set-Location <仓库路径>\Nodara-Studio
npm ci
npm run dev
```

打开 `http://localhost:4173`。Vite 会把 `/api` 代理到 `NODARA_RUNTIME_URL` 指定的
地址，默认是 `http://127.0.0.1:8710`。

## 3. CLI 命令参考

| 命令 | 用途 | 是否执行节点 |
|---|---|---|
| `nodara-cli validate FILE` | 校验结构、图、节点类型和配置 | 否 |
| `nodara-cli simulate FILE` | 展示执行顺序和所需权限 | 否 |
| `nodara-cli run FILE` | 执行工作流 | 是 |
| `nodara-cli inspect FILE` | 输出节点、边和变量摘要 | 否 |
| `nodara-cli migrate FILE --out NEW` | 升级旧版工作流 | 否 |
| `nodara-cli plugins` | 列出发现的插件 | 否 |
| `nodara-cli schema --out schema` | 导出 JSON Schema | 否 |
| `nodara-cli serve` | 启动带完整参数的 runtime | 服务 |

常用示例：

```powershell
# 机器可读校验报告
.\nodara-cli.exe validate .\examples\branching.json --json

# 仅允许安全能力
.\nodara-cli.exe run .\examples\hello-world.json --allow core.Log --quiet

# 从指定插件目录加载能力
.\nodara-cli.exe plugins --plugin-dir .\plugins --json

# 查看命令参数
.\nodara-cli.exe <命令> --help
```

## 4. 工作流基本用法

工作流文件是 `schema_version: "2.0"` 的 JSON DAG。最小结构包含变量、节点和边：

```json
{
  "$schema": "../schema/workflow.schema.json",
  "schema_version": "2.0",
  "id": "workflow.hello",
  "metadata": { "name": "Hello" },
  "nodes": [
    { "id": "start", "type": "core.Start" },
    { "id": "log", "type": "core.Log", "config": { "message": "Hello, {{name}}!" } },
    { "id": "end", "type": "core.End" }
  ],
  "edges": [
    { "id": "e1", "source": "start", "target": "log" },
    { "id": "e2", "source": "log", "target": "end" }
  ],
  "variables": {
    "name": { "value": "World" }
  }
}
```

模板使用 `{{变量名}}`；变量可通过 `--var` 或 Agent 的 `--variables` 覆盖。当前
节点、端口、配置字段和权限的完整清单见
[nodes.zh.md](nodes.zh.md)。

编辑 JSON 时，`$schema` 可指向仓库内 Schema，也可在 runtime 运行时指向：

```text
http://127.0.0.1:8710/api/v1/schema/workflow
```

后者只包含当前部署实际安装的节点类型。

## 5. Studio 操作

### 编辑与校验

- 从左侧节点面板拖入节点，或双击添加；
- 拖动输出端口到输入端口建立连接；
- 选中节点后，在属性面板中编辑由插件 Schema 生成的字段；
- `Workflow JSON` 页可直接编辑并应用完整文档；
- `Validate` 调用 runtime 进行与执行前完全相同的校验。

### 运行与控制

- `Run` 启动新运行；
- `Pause`、`Resume`、`Step`、`Cancel` 控制当前运行；
- `Events` 页展示与 CLI 相同的事件序列；
- 画布高亮正在执行、成功或失败的节点；
- `Audit` 页按运行过滤策略决策、审批和节点结果。

### Agent 与审批

`Agent` 页只读取 runtime 中的会话，不直接调用 Agent 进程。运行时需要审批的节点会
阻塞，只有 Studio 或 `nodara-agent approve` 作出决定后才继续。审批界面会展示节点、
权限和将要传入的输入。

## 6. Agent 操作

先启动 runtime，再运行：

```powershell
.\nodara-agent.exe capabilities
.\nodara-agent.exe sessions
.\nodara-agent.exe audit
```

规划并输出到文件：

```powershell
$env:NODARA_LLM_API_KEY = "sk-..."
.\nodara-agent.exe plan "打开记事本并输入 hello" --safe --out .\plan.json
```

规划和执行：

```powershell
.\nodara-agent.exe run "读取剪贴板并写日志" --variables '{"expected":"hello"}' --timeout 60
```

控制运行：

```powershell
.\nodara-agent.exe control <run-id> pause
.\nodara-agent.exe control <run-id> resume
.\nodara-agent.exe control <run-id> cancel
```

审批：

```powershell
.\nodara-agent.exe sessions
.\nodara-agent.exe approve <session-id> <approval-id> --by tester
.\nodara-agent.exe approve <session-id> <approval-id> --deny --by tester
```

环境变量：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `NODARA_RUNTIME_URL` | `http://127.0.0.1:8710` | runtime 地址 |
| `NODARA_LLM_ENDPOINT` | OpenAI chat-completions 地址 | OpenAI 兼容接口 |
| `NODARA_LLM_MODEL` | `gpt-4o-mini` | 模型名 |
| `NODARA_LLM_API_KEY` | 无 | API Key；也接受 `OPENAI_API_KEY` |

无真实 Key 时，自动化测试可使用隐藏参数 `--mock '<workflow-json>'` 提供固定模型
响应。`--safe` 拒绝带副作用或声明权限的节点，`--allow <类型>` 建立显式白名单。

## 7. HTTP 与 WebSocket

常用只读端点：

```powershell
Invoke-RestMethod http://127.0.0.1:8710/api/v1/health
Invoke-RestMethod http://127.0.0.1:8710/api/v1/plugins
Invoke-RestMethod http://127.0.0.1:8710/api/v1/node-types
Invoke-RestMethod http://127.0.0.1:8710/api/v1/runs
Invoke-RestMethod http://127.0.0.1:8710/api/v1/audit
```

启动工作流：

```powershell
$workflow = Get-Content -Raw .\examples\hello-world.json | ConvertFrom-Json
$body = @{ workflow = $workflow; variables = @{ name = "API" } } | ConvertTo-Json -Depth 20
$run = Invoke-RestMethod `
  -Method Post `
  -ContentType "application/json" `
  -Body $body `
  http://127.0.0.1:8710/api/v1/runs

$run.id
Invoke-RestMethod "http://127.0.0.1:8710/api/v1/runs/$($run.id)/event-log"
```

完整的请求和响应结构见
[Nodara-Core/protocol/runtime-api.md](../Nodara-Core/protocol/runtime-api.md)。固定
版本是 HTTP `v1`、工作流 Schema `2.0`、插件协议 `1`。

## 8. 权限、审批与审计

节点可声明权限。runtime 在每次执行前调用策略：

| 策略模式 | 行为 |
|---|---|
| `DefaultPolicy` | 安全节点放行，特权节点请求审批 |
| `AllowlistPolicy` | 仅允许显式能力/权限，其余拒绝 |
| `AllowAllPolicy` | 全部允许，仅用于受信任环境 |
| `PolicyChain` | 拒绝优先，其次审批，最后放行 |

`nodara-cli serve` 默认自动批准，适合无人值守测试；加 `--require-approval` 后，
需要审批的节点必须绑定 Agent 会话，并由操作员批准，超时视为拒绝。独立 CLI
`nodara-cli run` 使用自动审批。审计记录可通过 `--audit FILE` 持久化；未指定文件
时只保存在进程内存中。

## 9. 排障

| 现象 | 检查与处理 |
|---|---|
| `runtime listening` 后立即退出 | 端口被占用；改用 `NODARA_RUNTIME_PORT=8720` 做 API 测试，或在 Studio 联调时释放 8710 |
| Studio 显示 `runtime unreachable` | 确认同目录存在 `nodara-runtime.exe`；检查 8710 端口、防火墙和 debug 控制台中的启动错误 |
| 只显示 14 个节点 | `plugins\` 中的两个插件没有加载；检查插件 exe 是否与 manifest 同目录、是否被杀毒软件隔离 |
| `unknown node type` | 启动 runtime 时加载对应插件，或对 CLI 传入正确的 `--plugin-dir` |
| `no API key` | 设置 `NODARA_LLM_API_KEY` 或 `OPENAI_API_KEY`；离线自动化改用 `--mock` |
| Schema 补全缺少第三方节点 | `$schema` 尚未指向当前 runtime，或 runtime 未加载插件 |
| release 程序无法启动 | 安装 Microsoft Visual C++ 2015-2022 x64 Redistributable；桌面 Studio 还需要 WebView2 Runtime |
| 运行被审批阻塞 | 执行 `nodara-agent sessions`，再 approve/deny 对应条目 |
| 工作流被拒绝 | 运行 `nodara-cli validate FILE --json`，按诊断码和 JSON 指针修复 |

日志可通过提高 `RUST_LOG` 详细度获得：

```powershell
$env:RUST_LOG = "nodara_runtime=debug,nodara_plugin=debug,info"
.\nodara-runtime.exe
```

## 10. 退出与清理

- Studio：直接关闭窗口；
- runtime / Agent：在各自控制台按 `Ctrl+C`；
- 安装版 Studio：通过 Windows“设置 → 应用”卸载；
- 测试生成的 `audit.jsonl`、trace 和临时工作流可单独删除；
- 不要删除正在运行进程占用的插件 exe。

如需提交缺陷，请同时提供 `build-info.json`、`SHA256SUMS.txt`、复现命令、完整终端
输出和相关 trace/audit 文件；提交前确认其中不包含 API Key 或敏感工作流变量。