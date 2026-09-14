# Schema 详解

> English: [schema.md](schema.md)

在 Nodara-Core 中，所有跨越进程边界的数据都是带公开 JSON Schema 的 JSON 文档：
工作流、插件 manifest、节点描述符、执行事件、Agent 会话与工具调用。这些 schema 由 Rust 类型生成，
因此不会与实现脱节；也正是它们让 Studio、Agent、CLI 与第三方编辑器对同一份数据达成一致。

本文逐个文档、逐个字段地说明这些 schema，并解释工作流 schema 是如何合成的 ——
从而让 `"$schema"` 能在编辑器里给出节点类型与配置的补全。

## 1. Schema 都在哪里

| 文档 | 文件 | 运行时接口 | 生成来源 |
|---|---|---|---|
| 工作流 | [`schema/workflow.schema.json`](../Nodara-Core/schema/workflow.schema.json) | `GET /api/v1/schema/workflow` | `Workflow` + 已安装节点描述符 |
| 插件 manifest | [`schema/plugin-manifest.schema.json`](../Nodara-Core/schema/plugin-manifest.schema.json) | `GET /api/v1/schema/plugin-manifest` | `PluginManifest` |
| 节点描述符 | [`schema/node-descriptor.schema.json`](../Nodara-Core/schema/node-descriptor.schema.json) | `GET /api/v1/schema/node-descriptor` | `NodeDescriptor` |
| 执行事件 | [`schema/execution-event.schema.json`](../Nodara-Core/schema/execution-event.schema.json) | `GET /api/v1/schema/execution-event` | `EventEnvelope` |
| Agent 会话 | [`schema/agent-session.schema.json`](../Nodara-Core/schema/agent-session.schema.json) | `GET /api/v1/schema/agent-session` | `AgentSession` |
| Agent 工具调用 | [`schema/agent-tool-call.schema.json`](../Nodara-Core/schema/agent-tool-call.schema.json) | `GET /api/v1/schema/agent-tool-call` | `ToolCall` |

`GET /api/v1` 会列出这些名字，客户端无需硬编码。每个文档也可以用文件名访问
（例如 `/schema/workflow.schema.json`）。

## 2. 工作流文档

```json
{
  "$schema": "../Nodara-Core/schema/workflow.schema.json",
  "schema_version": "2.0",
  "id": "workflow.hello-world",
  "metadata": { "name": "Hello World", "tags": ["getting-started"] },
  "nodes": [
    { "id": "start", "type": "core.Start", "label": "Start" },
    { "id": "greet", "type": "core.Log",
      "config": { "message": "Hello, {{name}}!", "level": "info" } },
    { "id": "end", "type": "core.End", "config": { "code": 0 } }
  ],
  "edges": [
    { "id": "e1", "source": "start", "target": "greet" },
    { "id": "e2", "source": "greet", "target": "end" }
  ],
  "variables": { "name": { "value": "World", "description": "Who to greet." } }
}
```

### 顶层字段

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `$schema` | string | 否 | — | 本文档遵循的 JSON Schema。编辑器据此提供补全与文档提示；Nodara 的所有工具都会在往返中保留它。 |
| `schema_version` | string | 否 | `"2.0"` | 工作流格式版本。主版本不匹配会报 `WF100`，可用 `nodara-cli migrate` 升级。 |
| `id` | string | **是** | — | 稳定的工作流标识，约定形如 `workflow.<name>`。不能为空（`WF101`）。 |
| `metadata` | object | 否 | `{}` | 面向人的元数据，见下。 |
| `nodes` | array | 否 | `[]` | 图上的节点。 |
| `edges` | array | 否 | `[]` | 图上的边。 |
| `variables` | object | 否 | `{}` | 工作流级变量，以名称为键。 |

### `metadata`

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | string | 显示名称。 |
| `description` | string / null | 较长描述。 |
| `tags` | string[] | 用于检索的自由标签。 |
| `author` | string / null | 作者或负责团队。 |
| `version` | string / null | 工作流自身的语义化版本（与 `schema_version` 无关）。 |
| `created_at`、`updated_at` | string / null | ISO-8601 时间戳。 |
| `extensions` | object | 扩展包；未知字段会在往返中保留。 |

