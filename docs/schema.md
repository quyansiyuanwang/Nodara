# Schema guide

> 中文版：[schema.zh.md](schema.zh.md)

Everything that crosses a process boundary in Nodara-Core is a JSON
document with a published JSON Schema: workflows, plugin manifests, node
descriptors, execution events, agent sessions and tool calls. The schemas are
generated from the Rust types, so they cannot drift from the implementation, and
they are what makes the Studio, the agent, the CLI and third-party editors agree
on the same data.

This guide covers every document, field by field, and explains how the workflow
schema is composed so that `"$schema"` gives an editor completion for node types
and their configuration.

## 1. Where the schemas live

| Document | File | Served by the runtime | Generated from |
|---|---|---|---|
| Workflow | [`schema/workflow.schema.json`](../Nodara-Core/schema/workflow.schema.json) | `GET /api/v1/schema/workflow` | `Workflow` + installed node descriptors |
| Plugin manifest | [`schema/plugin-manifest.schema.json`](../Nodara-Core/schema/plugin-manifest.schema.json) | `GET /api/v1/schema/plugin-manifest` | `PluginManifest` |
| Node descriptor | [`schema/node-descriptor.schema.json`](../Nodara-Core/schema/node-descriptor.schema.json) | `GET /api/v1/schema/node-descriptor` | `NodeDescriptor` |
| Execution event | [`schema/execution-event.schema.json`](../Nodara-Core/schema/execution-event.schema.json) | `GET /api/v1/schema/execution-event` | `EventEnvelope` |
| Agent session | [`schema/agent-session.schema.json`](../Nodara-Core/schema/agent-session.schema.json) | `GET /api/v1/schema/agent-session` | `AgentSession` |
| Agent tool call | [`schema/agent-tool-call.schema.json`](../Nodara-Core/schema/agent-tool-call.schema.json) | `GET /api/v1/schema/agent-tool-call` | `ToolCall` |

`GET /api/v1` lists these names, so a client never has to hardcode them. Every
document is also served under its file name (`/schema/workflow.schema.json`).

## 2. The workflow document

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

### Root fields

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `$schema` | string | no | — | JSON Schema this document follows. Editors use it for completion and documentation; every Nodara tool preserves it on round-trip. |
| `schema_version` | string | no | `"2.0"` | Workflow format version. A different major is reported as `WF100` and can be upgraded with `nodara-cli migrate`. |
| `id` | string | **yes** | — | Stable workflow identifier, conventionally `workflow.<name>`. Must not be empty (`WF101`). |
| `metadata` | object | no | `{}` | Human-facing metadata, see below. |
| `nodes` | array | no | `[]` | The graph's nodes. |
| `edges` | array | no | `[]` | The graph's edges. |
| `variables` | object | no | `{}` | Workflow-scoped variables, keyed by name. |

### `metadata`

| Field | Type | Description |
|---|---|---|
| `name` | string | Display name. |
| `description` | string / null | Longer description. |
| `tags` | string[] | Free-form tags used for search. |
| `author` | string / null | Author or owning team. |
| `version` | string / null | Semantic version of the workflow itself (independent of `schema_version`). |
| `created_at`, `updated_at` | string / null | ISO-8601 timestamps. |
| `extensions` | object | Extension bag; unknown keys are preserved on round-trip. |

