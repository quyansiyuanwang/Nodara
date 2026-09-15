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
| Platform 插件 | `nodara-platform-plugin.exe` | 键鼠、窗口、截图、剪贴板、进程执行 |
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
未保存的编辑内容会保存在当前标签页的会话存储中，因此切换语言或意外刷新页面后会恢复草稿；
关闭标签页后该副本会清除，不会长期保存包含敏感变量的工作流。

#### Studio 画布操作

| 操作 | 方法 |
|---|---|
| 添加节点 | 在左侧列表中**单击**节点，或拖到画布指定位置；每个工作流只允许一个 `core.Start` |
| 移动节点 | 按住节点拖动 |
| 创建数据连线 | 从圆形数据输出拖到圆形数据输入；按 `Esc` 取消；悬停显示名称、方向、类型和用途 |
| 创建执行连线 | 从右下角 Always / Success / Failure 菱形输出拖到左下角执行输入 |
| 框选与多选 | 空白处左键拖动框选；`Ctrl+左键` 切换单个节点；`Shift+左键` 选取控制图中锚点到目标的所有有向路径节点 |
| 多节点操作 | 拖动任一已选节点会移动全部；删除、启停、断点应用整组；浮动快配置可批量设置常用执行字段 |
| 删除节点或连线 | 选中后按 `Delete`，或右键目标并选择删除 |
| 编辑连线 | 选中连线后可编辑标签或执行条件，也可右键直接切换“始终 / 成功时 / 失败时”；加宽的透明命中区域让细线更容易选中 |
| 自动校验 | 每次修改后自动执行；`Validate` 旁显示结果，有错误时会禁用 `Run` |
| 调整布局 | 拖动左右面板之间或底部面板上方的分隔条；双击恢复默认尺寸 |
| 折叠配置区 | 属性面板中的“执行设置”“配置”“变量”均可折叠，Studio 会记住展开状态 |
| 过滤事件 | Events 工具栏可按类型、节点、消息和输出内容过滤；清空只清除当前视图，自动跟随控制是否滚动到最新事件 |
| 撤销 / 重做 | 使用工具栏按钮、`Ctrl+Z`、`Ctrl+Y` 或 `Ctrl+Shift+Z` |
| 复制节点 | `Ctrl+D` 或右键选择复制；`core.Start` 只能存在一个，不能复制或重复导入 |
| 浏览画布 | 世界坐标无边界；中键或 `Space+左键` 平移，滚轮缩放；负坐标也可正常适应和自动布局 |
| 自动布局 | 点击“自动布局”，按拓扑层级从左到右整理节点 |

#### 工作流设置与变量

未选中节点或连线时，属性面板可编辑工作流 ID、名称、描述、标签、作者和版本，并可新增、编辑或删除工作流变量，包括变量说明和敏感值标记。每个变量还可设置仅当前 Studio 会话生效的“运行值覆盖”，它会在启动运行时不修改工作流默认值。Schema 中的数组配置会以可排序列表渲染，支持添加、删除和恢复默认值。

#### 节点通用执行设置

属性面板中的“执行设置”适用于所有节点，并会写入工作流文档、由 runtime 实际执行：

| 设置 | 作用 |
|---|---|
| 启用 | 禁用后跳过该节点，并让入站分支通过出站连线继续执行 |
| 运行条件 | 可选表达式；结果为假时跳过节点并剪除后续分支 |
| 节点前断点 | 执行此节点前自动暂停；“继续”恢复运行，“单步”只执行此节点 |
| 前置延时（毫秒） | 执行节点前等待 |
| 后置延时（毫秒） | 节点成功后再等待，然后激活后续分支 |
| 失败后继续 | 重试耗尽后仍继续执行符合条件的“始终/失败时”连线，而不是终止整个运行 |
| 超时时间（毫秒） | 插件单次执行尝试的可选最长时间 |
| 失败重试次数 | 首次失败后的额外执行次数 |
| 重试间隔（毫秒） | 两次失败尝试之间的等待时间 |
| 重试退避 | 保持固定间隔，或在每次失败后将间隔翻倍 |
| 最大重试间隔（毫秒） | 可选，限制计算后的最大等待时间 |
| 结果存入变量 | 将节点某个输出端口发布到运行变量，供后续节点和表达式使用 |
| 结果端口 | “结果存入变量”选择的输出端口；留空时使用 `out` 或第一个输出 |