### `nodes[]`

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `id` | string | **是** | 工作流内唯一（为空报 `WF102`，重复报 `WF103`）。边通过它引用节点。 |
| `type` | string | **是** | 带命名空间的节点类型，如 `core.Log`、`windows.Input.Keyboard`。合成后的 schema 会公布已安装类型的枚举（为空报 `WF104`，未知报 `WF140`）。 |
| `label` | string | 否 | 画布上显示的标签。 |
| `config` | object | 否 | 该节点类型的配置，按描述符校验（`WF141`–`WF143`）。默认 `{}`。 |
| `position` | `{ "x": number, "y": number }` | 否 | 画布坐标，供编辑器往返使用；运行时忽略。 |
| `enabled` | boolean | 否 | 默认 `true`。禁用节点会被跳过，并作为透明节点将入站分支透传。 |
| `condition` | string | 否 | 执行前计算的可选表达式；结果为假时跳过节点并剪除后续分支。 |
| `delay_before_ms` | integer | 否 | 默认 `0`。执行节点前等待，期间仍可响应取消。 |
| `delay_after_ms` | integer | 否 | 默认 `0`。节点成功后再等待指定时间，然后激活后续分支。 |
| `continue_on_error` | boolean | 否 | 默认 `false`。重试耗尽后仍激活后续分支，而不是终止运行；策略拒绝和校验错误仍会阻止执行。 |
| `retry` | integer | 否 | 默认 `0`。首次失败后的额外执行次数。 |
| `retry_delay_ms` | integer | 否 | 默认 `0`。失败尝试之间的等待时间。 |
| `metadata` | object | 否 | 往返保留的扩展包。 |

某个 `type` 可用的配置键在[节点参考](nodes.zh.md)中逐节点列出；机器可读的权威来源是
`GET /api/v1/node-types`。

### `edges[]`

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `id` | string | **是** | 工作流内唯一（为空报 `WF110`，重复报 `WF111`）。 |
| `source` | string | **是** | 源节点 id（不存在报 `WF112`）。 |
| `target` | string | **是** | 目标节点 id（不存在报 `WF113`）。 |
| `source_port`、`target_port` | string / null | 否 | 命名端口。省略时视为 `out` → `in`。 |
| `condition` | string / null | 否 | 守卫表达式；仅当表达式为真时走这条边。没有 `condition` 的边永远走。表达式可读取运行变量与 `{{模板}}`。 |
| `label` | string / null | 否 | 显示标签。 |

自环会产生告警 `WF114`。

### `variables.<名称>`

| 字段 | 类型 | 说明 |
|---|---|---|
| `value` | any | 运行未覆盖时使用的默认值。 |
| `description` | string / null | 文档说明。 |
| `secret` | boolean | 标记该值绝不能写入日志或审计记录。默认 `false`。 |

### 校验诊断

`POST /api/v1/workflows/validate`（以及 `nodara-cli validate`）会一次性返回全部问题，每条包含稳定的
错误码、严重级别、JSON 指针形式的 `path` 和可选的 `hint`：

| 错误码 | 级别 | 含义 |
|---|---|---|
| `WF100` | error | `schema_version` 不受支持 |
| `WF101` | error | 工作流 `id` 为空 |
| `WF102` | error | 节点 `id` 为空 |
| `WF103` | error | 节点 `id` 重复 |
| `WF104` | error | 节点 `type` 为空 |
| `WF110` | error | 边的 `id` 为空 |
| `WF111` | error | 边的 `id` 重复 |
| `WF112` | error | 边的源头节点不存在 |
| `WF113` | error | 边的目标节点不存在 |
| `WF114` | warning | 边是自环 |
| `WF120` | error | 没有 `core.Start` 节点（启用 `require_start` 时） |
| `WF121` | error | 有多个 `core.Start` 节点 |
| `WF122` | error | 没有 `core.End` 节点（启用 `require_end` 时） |
| `WF130` | error | 图中存在环（启用 `reject_cycles` 时） |
| `WF131` | warning | 节点从 `core.Start` 不可达 |
| `WF132` | warning | 节点没有出边 |
| `WF140` | error | 未知节点类型（未安装） |
| `WF141` | error | `config` 不是 JSON 对象 |
| `WF142` | error | `config` 缺少必填键 |
| `WF143` | warning | `config` 含有描述符未声明的键 |
| `WF150` | warning | `{{模板}}` 引用了未声明的变量 |
| `WF151` | warning | 节点 `condition` 引用了未声明的变量 |
| `WF152` | warning | 连线 `condition` 引用了未声明的变量 |