### `nodes[]`

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | **yes** | Unique within the workflow (`WF102` when empty, `WF103` when duplicated). Edges refer to it. |
| `type` | string | **yes** | Namespaced node type, e.g. `core.Log`, `windows.Input.Keyboard`. The composed schema publishes the enum of installed types (`WF104` when empty, `WF140` when unknown). |
| `label` | string | no | Display label shown on the canvas. |
| `config` | object | no | Node-type specific configuration, validated against that node's descriptor (`WF141`–`WF143`). Defaults to `{}`. |
| `position` | `{ "x": number, "y": number }` | no | Canvas position, for editor round-tripping; ignored by the runtime. |
| `enabled` | boolean | no | Defaults to `true`. A disabled node is skipped and acts as a transparent pass-through. |
| `condition` | string | no | Optional expression evaluated before the node. A false result skips the node and prunes its outgoing branches. |
| `delay_before_ms` | integer | no | Defaults to `0`. Wait before invoking the node; cancellation remains responsive. |
| `delay_after_ms` | integer | no | Defaults to `0`. Wait after successful execution before activating outgoing branches. |
| `continue_on_error` | boolean | no | Defaults to `false`. After retries are exhausted, activate outgoing branches instead of failing the run. Policy denials and validation errors still stop execution. |
| `timeout_ms` | integer | no | Optional maximum time for one plugin execution attempt. Core built-in nodes ignore process-level timeouts. |
| `retry` | integer | no | Defaults to `0`. Additional attempts after the first failed execution. |
| `retry_delay_ms` | integer | no | Defaults to `0`. Wait between failed attempts. |
| `result_var` | string | no | Common output mapping. Publishes the selected output port under this run-scope variable. |
| `result_port` | string | no | Output port captured by `result_var`; defaults to `out` or the first available output (`WF144` when unknown). |
| `metadata` | object | no | Extension bag preserved on round-trip. |

The configuration keys available for a `type` are documented per node in the
[node reference](nodes.md); the authoritative machine-readable source is
`GET /api/v1/node-types`.

### `edges[]`

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | **yes** | Unique within the workflow (`WF110` when empty, `WF111` when duplicated). |
| `source` | string | **yes** | Source node id (`WF112` when it does not exist). |
| `target` | string | **yes** | Target node id (`WF113` when it does not exist). |
| `source_port`, `target_port` | string / null | no | Named ports. When omitted, `out` → `in` is assumed. |
| `branch` | `always` / `success` / `failure` | no | Selects the source-node outcome that can activate the edge. Omitted or `always` follows both outcomes. A `failure` edge handles execution errors and activates the recovery path without requiring Continue on error. |
| `condition` | string / null | no | Guard expression. The edge is taken only when it evaluates truthy; an edge without a condition is always taken. Expressions may read run variables and `{{templates}}`. |
| `label` | string / null | no | Display label. |

A self-loop is a warning (`WF114`).

### `variables.<name>`

| Field | Type | Description |
|---|---|---|
| `value` | any | Default value used when the run does not override it. |
| `description` | string / null | Documentation. |
| `secret` | boolean | Marks a value that must never be written to logs or audit records. Defaults to `false`. |

### Validation diagnostics

`POST /api/v1/workflows/validate` (and `nodara-cli validate`) returns every problem
at once, each with a stable code, a severity, a JSON-pointer `path` and an
optional `hint`:

| Code | Severity | Meaning |
|---|---|---|
| `WF100` | error | Unsupported `schema_version` |
| `WF101` | error | Workflow `id` is empty |
| `WF102` | error | Node `id` is empty |
| `WF103` | error | Duplicate node `id` |
| `WF104` | error | Node `type` is empty |
| `WF110` | error | Edge `id` is empty |
| `WF111` | error | Duplicate edge `id` |
| `WF112` | error | Edge source does not exist |
| `WF113` | error | Edge target does not exist |
| `WF114` | warning | Edge is a self-loop |
| `WF120` | error | No `core.Start` node (when `require_start`) |
| `WF121` | error | More than one `core.Start` node |
| `WF122` | error | No `core.End` node (when `require_end`) |
| `WF130` | error | The graph contains a cycle (when `reject_cycles`) |
| `WF131` | warning | Node is unreachable from `core.Start` |
| `WF132` | warning | Node has no outgoing edge |
| `WF140` | error | Unknown node type (not installed) |
| `WF141` | error | `config` is not a JSON object |
| `WF142` | error | `config` is missing a required key |
| `WF143` | warning | `config` has a key the descriptor does not declare |
| `WF144` | error | `result_port` is not declared by the node descriptor |
| `WF145` | error | `result_var` is empty |
| `WF150` | warning | `{{template}}` references an undeclared variable |
| `WF151` | warning | Node `condition` references an undeclared variable |
| `WF152` | warning | Edge `condition` references an undeclared variable |

