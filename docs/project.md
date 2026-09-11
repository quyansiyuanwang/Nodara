# Project guide

> 中文版：[project.zh.md](project.zh.md)

RecognizerFramework (RCR) is a workflow-automation framework for Windows
desktops and the ecosystem around it: a headless runtime, a visual editor, an
autonomous agent, and a plugin model that lets third parties add capabilities
without touching any of them.

A workflow is a document, not a program. It names capabilities (nodes), wires
them into a directed graph, and hands the graph to the runtime. The runtime
validates the graph, decides whether policy allows each side effect, executes
the nodes in dependency order, and publishes every step as an event.

## 1. What the project is for

| Problem | How RCR answers it |
|---|---|
| Automating a desktop task that is too irregular for a macro | Node graphs with conditions, variables and error branches |
| Teaching the automation to a non-programmer | The Studio renders a palette and configuration forms from the node descriptors, so no node type is hardcoded in the UI |
| Letting a model drive the desktop safely | The agent plans, but every capability call goes through the runtime's policy, approval and audit layer |
| Adding new capabilities without forking the core | Capabilities are plugins; the runtime discovers them, the editor and the agent pick them up automatically |
| Keeping clients and core in sync | One set of JSON Schema documents, generated from the Rust types and published by the runtime |

## 2. Repository layout

| Path | Role |
|---|---|
| `RecognizerFramework/` | Core: `rf-schema` contracts, `rf-core` engine and SDK, `rf-plugin` host, `rf-runtime` server, the official `rf-platform`/`rf-vision` capability sets, `rf-cli`, `rf-testkit` |
| `RecognizerFramework-Studio/` | Visual editor: Vite + TypeScript web app and a Tauri v2 desktop shell |
| `RecognizerFramework-Agent/` | Planner and operator: natural language to workflow, run supervision, explanation, audit |
| `examples/` | Runnable workflow documents (`hello-world`, `branching`, `delayed-log`, `window-find`, plus a legacy fixture) |
| `docs/` | This guide, the [schema guide](schema.md), the [node reference](nodes.md) and the index |

Everything else — plugins, additional workflows — is data. A deployment is the
runtime binary plus a directory of plugins plus whatever client you point at it.

## 3. Architecture

```text
        RecognizerFramework-Studio        RecognizerFramework-Agent
                    \                              /
                     \   HTTP + WebSocket + JSON  /
                      \                        /
                       v                      v
                  +--------------------------------+
                  |   RecognizerFramework Runtime  |   rf-runtime
                  +--------------------------------+
                              |
                  +-----------+-----------+
                  |                       |
            rf-core (engine)        rf-plugin (host)
                  |                       |
                  +----------+------------+
                             |
                        rf-schema  (contracts)
```

Three rules are enforced by the crate graph rather than by convention:

1. **The core is the only dependency direction.** The Studio and the agent are
   ordinary API clients; they link no core internals.
2. **The Studio and the agent do not know about each other.** If they need to
   cooperate they do it through runtime sessions, runs and events.
3. **Capabilities are plugins, not core modules.** `rf-core` contains no
   reference to `rf-platform` or `rf-vision`; the runtime discovers the official
   capability sets exactly as it discovers a third-party plugin.

The agent's isolation is the sharpest case: it depends on `rf-schema` and
nothing else from the core, so it physically cannot execute a capability without
the runtime seeing the call. "Permission does not depend on the prompt" is a
property of the dependency graph.

### Crates

| Crate | Responsibility | Depends on |
|---|---|---|
| `rf-schema` | Workflow, manifest, descriptor, event, session and tool-call contracts; validation; graph algorithms; migration; JSON Schema generation | `serde`, `schemars` |
| `rf-core` | `NodeExecutor` SDK, `CapabilityRegistry`, `WorkflowEngine`, run control, policy, audit, events, built-in nodes, expression evaluator | `rf-schema` |
| `rf-plugin` | JSON-RPC 2.0 over stdio, in-process transport, discovery, plugin host | `rf-schema`, `rf-core` |
| `rf-runtime` | Composition root: engine + plugins + policy + audit, run manager, agent sessions, HTTP/WebSocket API | `rf-schema`, `rf-core`, `rf-plugin` |
| `rf-platform` | Windows input, window management, screen capture, clipboard | `rf-core`, `rf-schema` |
| `rf-vision` | Template matching, pluggable OCR | `rf-core`, `rf-schema` |
| `rf-cli` | `validate`, `run`, `simulate`, `inspect`, `migrate`, `plugins`, `schema`, `serve` | all of the above |
| `rf-testkit` | Workflow builder, recording executors, in-process plugin harness | `rf-schema`, `rf-core`, `rf-plugin` |