严格程度可在每次调用时配置（`reject_cycles`、`require_start`、`require_end`、
`warn_unreachable`、`warn_dead_end`、`check_variable_references`）。

## 3. 节点配置与内容提示

工作流 schema 是唯一无法只靠 Rust 类型得到的 schema：`schemars` 知道文档的**形状**，却不知道某个
部署装了哪些节点类型。若只发布裸 schema，`type` 就是无约束字符串，`config` 就是开放对象，编辑器
即便读了 `$schema` 也无从提示。

`nodara_schema::workflow_schema_for(&[NodeDescriptor])` 通过把已安装的节点目录合入文档 schema 来解决：

```text
Workflow
├── properties.nodes.items = Node
│   ├── properties.type   -> $ref #/definitions/NodeType        （已安装类型的枚举）
│   └── allOf [ 每个节点类型一个分支 ]
│       ├── if   { "type": { "const": "core.Log" } }
│       └── then { "type": { "const", "description" },
│                  "config": { "$ref": "#/definitions/NodeConfig.core.Log" } }
└── definitions
    ├── NodeType                    枚举，使 `type` 的值可补全
    └── NodeConfig.<节点类型>        该节点的配置 schema，含标题、描述、默认值与枚举
```

这个形状是刻意选择并实测过的（对照 VS Code 使用的
`vscode-json-languageservice`）：

* `allOf` + `if`/`then` 既能补全节点类型，也能补全正确的配置键；
* 不做判别的 `config.oneOf` 会补出**别的节点**的配置键；
* 用整体节点的 `oneOf` 则会让当前节点类型无法出现在值补全里。

由于分支以 `type` 的 `const` 作为判别条件，同一份 schema 也能正确**校验**：`core.Log` 节点只按
`core.Log` 的 schema 校验，不会误用其它节点的规则。

### 在编辑器里开启提示

在文档里写上 schema 引用即可（与原项目的做法完全一致）：

```json
{
  "$schema": "../Nodara-Core/schema/workflow.schema.json"
}
```

仓库中的 `examples/*.json` 已经这样做了。正在运行的运行时也会提供同一份文档，但按**它自己的**
插件集合合成：

```json
{
  "$schema": "http://127.0.0.1:8710/api/v1/schema/workflow"
}
```