节点右键菜单提供“启用/禁用节点”和“设置/清除断点”快捷操作；选中节点后按 `F9` 也可切换断点。

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
| `nodara-cli extensions` | 统一列出内置、进程内和插件扩展注册 | 指定插件目录时会启动插件 |
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

工作流文件是 `schema_version: "2.1"` 的 JSON DAG。最小结构包含变量、节点和边：

```json
{
  "$schema": "../schema/workflow.schema.json",
  "schema_version": "2.1",
  "id": "workflow.hello",
  "metadata": { "name": "Hello" },
  "nodes": [
    { "id": "start", "type": "core.Start" },
    { "id": "log", "type": "core.Log", "config": { "message": "Hello, {{name}}!" } },
    { "id": "end", "type": "core.End" }
  ],
  "edges": [
    { "id": "e1", "kind": "control", "source": "start", "target": "log" },
    { "id": "e2", "kind": "control", "source": "log", "target": "end" }
  ],
  "variables": {
    "name": { "value": "World" }
  }
}
```

模板使用 `{{变量名}}`；变量可通过 `--var` 或 Agent 的 `--variables` 覆盖。配置值中如果只有一个
完整模板，会按目标字段类型保留数值、布尔值、数组或对象，因此 `"{{match.x}}"` 可以直接用于坐标、
时长等数值字段；`"x={{match.x}}"` 这样的混合文本仍是字符串。Studio 的数值和布尔字段提供 `{}`
按钮，可在直接值与模板表达式之间切换。当前节点、端口、配置字段和权限的完整清单见
[nodes.zh.md](nodes.zh.md)。

编辑 JSON 时，`$schema` 可指向仓库内 Schema，也可在 runtime 运行时指向：

```text
http://127.0.0.1:8710/api/v1/schema/workflow
```

后者只包含当前部署实际安装的节点类型。

### 2.1 控制边与数据边

每条边必须有 `kind`。`control` 边只决定执行路径，可设置 `branch=always|success|failure`、`condition` 和 `label`，但不得设置端口；`data` 边必须设置 `source_port`、`target_port`，不得设置控制字段。拓扑、入口、可达性、死路和循环检测只基于控制边。数据边只在目标已被控制边激活后把来源输出填入 `NodeInput.inputs`。

旧 2.0 文档会以 `WF118` 阻止运行：

```powershell
.\nodara-cli.exe migrate .\legacy.json --out .\legacy-2.1.json
```

迁移将所有旧边写为 `control`，移除旧端口映射，并逐条提示需要手工重建的数据线。
## 5. Studio 操作

### 编辑与校验

- 从左侧节点面板拖入节点，或双击添加；
- 拖动输出端口到输入端口建立连接；
- 选中节点后，在属性面板中编辑由插件 Schema 生成的字段；
- `Workflow JSON` 页可直接编辑并应用完整文档；
- `Validate` 调用 runtime 进行与执行前完全相同的校验。
- Problems 中能够解析到节点或连线的诊断可以直接点击，画布会居中目标并打开对应属性，便于立即修复。

### 运行与控制

