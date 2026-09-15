# 节点参考

> English: [nodes.md](nodes.md)

这里是默认构建随附的节点类型目录：5 个核心节点、3 个系统节点、3 个输入节点、3 个窗口节点、
桌面捕获与 2 个视觉节点。部署方可以通过安装插件扩展它们 —— 调用 `GET /api/v1/node-types`，
或运行 `nodara-cli simulate <工作流>` 查看某个文档需要哪些节点类型 —— 发布的工作流 schema 也会随之增长。

每个条目列出端口、策略行为与全部配置项。同样的信息可由 `GET /api/v1/node-types` 机器读取，
描述符字段的含义见 [Schema 详解](schema.zh.md#5-节点描述符)。

## 如何阅读条目

* **端口** —— `in`/`out` 端口名与抽象值类型（`any`、`string`、`number`、`object`、`image`、
  `window`）。边把输出连到输入；未指定端口时默认 `out` → `in`。
* **策略** —— 带权限的节点属于**特权节点**。默认策略下特权节点执行前需要操作员审批；
  `--allow-all` 时全部放行，白名单模式下未列出的全部拒绝。权限也是 Agent 的 `--safe` 与 CLI 的
  `--allow` 的作用对象。
* **必填** —— 缺少必填项时校验会报 `WF142`。有默认值的键通常是可选的。
* **模板** —— 任何字符串值都支持 `{{变量}}` 插值。

### 权限与默认策略

| 权限 | 节点 | 默认策略 |
|---|---|---|
| `input.control` | `windows.Input.Keyboard`、`windows.Input.Text`、`windows.Input.Mouse` | 需要审批 |
| `window.control` | `windows.Window.Focus` | 需要审批 |
| `screen.capture` | `windows.Desktop.Capture`、`windows.Window.Capture` | 需要审批 |
| `clipboard` | `system.Clipboard` | 需要审批 |
| `process.execute` | `system.Command` | 需要审批 |
| `vision.analyze` | `vision.TemplateMatch`、`vision.Ocr` | 需要审批 |
| — | `core.*`、`system.Delay`、`windows.Window.Find` | 直接放行 |

### 汇总

| 节点类型 | 分类 | 端口 | 权限 |
|---|---|---|---|
| `core.Start` | Core | out `out` | — |
| `core.End` | Core | in `in` | — |
| `core.Log` | Core | in `in` → out `out` | — |
| `core.Calculate` | Core | in `in` → out `result` | — |
| `core.SetVariable` | Core | in `in` → out `out` | — |
| `system.Delay` | System | in `in` → out `out` | — |
| `system.Clipboard` | System | in `in` → out `out` | `clipboard` |
| `system.Command` | System | in `in` → out `out`、`stderr`、`exit_code`、`success`、`pid` | `process.execute` |
| `windows.Input.Keyboard` | Input | in `in` → out `out` | `input.control` |
| `windows.Input.Text` | Input | in `in` → out `out` | `input.control` |
| `windows.Input.Mouse` | Input | in `in` → out `out` | `input.control` |
| `windows.Window.Find` | Window | in `in` → out `window` | — |
| `windows.Window.Focus` | Window | in `in` → out `out` | `window.control` |
| `windows.Window.Capture` | Window | in `in` → out `artifact` | `screen.capture` |
| `windows.Desktop.Capture` | Desktop | in `in` → out `artifact` | `screen.capture` |
| `vision.TemplateMatch` | Vision | in `in` → out `match` | `vision.analyze` |
| `vision.Ocr` | Vision | in `in` → out `text` | `vision.analyze` |

## Core（核心）

### `core.Start` — Start

工作流入口。一个可运行的文档必须有且仅有一个（缺失报 `WF120`，多个报 `WF121`）。不需要任何配置。

* 端口：out `out`（any）
* 策略：始终放行

```json
{ "id": "start", "type": "core.Start", "label": "Start" }
```

### `core.End` — End

结束工作流。要求正常终止的文档需要一个（`WF122`）。

* 端口：in `in`（any）
* 策略：始终放行

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `code` | integer | 否 | `0` | 工作流结束时运行时上报的退出码。 |

```json
{ "id": "end", "type": "core.End", "config": { "code": 0 } }
```

### `core.Log` — Log

把消息写进运行日志与事件流，同时在 `out` 端口发布该消息，便于串联。

* 端口：in `in`（any）→ out `out`（string）
* 策略：始终放行

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `message` | string | **是** | — | 消息模板，支持 `{{变量}}` 插值。示例：`Hello, {{name}}!` |
| `level` | string | 否 | `info` | 日志级别：`debug`、`info`、`warn`、`error`。 |

```json
{ "id": "greet", "type": "core.Log",
  "config": { "message": "Hello, {{name}}!", "level": "info" } }
```

### `core.Calculate` — Calculate

基于运行作用域求算术与比较表达式，并把结果发布为变量。支持 `+`、`-`、`*`、`/`、`%`、`^`、比较运算以及 `&&`/`||`。

* 端口：in `in`（any）→ out `result`（number）
* 策略：始终放行

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `expression` | string | **是** | — | 算术表达式，可引用其它变量。示例：`2 + 2 * 3`、`{{price}} * {{quantity}}` |
| `output_var` | string | **是** | — | 结果发布到的变量名，如 `answer`。 |

```json
{ "id": "compute", "type": "core.Calculate",
  "config": { "expression": "2 + 2 * 3", "output_var": "answer" } }
```

### `core.SetVariable` — Set Variable

把任意 JSON 值写入运行作用域。字符串支持 `{{变量}}` 插值，因此它也是工作流归一化或重命名数据的方式。

* 端口：in `in`（any）→ out `out`（any）
* 策略：始终放行

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `name` | string | **是** | — | 值发布到的变量名。 |
| `value` | any | 否 | — | 要存储的值；接受任意 JSON 值，字符串会做插值。 |

```json
{ "id": "remember", "type": "core.SetVariable",
  "config": { "name": "greeting", "value": "Hello, {{name}}!" } }
```

## System（系统）

### `system.Delay` — Delay

等待固定时长。等待过程可被中断：睡眠期间会观察取消信号，因此长时间延时的 `cancel` 也能及时生效。

* 端口：in `in`（any）→ out `out`（any）
* 策略：始终放行

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `duration_ms` | integer | **是** | `0` | 等待毫秒数，最小 `0`。 |

```json
{ "id": "wait", "type": "system.Delay", "config": { "duration_ms": 1500 } }
```

### `system.Clipboard` — Clipboard

把剪贴板内容读入运行，或替换剪贴板文本。

* 端口：in `in`（any）→ out `out`（string）
* 策略：**特权** —— 权限 `clipboard`

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `action` | string | **是** | `read` | `read` 或 `write`。 |
| `text` | string | `action` 为 `write` 时 | — | 要写入剪贴板的文本，支持 `{{变量}}`。 |
| `output_var` | string | 否 | — | 读取时接收剪贴板文本的变量。 |

```json
{ "id": "read_clip", "type": "system.Clipboard",
  "config": { "action": "read", "output_var": "clip" } }
```

### `system.Command` — Command

启动外部程序或 Shell 命令，等待结束后捕获标准输出、标准错误和退出码。进程执行期间会响应
取消与节点超时；超时或取消时会终止整个子进程树。

* 端口：in `in`（any）→ out `out`（string，stdout）、`stderr`（string）、
  `exit_code`（number）、`success`（boolean）、`pid`（number）
* 策略：**特权节点** —— 权限 `process.execute`

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `program` | string | **是** | — | 要启动的可执行文件；开启 `shell` 后也可以是命令或 Windows 内置命令。 |
| `args` | string[] | 否 | `[]` | 传给程序的参数列表。 |
| `cwd` | string | 否 | — | 工作目录；留空时使用插件进程目录。 |
| `env` | object | 否 | `{}` | 合并到继承环境中的附加环境变量。 |
| `shell` | boolean | 否 | `true` | Windows 使用 `cmd.exe /C`，其他系统使用 `sh -c`；关闭后直接启动程序。 |
| `stdin` | string | 否 | — | 写入进程标准输入的文本，支持模板插值。 |
| `check_exit_code` | boolean | 否 | `true` | 非零退出时让节点失败；关闭后由下游检查输出。 |
| `wait` | boolean | 否 | `true` | 是否等待进程完成；关闭后立即返回 `pid`。 |

```json
{
  "id": "command",
  "type": "system.Command",
  "config": {
    "program": "echo",
    "args": ["hello"],
    "shell": true,
    "check_exit_code": true,
    "wait": true
  },
  "result_var": "command_output",
  "result_port": "out"
}
```

## Input（输入）

这三个输入节点都是**特权**节点（`input.control`），把输入发送到当前焦点窗口。

### `windows.Input.Keyboard` — Keyboard

发送按键或组合键。

* 端口：in `in`（any）→ out `out`（string）
* 策略：**特权** —— 权限 `input.control`

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `keys` | string | **是** | — | 按键或组合键，修饰键与按键用 `+` 连接。示例：`ctrl+shift+s`、`win+i`、`enter` |
| `focus` | boolean | 否 | `false` | 发送按键前查找并聚焦目标窗口。 |
| `title` | string | 否 | — | `focus` 为 true 时匹配的目标窗口标题。 |
| `class` | string | 否 | — | `focus` 为 true 时匹配的目标 Win32 窗口类名。 |
| `process` | string | 否 | — | `focus` 为 true 时匹配的目标进程名，例如 `notepad.exe`；不区分大小写。 |
| `exact` | boolean | 否 | `false` | 要求标题、窗口类和进程名精确匹配。 |
| `visible_only` | boolean | 否 | `true` | 仅搜索可见窗口。 |

```json
{ "id": "open_settings", "type": "windows.Input.Keyboard",
  "config": { "keys": "win+i" } }
```

### `windows.Input.Text` — Text

向焦点窗口输入字面文本。

* 端口：in `in`（any）→ out `out`（string）
* 策略：**特权** —— 权限 `input.control`

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `text` | string | **是** | — | 要输入的字面文本。 |
| `interval_ms` | integer | 否 | `10` | 按键间隔；`0` 表示以窗口能接受的最快速度输入。最小 `0`。 |
| `focus` | boolean | 否 | `false` | 输入文本前查找并聚焦目标窗口。 |
| `title` | string | 否 | — | `focus` 为 true 时匹配的目标窗口标题。 |
| `class` | string | 否 | — | `focus` 为 true 时匹配的目标 Win32 窗口类名。 |
| `process` | string | 否 | — | `focus` 为 true 时匹配的目标进程名，例如 `notepad.exe`；不区分大小写。 |
| `exact` | boolean | 否 | `false` | 要求标题、窗口类和进程名精确匹配。 |
| `visible_only` | boolean | 否 | `true` | 仅搜索可见窗口。 |

```json
{ "id": "type_note", "type": "windows.Input.Text",
  "config": { "text": "Hello from Nodara", "interval_ms": 10 } }
```

### `windows.Input.Mouse` — Mouse

移动光标并合成鼠标按键。在光标当前位置点击时可以不写 `x`/`y`。

* 端口：in `in`（any）→ out `out`（object：`action`、`moved`、`foreground`）
* 策略：**特权** —— 权限 `input.control`

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `action` | string | **是** | `click` | `move`、`click`、`double_click`、`right_click`、`middle_click`、`down`、`up`。 |
| `x` | integer | 否 | — | 屏幕绝对 X 像素。 |
| `y` | integer | 否 | — | 屏幕绝对 Y 像素。 |
| `focus` | boolean | 否 | `false` | 执行鼠标操作前查找并聚焦目标窗口。 |
| `title` | string | 否 | — | `focus` 为 true 时匹配的目标窗口标题。 |
| `class` | string | 否 | — | `focus` 为 true 时匹配的目标 Win32 窗口类名。 |
| `process` | string | 否 | — | `focus` 为 true 时匹配的目标进程名，例如 `notepad.exe`；不区分大小写。 |
| `exact` | boolean | 否 | `false` | 要求标题、窗口类和进程名精确匹配。 |
| `visible_only` | boolean | 否 | `true` | 仅搜索可见窗口。 |

```json
{ "id": "click_ok", "type": "windows.Input.Mouse",
  "config": { "action": "click", "x": 640, "y": 480 } }
```

## Window（窗口）

### `windows.Window.Find` — Find Window

定位唯一一个窗口，并发布记录供后续节点使用。它只读取窗口元数据，因此不需要权限。

* 端口：in `in`（any）→ out `window`（window）
* 策略：始终放行

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `title` | string | 否 | — | 要匹配的窗口标题；除非设置 `exact`，否则为子串匹配。示例：`Notepad`、`Settings` |
| `class` | string | 否 | — | 要匹配的 Win32 窗口类名，如 `Notepad`。 |
| `process` | string | 否 | — | 拥有窗口的可执行文件名，例如 `notepad.exe`；不区分大小写。 |
| `exact` | boolean | 否 | `false` | 要求标题、窗口类和进程名精确匹配。 |
| `visible_only` | boolean | 否 | `true` | 搜索时忽略隐藏窗口。 |
| `foreground` | boolean | 否 | `false` | 使用前台窗口，覆盖标题、窗口类与进程筛选。 |
| `output_var` | string | **是** | — | 接收窗口记录的变量：`handle`、`title`、`class`、`process`、`visible`、`rect`。 |

```json
{ "id": "find", "type": "windows.Window.Find",
  "config": { "title": "Notepad", "output_var": "notepad" } }
```

### `windows.Window.Focus` — Focus Window

把匹配到的窗口置于前台。

* 端口：in `in`（any）→ out `out`（window）
* 策略：**特权** —— 权限 `window.control`

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `title` | string | 否 | — | 要匹配的窗口标题；除非设置 `exact`，否则为子串匹配。 |
| `class` | string | 否 | — | 要匹配的 Win32 窗口类名。 |
| `process` | string | 否 | — | 拥有窗口的可执行文件名。 |
| `exact` | boolean | 否 | `false` | 要求标题、窗口类和进程名精确匹配。 |
| `visible_only` | boolean | 否 | `true` | 搜索时忽略隐藏窗口。 |
| `foreground` | boolean | 否 | `false` | 使用前台窗口，覆盖标题、窗口类与进程筛选。 |

```json
{ "id": "focus", "type": "windows.Window.Focus",
  "config": { "title": "Notepad" } }
```

### `windows.Window.Capture` — Capture Window

捕获窗口并存为图像 artefact，后续视觉节点可按 id 读取。

* 端口：in `in`（any）→ out `artifact`（image）
* 策略：**特权** —— 权限 `screen.capture`

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `title` | string | 否 | — | 要匹配的窗口标题；除非设置 `exact`，否则为子串匹配。 |
| `class` | string | 否 | — | 要匹配的 Win32 窗口类名。 |
| `process` | string | 否 | — | 拥有窗口的可执行文件名。 |
| `exact` | boolean | 否 | `false` | 要求标题、窗口类和进程名精确匹配。 |
| `visible_only` | boolean | 否 | `true` | 搜索时忽略隐藏窗口。 |
| `foreground` | boolean | 否 | `false` | 使用前台窗口，覆盖标题、窗口类与进程筛选。 |
| `output_var` | string | **是** | — | 接收捕获 artefact 元数据的变量。 |

```json
{ "id": "grab", "type": "windows.Window.Capture",
  "config": { "title": "Notepad", "output_var": "shot" } }
```

## Desktop（桌面）

### `windows.Desktop.Capture` — Capture Desktop

捕获整个主显示器，或它的一个矩形区域。

* 端口：in `in`（any）→ out `artifact`（image）
* 策略：**特权** —— 权限 `screen.capture`

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `x` | integer | 否 | `0` | 区域左边界（屏幕像素）。 |
| `y` | integer | 否 | `0` | 区域上边界（屏幕像素）。 |
| `width` | integer | 否 | 整屏宽度 | 区域宽度（像素），最小 `1`。 |
| `height` | integer | 否 | 整屏高度 | 区域高度（像素），最小 `1`。 |
| `output_var` | string | **是** | — | 接收捕获 artefact 元数据的变量。 |

```json
{ "id": "grab", "type": "windows.Desktop.Capture",
  "config": { "x": 0, "y": 0, "width": 800, "height": 600, "output_var": "shot" } }
```

## Vision（视觉）

### `vision.TemplateMatch` — Template Match

用归一化互相关（ZNCC）在捕获帧中查找模板图像，并发布匹配结果。

* 端口：in `in`（any）→ out `match`（object）
* 策略：**特权** —— 权限 `vision.analyze`

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `frame` | string | **是** | — | 捕获节点产生的 artefact id，或要搜索的图片路径。 |
| `template` | string | **是** | — | 要查找的模板 artefact id 或图片路径。 |
| `threshold` | number | 否 | `0.8` | 判定匹配的最小 ZNCC 分数，取值 `-1` 到 `1`。 |
| `output_var` | string | **是** | — | 接收 `found`、`score`、`x`、`y`、`width`、`height` 的变量。 |

```json
{ "id": "find_button", "type": "vision.TemplateMatch",
  "config": { "frame": "shot", "template": "C:/images/ok.png",
              "threshold": 0.85, "output_var": "hit" } }
```

### `vision.Ocr` — OCR

通过已配置的 OCR 后端从图像中提取文本。OCR 需要注入后端（`NODARA_OCR_COMMAND`）；没有后端时节点会
以明确的"无后端"错误失败，而不是猜测结果。

* 端口：in `in`（any）→ out `text`（string）
* 策略：**特权** —— 权限 `vision.analyze`

| 配置项 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `image` | string | **是** | — | 捕获节点产生的 artefact id，或要读取的图片路径。 |
| `language` | string | 否 | — | 后端支持时传入的 BCP-47 语言标签。示例：`en-US`、`zh-CN` |
| `output_var` | string | **是** | — | 接收识别文本的变量。 |

```json
{ "id": "read_text", "type": "vision.Ocr",
  "config": { "image": "shot", "language": "en-US", "output_var": "text" } }
```

## 增加节点类型

安装插件后，其节点类型会在下列操作之后出现在这里 —— 也出现在节点面板、Agent 能力列表与发布的 schema 中：

```bash
nodara-cli schema --plugin-dir path/to/plugins --out schema
```

实现方式见[编写节点（英文）](../Nodara-Core/docs/node-authoring.md)，
能产出良好提示的配置 schema 约定见
[Schema 详解](schema.zh.md#10-编写对用户友好的配置-schema)。