Studio 的 *Workflow JSON* 页签会显示当前引用，并可一键切换到运行时地址；`nodara-agent plan --out`
也会把运行时地址写入生成的文档。如果某个文件没有 `$schema`，也可以在编辑器里做一次映射
（VS Code 示例见 [QUICKSTART.md](../QUICKSTART.md#editor-content-hints)）。

开启后编辑器会提供：

| 操作 | 效果 |
|---|---|
| 补全节点 `type` | 列出所有已安装节点类型，并显示描述符里的描述 |
| 补全节点 `config` 键 | 只列出该节点声明的键，并附文档说明 |
| 补全 `enum` 值 | 列出可接受取值（如 `debug`、`info` 等），并自动插入默认值 |
| 悬停属性 | 显示描述符里的 `description` |
| 输入时 | 内联提示缺少必填键、未知键、类型不符、数值越界等 |

## 4. 重新生成 schema

```bash
cd Nodara-Core
cargo run -p nodara-cli -- schema --out schema                        # 内置 + 官方能力
cargo run -p nodara-cli -- schema --plugin-dir target/release/plugins # 加入第三方节点类型
cargo run -p nodara-cli -- schema --stdout --no-capabilities          # 不含节点目录的裸 schema
```

CI 会重新生成并与仓库比对，一旦不一致就失败，因此"改了类型或描述符却没发布 schema"会在评审前被拦住。
`workflow.schema.json` 是确定性的：只取决于描述符**集合**，与顺序无关。

## 5. 节点描述符

描述符是能力对外公布的机器可读契约，节点面板、配置表单、校验与编辑器提示都由它驱动。

| 字段 | 类型 | 说明 |
|---|---|---|
| `node_type` | string | 带命名空间的类型，如 `windows.Input.Keyboard`。 |
| `display_name` | string | 面板中显示的友好名称。 |
| `category` | string | 面板分组，如 `Input`。 |
| `description` | string | 简短提示文本；同时作为编辑器里 `type` 值的文档。 |
| `version` | string | 该节点类型契约的语义化版本。 |
| `inputs`、`outputs` | `PortDescriptor[]` | 声明的端口。 |
| `config_schema` | object | 节点 `config` 对象的 JSON Schema。 |
| `capabilities` | string[] | 节点使用的能力标识，如 `Input.Keyboard`。 |
| `permissions` | string[] | 需要的权限，如 `input.control`。 |
| `dangerous` | boolean | 是否执行可能被策略拦截的副作用。 |
| `allows_additional_config` | boolean | 是否容忍未知 `config` 键（默认 `true`）。 |
| `plugin_id` | string / null | 来自插件时的提供方标识。 |

### `PortDescriptor`

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | string | 边使用的稳定端口名（`out`、`in`、`artifact` 等）。 |
| `display_name` | string | 友好名称。 |
| `kind` | `input` / `output` | 方向。 |
| `value_type` | `any`、`string`、`number`、`boolean`、`object`、`array`、`image`、`window`、`path` | 用于编辑器期兼容性判断的抽象类型。 |
| `required` | boolean | 是否必须连边才能执行成功。 |
| `description` | string / null | 文档说明。 |
| `default` | any / null | 编辑器中显示的默认值。 |

### `config_schema` 约定

描述符里的 `config_schema` 是描述 `config` 对象的 draft-07 JSON Schema。运行时执行它的 `required`
列表与 `allows_additional_config`；合成后的工作流 schema 与 Studio 表单使用其余部分。为了让提示好用，
请为每个属性写全信息：

| 关键字 | 效果 |
|---|---|
| `title` | 表单标签与补全详情 |
| `description` | 悬停文本、编辑器提示、表单帮助 |
| `default` | 新建节点时的初始值，补全时自动插入 |
| `enum` | 取值补全与校验 |
| `minimum` / `maximum` | 数值范围与输入约束 |
| `examples` | 额外文档说明 |

## 6. 插件 manifest

`manifest.json` 是运行时启动插件前唯一读取的文件，因此它很小，并且在**任何插件代码运行之前**就被校验。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `id` | string | **是** | 反域名风格唯一标识，如 `nodara.windows.input`。 |
| `name` | string | **是** | 友好名称。 |
| `version` | string | **是** | 插件语义化版本。 |
| `protocol_version` | string | **是** | 插件使用的线协议版本（当前为 `1`）。 |
| `executable` | string | **是** | 相对插件目录的可执行文件路径。 |
| `args` | string[] | 否 | 额外进程参数。 |
| `capabilities` | string[] | 否 | 提供的能力，如 `Input.Keyboard`。 |
| `permissions` | string[] | 否 | 运行所需权限。 |
| `node_types` | string[] | 否 | 提供的节点类型。 |
| `description`、`author`、`homepage` | string / null | 文档与来源信息。 |
| `metadata` | object | 否 | 扩展包。 |

## 7. 执行事件

每次运行都会发布单调递增序号的事件流。事件是 CLI、Studio 事件视图、Agent 与审计日志共用的唯一可观测面。

```json
{
  "run_id": "66aed51c-…",
  "seq": 7,
  "timestamp_ms": 1757226000000,
  "event": { "type": "node_finished", "node_id": "greet", "outputs": { "out": "hello" }, "duration_ms": 3 }
}
```

| 事件 `type` | 载荷 |
|---|---|
| `run_started` | `workflow_id` |
| `node_started` | `node_id`、`node_type` |
| `node_progress` | `node_id`、可选 `progress`、可选 `message` |
| `node_finished` | `node_id`、`outputs`、`duration_ms` |
| `node_failed` | `node_id`、`code`、`message`、`retryable` |
| `log` | `level`（`debug`/`info`/`warn`/`error`）、`message`、可选 `node_id` |
| `run_paused`、`run_resumed` | — |
| `run_cancelled` | 可选 `reason` |
| `run_completed` | `nodes_executed`、`duration_ms` |
| `run_failed` | `code`、`message` |
| `capability_decision` | `capability`、`decision`（`allow`/`deny`/`require_approval`）、可选 `node_id` |

`RunStatus` 取值为 `pending`、`running`、`paused`、`completed`、`failed`、`cancelled`。
WebSocket 接口会先回放已缓冲事件再推送实时事件；运行时在分发前重写 `seq`，因此重连的客户端总能
看到严格递增的序号。

## 8. Agent 会话与工具调用

会话是操作员、Agent 与 Studio 之间的协作面；它们彼此不直接调用。

| `AgentSession` 字段 | 类型 | 说明 |
|---|---|---|
| `id` | string | 会话 id。 |
| `goal` | string | 操作员提出的目标。 |
| `provider` | string | 使用的模型提供方，用于展示。 |
| `status` | `draft`、`planning`、`awaiting_approval`、`ready`、`running`、`completed`、`failed`、`cancelled` | 生命周期状态。 |
| `created_at_ms`、`updated_at_ms` | integer | Unix 毫秒时间戳。 |
| `messages` | `SessionMessage[]` | 对话，最早在前（`seq`、`at_ms`、`role`、`text`）。 |
| `plan` | `PlanPreview` / null | 最新计划的工作流及其校验摘要。 |
| `approvals` | `ApprovalRequest[]` | 会话期间产生的审批请求。 |
| `run_id` | string / null | 由该会话启动的运行。 |
| `tokens_used` | integer | Agent 消耗的 token 数。 |

`ApprovalRequest` 记录 `id`、`run_id`、`node_id`、`node_type`、`capability`、`permissions`、
`reason`、节点将收到的 `input`，以及决定后的 `decision`（`approved`/`denied`，附
`decided_at_ms` 与 `decided_by`）。

`ToolCall` 是客户端表达"我想用这个输入执行这个能力"的形式：`call_id`、`capability`、`node_type`、
可选 `run_id` 与 `reason`、`requested_permissions` 与 `input`。它是**请求**而不是授权 —— 运行时以
`ToolCallOutcome` 回答：`completed`、`denied`、`awaiting_approval` 或 `failed`。

## 9. 版本与迁移

| 契约 | 字段 | 当前值 | 归属 |
|---|---|---|---|
| 工作流文档 | `schema_version` | `2.0` | `nodara-schema` |
| 插件/运行时线协议 | `protocol_version` | `1` | `nodara-schema`、`nodara-plugin` |
| 公开 HTTP API | `api_version` | `v1` | `nodara-runtime` |

三个版本互相独立，绝不互相推导。次版本变化向前兼容：未知字段会在往返中保留，未知节点类型由
"感知能力的校验"报告，而不是解析器报错。

`nodara-cli migrate <file> [--out <file>] [--from 1] [--to 2]` 可升级旧文档：节点 kind 变为带命名空间的
类型，`from`/`to` 变为 `source`/`target`，`seconds` 变为 `duration_ms`，`text` 变为 `message` ——
并保留原有的 `$schema` 引用。

## 10. 编写对用户友好的配置 schema

```json
{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "title": "文件",
      "description": "要读取的文件，支持 `{{变量}}` 插值。",
      "examples": ["C:/data/input.csv"]
    },
    "mode": {
      "type": "string",
      "title": "模式",
      "description": "文件解码方式。",
      "enum": ["utf8", "utf16", "binary"],
      "default": "utf8"
    },
    "retries": {
      "type": "integer",
      "title": "重试次数",
      "minimum": 0,
      "default": 0
    }
  },
  "required": ["path"],
  "additionalProperties": false
}
```

检查清单：

- [ ] 每个属性都有 `title` 与 `description`
- [ ] 有合理默认值的属性都声明了 `default`
- [ ] 封闭取值集合用 `enum`，这样编辑器能补全也能校验
- [ ] 数值范围用 `minimum`/`maximum`
- [ ] `required` 只列真正没有默认值的项
- [ ] 除非节点有意接受开放配置，否则写 `additionalProperties: false`
- [ ] `allows_additional_config` 与 `additionalProperties` 保持一致
- [ ] 已执行 `nodara-cli schema --out schema`，让发布的 schema 与提示包含新节点

## 11. 相关文档

| 主题 | 文档 |
|---|---|
| 节点目录：端口、权限、配置 | [nodes.zh.md](nodes.zh.md) |
| 项目介绍与架构 | [project.zh.md](project.zh.md) |
| 运行时 HTTP/WebSocket API（英文） | [Nodara-Core/protocol/runtime-api.md](../Nodara-Core/protocol/runtime-api.md) |
| 插件线协议与错误码（英文） | [Nodara-Core/protocol/plugin-protocol.md](../Nodara-Core/protocol/plugin-protocol.md) |
| 编写能力（英文） | [Nodara-Core/docs/node-authoring.md](../Nodara-Core/docs/node-authoring.md) |