The strictness is configurable per call (`reject_cycles`, `require_start`,
`require_end`, `warn_unreachable`, `warn_dead_end`,
`check_variable_references`).

## 3. Node configuration and content hints

The workflow schema is the one schema that cannot come from the Rust types
alone: `schemars` knows the *shape* of a document, not which node types a
deployment installed. Publishing the bare schema would leave `type` an
unconstrained string and `config` an open object, so an editor following
`$schema` would have nothing to suggest.

`nodara_schema::workflow_schema_for(&[NodeDescriptor])` fixes that by folding the
installed catalogue into the document schema:

```text
Workflow
├── properties.nodes.items = Node
│   ├── properties.type   -> $ref #/definitions/NodeType        (enum of installed types)
│   └── allOf [ one branch per node type ]
│       ├── if   { "type": { "const": "core.Log" } }
│       └── then { "type": { "const", "description" },
│                  "config": { "$ref": "#/definitions/NodeConfig.core.Log" } }
└── definitions
    ├── NodeType                    the enum, so `type` values complete
    └── NodeConfig.<node type>      that node's config schema, with its
                                    titles, descriptions, defaults and enums
```

The shape is deliberate and measured against
`vscode-json-languageservice`, the engine VS Code uses:

* `allOf` + `if`/`then` completes both the node type values and the right
  configuration keys;
* an undiscriminated `config.oneOf` completes keys from the *wrong* node type;
* a single `oneOf` of whole node variants hides the current type from value
  completion.

Because the branches are keyed on `type` `const`, the same schema also
*validates* correctly: a `core.Log` node is checked against `core.Log`'s schema
and nothing else.

### Turning the hints on in an editor

Put the schema reference in the document, exactly like the original
single-process implementation did:

```json
{
  "$schema": "../Nodara-Core/schema/workflow.schema.json"
}
```

The repository's `examples/*.json` already do this. A running runtime serves the
same document composed for *its* plugins:

```json
{
  "$schema": "http://127.0.0.1:8710/api/v1/schema/workflow"
}
```