## 4. Execution model

```text
RunManager.start
   |
   +--> WorkflowEngine.run          (one dedicated OS thread per run)
          |
          +--> validate             (capability-aware, never stops at the first problem)
          +--> topological order + branch activation
          +--> for each activated node:
          |      control.await_permission()   <- pause / step / cancel
          |      policy.decide()              -> event + audit record
          |      executor.execute()           -> local call or plugin round trip
          |      publish NodeFinished, activate guarded edges
          |
          +--> RunCompleted | RunFailed | RunCancelled
```

* **Nodes are synchronous.** `NodeExecutor::execute` returns a `Result`; a node
  may block on a Win32 call, a plugin round trip or a sleep without stalling an
  async runtime, because the run owns its own thread. No `async` colouring leaks
  into the SDK.
* **Pause is deterministic.** Control is checked at node boundaries, so a paused
  run is always suspended *between* nodes. Cancellation is stronger:
  `RunControl` is visible to executors and long-running nodes poll it, so cancel
  is prompt even mid-node.
* **Branching is data-driven.** An edge without a `condition` is always taken;
  an edge with one is taken when the expression evaluates truthy. Nodes that are
  never activated never execute.
* **Variables are the data channel.** Nodes publish values into the run scope
  (`SetVariable`, `output_var`, outputs) and templates read them with
  `{{name}}` interpolation; artifacts carry images and other large payloads by
  id.
* **Failures are structured.** A node failure carries a stable code and a
  message; the engine stops the run with `RunFailed` unless the failure is
  retryable-by-replanning, in which case an agent session may plan again.

## 5. Capability and plugin model