- `Run` 启动并连续执行新运行；
- `Variables…` 集中编辑本次运行变量覆盖，不修改工作流默认值；敏感值不会在弹窗中显示，JSON 无效时无法启动运行；
- `Pause`、`Resume`、`Cancel` 控制当前运行；暂停和继续会写入事件流，节点断点会在执行该节点前自动暂停，即使运行不是以暂停状态启动；
- `Step` 用于单步调试：空闲、完成、失败或取消后点击时，会以暂停状态新建运行并执行第一个节点；运行暂停后，每次点击只执行一个节点，并继续保持暂停；
- `Events` 页展示与 CLI 相同的事件序列；向上滚动查看历史时会自动暂停跟随，再滚回底部后恢复；
- `Runs` 页列出 runtime 中的历史运行，显示状态、工作流、节点数、事件数、产物数、准确的开始/结束时间和耗时；可按文本与状态筛选，点击 `打开` 可重新载入该运行的完整事件；
- `Extensions` 页展示 runtime 统一注册的内置、进程内和插件扩展，包括来源、能力数、节点类型数和加载状态；
- 画布高亮正在执行、成功或失败的节点；
- `Audit` 页按运行过滤策略决策、审批和节点结果。

### 截图与 artifact 调试

窗口查找、聚焦、窗口截图以及键盘/鼠标/文本输入的“聚焦目标窗口”共用同一套筛选条件：

- `标题`：按窗口标题匹配；
- `窗口类`：按 Win32 class 匹配；
- `进程名`：按拥有窗口的可执行文件匹配，例如 `notepad.exe`，不区分大小写；
- `精确匹配`：关闭时使用包含匹配，开启时三个条件都必须完全相等；
- `仅可见窗口`：默认开启，隐藏窗口不会被选中。

节点会从所有匹配窗口中选取面积最大的一个，并在 Find 节点输出 `process` 与 `visible`
字段，便于后续日志和条件分支判断。

需要等待应用启动时使用 `windows.Window.Wait`：它按同一套条件轮询，支持
`mode=appear|disappear`、`wait_timeout_ms` 和 `poll_interval_ms`，窗口出现或消失后输出最后一个匹配记录，超时返回
`E_TIMEOUT`。

输入节点现已区分更多真实操作阶段：

- 键盘 `action=type` 会按下并释放；可设置 `hold_ms` 控制按住时长；`press`/`release`
  可显式保持和释放组合键；
- 键盘、文本和鼠标节点可开启 `background`，向指定窗口发送输入而不改变当前焦点；
  文本节点还可选择 `set_text` 直接替换窗口文本，或 `clipboard` 临时写入剪贴板、发送 Ctrl+V，再恢复原剪贴板；
- 键盘 `action=type` 可通过 `repeat` 与 `repeat_interval_ms` 重复发送；
- 鼠标可选 `left/right/middle` 按钮，并通过 `click_count` 与 `click_interval_ms` 控制重复点击；
- 鼠标 `relative=true` 时，X/Y 是相对当前光标的位置；
- 鼠标 `drag` 支持可选起点 `start_x/start_y`、终点 `x/y` 和
  `duration_ms` 平滑移动时间；
- 后台鼠标使用窗口客户区坐标，发送 `WM_MOUSEMOVE` 和对应按键消息，并在操作结束后恢复原系统光标位置；
- 双击间隔可通过 `double_click_interval_ms` 调整。

`background` 必须配合标题、窗口类或进程名选择器，不能与 `focus` 同时开启；后台模式不适合所有应用，
部分控件会忽略消息输入。

截图节点会发布 artifact 元数据（`id`、`name`、`content_type`、`size`）。图片字节会从插件进程传回 runtime，并保留在该次运行中。在 Studio 的 **Events** 页找到 Capture 节点的 `node_finished` 事件，可直接看到图片预览和“打开”链接。

如果使用 `core.Log` 输出 artifact JSON（例如消息为 `{{screenshot}}`），对应的 `log` 事件也会识别其中的图片元数据并显示同样的内联预览。嵌套对象和数组中的 artifact 也会被递归识别。每个 `node_finished` 事件还提供可展开的完整 JSON 输出，便于检查 OCR、模板匹配和插件自定义结果。