The Studio's *Workflow JSON* tab shows the active reference and can switch it to
the runtime URL in one click, and `nodara-agent plan --out` stamps the runtime URL
on generated documents. If a file carries no `$schema`, map it once in the
editor instead (VS Code example in
[QUICKSTART.md](../QUICKSTART.md#editor-content-hints)).

What an editor then provides:

| Interaction | Result |
|---|---|
| Completing a node `type` | Every installed node type, each with the descriptor's description |
| Completing a node `config` key | Only the keys that node declares, with documentation |
| Completing `enum` values | The accepted values (`debug`, `info`, …), plus the default inserted automatically |
| Hovering a property | The descriptor's `description` |
| Typing | Inline diagnostics for missing required keys, unknown keys, wrong types, out-of-range numbers |

## 4. Regenerating the schemas

```bash
cd Nodara-Core
cargo run -p nodara-cli -- schema --out schema                        # built-ins + official capabilities
cargo run -p nodara-cli -- schema --plugin-dir target/release/plugins # include third-party node types
cargo run -p nodara-cli -- schema --stdout --no-capabilities          # the catalogue-free schema
```

CI regenerates the files and fails if the tree changes, so a type or descriptor
change that is not published is caught before review. `workflow.schema.json` is
deterministic: it depends only on the *set* of descriptors, not their order.

## 5. Node descriptor

The descriptor is the machine-readable contract a capability publishes. It is
what the palette, the configuration form, the validation and the editor hints
are all built from.

| Field | Type | Description |
|---|---|---|
| `node_type` | string | Namespaced type, e.g. `windows.Input.Keyboard`. |
| `display_name` | string | Human-facing name shown in the palette. |
| `category` | string | Palette grouping, e.g. `Input`. |
| `description` | string | Short tooltip text; also documents the `type` value in editors. |
| `version` | string | Semantic version of this node type's contract. |
| `inputs`, `outputs` | `PortDescriptor[]` | Declared ports. |
| `config_schema` | object | JSON Schema for the node's `config` object. |
| `capabilities` | string[] | Capability identifiers the node uses, e.g. `Input.Keyboard`. |
| `permissions` | string[] | Permissions required, e.g. `input.control`. |
| `dangerous` | boolean | Whether the node performs a side effect policy may gate. |
| `allows_additional_config` | boolean | Whether unknown `config` keys are tolerated (defaults to `true`). |
| `plugin_id` | string / null | Contributing plugin, when the node comes from a plugin. |

### `PortDescriptor`

| Field | Type | Description |
|---|---|---|
| `name` | string | Stable port name used by edges (`out`, `in`, `artifact`, …). |
| `display_name` | string | Human-facing label. |
| `kind` | `input` / `output` | Direction. |
| `value_type` | `any`, `string`, `number`, `boolean`, `object`, `array`, `image`, `window`, `path` | Abstract type used for editor-time compatibility. |
| `required` | boolean | Whether an edge must be attached for execution to succeed. |
| `description` | string / null | Documentation. |
| `default` | any / null | Default value shown in the editor. |

### `config_schema` conventions

A descriptor's `config_schema` is a draft-07 JSON Schema for the `config`
object. The runtime enforces its `required` list and
`allows_additional_config`; the composed workflow schema and the Studio form use
the rest. For helpful hints, describe every property:

| Keyword | Effect |
|---|---|
| `title` | Form label and completion detail |
| `description` | Hover text, editor hint, form help |
| `default` | Seeds a new node and is inserted by completion |
| `enum` | Value completion and validation |
| `minimum` / `maximum` | Numeric bounds and input constraints |
| `examples` | Extra documentation |

## 6. Plugin manifest

`manifest.json` is the only file the runtime reads before launching a plugin, so
it is small and validated before any plugin code runs.

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | **yes** | Reverse-DNS unique id, e.g. `nodara.windows.input`. |
| `name` | string | **yes** | Human-facing name. |
| `version` | string | **yes** | Semantic version of the plugin. |
| `protocol_version` | string | **yes** | Wire protocol version the plugin speaks (currently `1`). |
| `executable` | string | **yes** | Executable path relative to the plugin directory. |
| `args` | string[] | no | Extra process arguments. |
| `capabilities` | string[] | no | Capabilities provided, e.g. `Input.Keyboard`. |
| `permissions` | string[] | no | Permissions required to function. |
| `node_types` | string[] | no | Node types provided. |
| `description`, `author`, `homepage` | string / null | Documentation and provenance. |
| `metadata` | object | no | Extension bag. |

## 7. Execution events

Every run publishes a monotonically sequenced stream. Events are the single
observability surface shared by the CLI, the Studio event viewer, the agent and
the audit log.

```json
{
  "run_id": "66aed51c-…",
  "seq": 7,
  "timestamp_ms": 1757226000000,
  "event": { "type": "node_finished", "node_id": "greet", "outputs": { "out": "hello" }, "duration_ms": 3 }
}
```

| Event `type` | Payload |
|---|---|
| `run_started` | `workflow_id` |
| `node_started` | `node_id`, `node_type` |
| `node_progress` | `node_id`, optional `progress`, optional `message` |
| `node_finished` | `node_id`, `outputs`, `duration_ms` |
| `node_failed` | `node_id`, `code`, `message`, `retryable` |
| `log` | `level` (`debug`/`info`/`warn`/`error`), `message`, optional `node_id` |
| `run_paused`, `run_resumed` | — |
| `run_cancelled` | optional `reason` |
| `run_completed` | `nodes_executed`, `duration_ms` |
| `run_failed` | `code`, `message` |
| `capability_decision` | `capability`, `decision` (`allow`/`deny`/`require_approval`), optional `node_id` |

`RunStatus` is one of `pending`, `running`, `paused`, `completed`, `failed`,
`cancelled`. The WS endpoint replays the buffered events and then streams live
ones; the runtime rewrites `seq` before fan-out so a reconnecting client always
sees a strictly increasing series.

## 8. Agent session and tool call

A session is the collaboration surface between an operator, the agent and the
Studio; they do not call each other directly.

| `AgentSession` field | Type | Description |
|---|---|---|
| `id` | string | Session id. |
| `goal` | string | What the operator asked for. |
| `provider` | string | Model provider used, for display. |
| `status` | `draft`, `planning`, `awaiting_approval`, `ready`, `running`, `completed`, `failed`, `cancelled` | Lifecycle state. |
| `created_at_ms`, `updated_at_ms` | integer | Unix epoch milliseconds. |
| `messages` | `SessionMessage[]` | Conversation, oldest first (`seq`, `at_ms`, `role`, `text`). |
| `plan` | `PlanPreview` / null | Latest proposed workflow plus its validation summary. |
| `approvals` | `ApprovalRequest[]` | Approvals raised during the session. |
| `run_id` | string / null | Run started from this session. |
| `tokens_used` | integer | Tokens the agent spent. |

An `ApprovalRequest` records `id`, `run_id`, `node_id`, `node_type`,
`capability`, `permissions`, `reason`, the `input` the node would receive, and
the `decision` once taken (`approved`/`denied`, with `decided_at_ms` and
`decided_by`).

A `ToolCall` is how a client expresses "I want this capability to run with this
input": `call_id`, `capability`, `node_type`, optional `run_id` and `reason`,
`requested_permissions` and `input`. It is a *request*, never an authorisation —
the runtime answers with a `ToolCallOutcome` of `completed`, `denied`,
`awaiting_approval` or `failed`.

## 9. Versions and migration

| Contract | Field | Current | Owner |
|---|---|---|---|
| Workflow document | `schema_version` | `2.0` | `nodara-schema` |
| Plugin / runtime wire | `protocol_version` | `1` | `nodara-schema`, `nodara-plugin` |
| Public HTTP API | `api_version` | `v1` | `nodara-runtime` |

The three versions are independent and never inferred from one another. Minor
changes are forward compatible: unknown fields survive a round trip, and unknown
node types are reported by capability-aware validation rather than by the
parser.

`nodara-cli migrate <file> [--out <file>] [--from 1] [--to 2]` upgrades legacy
documents: node kinds become namespaced types, `from`/`to` become
`source`/`target`, `seconds` becomes `duration_ms`, `text` becomes `message` —
and a `$schema` reference is carried over.

## 10. Writing configuration schemas that help users

```json
{
  "type": "object",
  "properties": {
    "path": {
      "type": "string",
      "title": "File",
      "description": "File to read. Supports `{{variable}}` interpolation.",
      "examples": ["C:/data/input.csv"]
    },
    "mode": {
      "type": "string",
      "title": "Mode",
      "description": "How the file is decoded.",
      "enum": ["utf8", "utf16", "binary"],
      "default": "utf8"
    },
    "retries": {
      "type": "integer",
      "title": "Retries",
      "minimum": 0,
      "default": 0
    }
  },
  "required": ["path"],
  "additionalProperties": false
}
```

Checklist:

- [ ] every property has a `title` and a `description`
- [ ] every property that can have a sensible default declares one
- [ ] closed value sets use `enum`, so editors complete and validate them
- [ ] numeric bounds use `minimum`/`maximum`
- [ ] `required` lists only what genuinely has no default
- [ ] `additionalProperties: false` unless the node intentionally accepts open configuration
- [ ] `allows_additional_config` agrees with `additionalProperties`
- [ ] `nodara-cli schema --out schema` was run, so the published schema and the hints include the new node

## 11. Related documents

| Topic | Document |
|---|---|
| Node catalogue with ports, permissions and configuration | [nodes.md](nodes.md) |
| Project overview and architecture | [project.md](project.md) |
| Runtime HTTP/WebSocket API | [Nodara-Core/protocol/runtime-api.md](../Nodara-Core/protocol/runtime-api.md) |
| Plugin wire protocol and error codes | [Nodara-Core/protocol/plugin-protocol.md](../Nodara-Core/protocol/plugin-protocol.md) |
| Writing a capability | [Nodara-Core/docs/node-authoring.md](../Nodara-Core/docs/node-authoring.md) |