A capability is any type implementing `NodeExecutor`. It describes itself with a
[`NodeDescriptor`](schema.md#5-node-descriptor): node type, display name,
category, description, ports, a JSON Schema for its configuration, the
permissions it needs and whether it is dangerous.

The same implementation can run two ways:

* **in process** — register it in the runtime's `CapabilityRegistry`;
* **as a plugin** — serve it over stdio with `rf_plugin::serve_stdio` and drop a
  `manifest.json` next to the binary.

The runtime launches plugins lazily through JSON-RPC (`initialize`, `describe`,
`execute`, `cancel`, `health`, `shutdown`), and turns protocol failures into
ordinary node failures: a crash, a timeout or a disconnect fails that node, it
does not take the runtime down. Descriptor-only registration never overwrites a
node type that is already runnable, so a directory of half-installed plugins
cannot shadow a working capability.

Details: [plugin protocol](../RecognizerFramework/protocol/plugin-protocol.md),
[authoring a node](../RecognizerFramework/docs/node-authoring.md).

## 6. Runtime API

The runtime is the only process that executes anything. Both clients are thin:

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/v1` | Identity, version axes, published schema names |
| `GET` | `/api/v1/health` | Liveness and capability counts |
| `GET` | `/api/v1/plugins` | Installed plugins and load failures |
| `GET` | `/api/v1/node-types` | Descriptor for every node type |
| `GET` | `/api/v1/schema/{document}` | JSON Schema, composed for this deployment |
| `POST` | `/api/v1/workflows/validate` | Diagnostics for a document |
| `POST` | `/api/v1/runs` | Start a run (optionally bound to an agent session) |
| `GET` | `/api/v1/runs[/{id}]` | List or inspect runs |
| `POST` | `/api/v1/runs/{id}/{pause,resume,step,cancel}` | Steer a run |
| `WS` | `/api/v1/runs/{id}/events` | Replay and stream events |
| `GET` | `/api/v1/runs/{id}/event-log` | The same sequence over REST |
| `GET`/`POST` | `/api/v1/agent/sessions…` | Sessions, plans, messages, approvals |
| `GET` | `/api/v1/audit` | What the runtime allowed, refused and recorded |

Full reference: [runtime API](../RecognizerFramework/protocol/runtime-api.md).

## 7. Studio

The Studio is a descriptor-driven editor. On start-up it fetches
`/api/v1/node-types` and builds the palette, the configuration forms and the
capability badges from what it finds; it hardcodes only `core.Start` and
`core.End`, for the scaffold of a new document. Installing a plugin changes the
UI without rebuilding it.

It offers a canvas, a properties inspector generated from each descriptor's
configuration schema, a run viewer with the event stream and controls
(`pause`/`resume`/`step`/`cancel`), an agent panel with plan preview and
approval prompts, an audit view, and a raw workflow JSON view. The JSON view
keeps the document's `$schema` reference and can point it at the running
runtime, which is what turns on completion and documentation in any
JSON-Schema-aware editor.

## 8. Agent

The agent turns a goal into a workflow, then operates it — always through the
runtime API:

| Command | What it does |
|---|---|
| `capabilities` | Lists the node types the runtime offers, with their permissions |
| `plan "<goal>"` | Produces a workflow document (optionally `--from` an existing one to modify it) |
| `run "<goal>"` | Plans, starts the run, observes events and re-plans on failures it can fix |
| `explain` | Explains a workflow, a run, or a node failure in prose |
| `sessions` / `approve` | Shows sessions and pending approvals; grants or refuses them |
| `control <run> <action>` | Pauses, resumes, steps or cancels a run |
| `audit` | Reads the runtime's audit log |
| `replay <trace>` | Replays a recorded decision trace |

Guardrails: `--safe` refuses anything with a side effect, `--allow <node-type>`
restricts the agent to an explicit allowlist, and re-planning is selective —
only failures the model can author its way out of (`E_INVALID_CONFIG`,
`E_EXECUTION`) trigger another planning turn.

## 9. Security, approval and audit

Execution never depends on a client's good behaviour:

* Every descriptor declares `permissions` and `dangerous`; the runtime asks the
  policy for a decision before each call.
* Policies: `DefaultPolicy` (allow safe nodes, require approval for anything
  privileged), `AllowlistPolicy` (deny everything not listed, permission-keyed),
  `AllowAllPolicy`, and `PolicyChain` (deny wins, then approval, then allow).
* With approvals enabled the runtime raises a request against the owning agent
  session and **blocks the run thread** until an operator answers. A timeout
  denies; a run bound to no session is refused.
* Privileged permissions in the current official set: `input.control` (keyboard,
  mouse, text), `window.control` (focus), `screen.capture`, `clipboard`,
  `vision.analyze`.
* The audit log records every decision, node outcome and log record; `GET
  /api/v1/audit` and `rf-agent audit` read it.

## 10. Versioning and migration

| Contract | Field | Current |
|---|---|---|
| Workflow document | `schema_version` | `2.0` |
| Plugin / runtime wire | `protocol_version` | `1` |
| Public HTTP API | `api_version` | `v1` |

Major bumps break, minor changes are forward compatible: unknown fields survive
a round trip and unknown node types are reported by capability-aware validation
rather than by the parser. `rf-cli migrate` upgrades legacy documents
(namespaced node types, `from`/`to` to `source`/`target`, `seconds` to
`duration_ms`, `text` to `message`), and keeps a document's `$schema` reference.

## 11. Build, run, test

```bash
# core: validate, run, serve
cd RecognizerFramework
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p rf-cli -- validate ../examples/hello-world.json
cargo run -p rf-cli -- run ../examples/hello-world.json --in-process
cargo run -p rf-cli -- serve --in-process --port 8710

# editor
cd ../RecognizerFramework-Studio
npm ci && npm test && npm run build

# agent
cd ../RecognizerFramework-Agent
cargo test --workspace
RF_LLM_API_KEY=sk-... cargo run -p rf-agent -- plan "open Notepad and type hello"
```

## 12. Extension points

| I want to… | Do this |
|---|---|
| Add a capability | Implement `NodeExecutor`, register it in process or ship it as a plugin; see [node authoring](../RecognizerFramework/docs/node-authoring.md) |
| Make a capability's form and hints good | Describe every config property (title, description, default, enum, examples); see [schema guide](schema.md#10-writing-configuration-schemas-that-help-users) |
| Change the safety rules | Implement `CapabilityPolicy`, or configure `AllowlistPolicy` from the CLI/API |
| Replace the approval channel | Implement `rf_core::ApprovalHandler` |
| Add a client | Speak `api_version v1`; nothing else in the core needs to change |
| Add a transport for plugins | Implement the JSON-RPC transport in `rf-plugin`; the descriptor contract stays the same |

## 13. Where to read next

| Topic | Document |
|---|---|
| Every JSON Schema document, field by field | [schema.md](schema.md) |
| Node catalogue: ports, permissions, configuration | [nodes.md](nodes.md) |
| Repository layout and dependency rules | [ARCHITECTURE.md](../ARCHITECTURE.md) |
| Five-minute walkthrough | [QUICKSTART.md](../QUICKSTART.md) |
| Crate detail, policy path, execution model | [RecognizerFramework/docs/architecture.md](../RecognizerFramework/docs/architecture.md) |
| Plugin wire protocol | [RecognizerFramework/protocol/plugin-protocol.md](../RecognizerFramework/protocol/plugin-protocol.md) |
| Runtime API | [RecognizerFramework/protocol/runtime-api.md](../RecognizerFramework/protocol/runtime-api.md) |
| Version history | [CHANGELOG.md](../CHANGELOG.md) |