`vision.TemplateMatch` 可用 `region_x`、`region_y`、`region_width` 和 `region_height` 限制搜索范围，
可通过 `fail_if_missing` 在未命中时直接失败，并输出 `center_x` / `center_y`。后续鼠标节点可直接使用这些值，
例如 `"x": "{{hit.center_x}}"`、`"y": "{{hit.center_y}}"`。

要直接用鼠标定位 `windows.Desktop.Capture` 的截图矩形：

1. 在画布中选中 Capture 节点；
2. 在配置区点击 **拖框选择截图区域**；
3. Studio 会临时隐藏自身，通过 runtime 截取当前桌面，再显示全屏截图；
4. 按住鼠标左键拖动矩形；下方实时显示 X、Y、宽度和高度；
5. 点击 **应用区域**，四个像素值会直接写入当前节点配置并触发自动校验。

浏览器版无法隐藏 Studio 窗口，应在点击前先安排好要截取的目标窗口。框选使用原始图片像素坐标，不受预览缩放比例影响。

其他节点需要 artifact ID 时使用 `{{screenshot.id}}`；诊断可使用 `{{screenshot.size}}` 或 `{{screenshot.content_type}}`。可直接运行 `examples/capture-preview.json` 验证。

API 调试：

```text
GET /api/v1/runs/{run_id}/artifacts
GET /api/v1/runs/{run_id}/artifacts/{artifact_id}
```

第一个接口返回元数据，第二个接口按 MIME 类型返回原始字节。artifact 在当前 runtime 保留运行记录期间可访问。

### 启动外部命令

`system.Command` 可直接启动程序或 Shell 命令，并将 stdout、stderr、退出码和 PID
发布到输出端口。常用配置全部在节点属性面板中：

| 配置 | 说明 |
|---|---|
| 程序或命令 | 要执行的程序；开启“通过 Shell 运行”后也可使用命令和 Windows 内置命令 |
| 参数 | 参数列表，每项会自动作为独立参数传递 |
| 工作目录 / 环境变量 | 控制进程启动目录和附加环境变量 |
| 标准输入 | 可选文本，支持 `{{变量}}` 插值 |
| 非零退出时失败 | 默认开启；关闭后可在下游读取 `exit_code`、`stdout`、`stderr` |
| 等待完成 | 关闭时后台启动，只返回 `pid` |

节点默认将 stdout 放在 `out` 端口，因此通用“结果存入变量”设置可直接把命令输出传给
后续节点。Command 完成后，Events 页会直接展开 stdout、stderr、退出码和 PID。
节点超时和运行取消都会终止整个子进程树。该节点需要
`process.execute` 权限；`examples/system-command.json` 提供了可直接运行的示例。

### 失败恢复上下文

节点执行失败时，运行作用域会发布 `last_error`，包含 `code`、`message`、`node_id` 和 `retryable`。失败分支可在条件表达式中使用这些字段，后续节点也可用 `{{last_error.code}}` 等模板渲染。

### Agent 对话与审批

桌面 Studio 的 `Agent` 页会调用同目录的 `nodara-agent.exe studio`，并把结构化请求
通过 stdin/stdout 传给 Agent。Provider 可填写 OpenAI-compatible Endpoint、Model、
API Key 和超时；API Key 仅保存在当前 Studio 进程内存。会话列表、完整消息历史、计划
JSON、审批和运行记录都由 runtime session 保存。

四档执行模式：

| 模式 | 行为 |
|---|---|
| 仅规划 | 只生成、修改和校验，不提供运行 |
| 手动执行 | 结果确认后以 paused 启动，由操作者 Resume |
| 部分审批 | 安全节点自动执行，危险/特权节点等待逐项审批 |
| 自动执行 | 自动运行并自动放行，但仍记录 capability decision 和 audit |

修改基线可选“当前画布”或“上一轮 Agent 计划”。Agent 结果永远不会自动覆盖画布；
先检查最终 JSON 和诊断，再使用“载入画布”“验证”“运行此计划”或“打开对应审计”。

## 6. Agent CLI 与桌面 Agent 操作

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
