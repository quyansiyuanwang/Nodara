# Plugin protocol

Status: version `1`. Transport: JSON-RPC 2.0 over a child process' stdio, one
JSON object per line.

## Why JSON-RPC

The architecture document requires that a plugin may be written in any language,
that a plugin crash must not take down the host, and that plugins can be upgraded
independently. A line-delimited JSON-RPC 2.0 conversation over stdio gives all
three without inventing a serialization format or a framing scheme.

`stdout` carries protocol frames **only**. Plugins log to `stderr`; the
`rf_plugin::tracing_init` helper installs a stderr-only subscriber for exactly
this reason.

## Lifecycle

```text
Runtime                                   Plugin process
  |--- initialize ------------------------>|   protocol + permission negotiation
  |<-- InitializeResult -------------------|
  |--- describe --------------------------->|   node types and config schemas
  |<-- DescribeResult ----------------------|
  |--- execute ---------------------------->|   run one node
  |<-- progress (0..n) ---------------------|   optional, asynchronous
  |<-- log (0..n) --------------------------|   optional, asynchronous
  |<-- ExecuteResult -----------------------|
  |--- cancel ----------------------------->|   best-effort interruption
  |--- health ----------------------------->|
  |<-- {"status":"ok"} ---------------------|
  |--- shutdown --------------------------->|   release resources and exit
```

Requests are dispatched on their own threads by the plugin server, which is what
makes `cancel` able to interleave with a long-running `execute`.

## Manifest

`manifest.json`, validated before the process is launched. The full schema is
[`plugin-manifest.schema.json`](../schema/plugin-manifest.schema.json).

```json
{
  "id": "rf.windows.platform",
  "name": "RecognizerFramework Windows Platform",
  "version": "2.0.0",
  "protocol_version": "1",
  "executable": "rf-platform-plugin.exe",
  "capabilities": ["Input.Keyboard"],
  "permissions": ["input.control"],
  "node_types": ["windows.Input.Keyboard"]
}
```

The manifest is read **before** any plugin code runs, so the runtime knows what
a plugin provides and what it will ask for. A plugin that declares a node type it
does not describe during `describe` is accepted but logged as drift; a plugin
whose manifest declares an incompatible `protocol_version` is rejected outright.

### Executable resolution

`executable` is resolved in this order:

1. `<plugin dir>/<executable>`;
2. `<dir of the running binary>/<executable>`;
3. `<dir of the running binary>/plugins/<plugin id>/<executable>`.

Rule 2 makes a `cargo build` tree directly runnable; rule 1 is the installed
layout.

## Methods

### `initialize`

Params:

```json
{
  "protocol_version": "1",
  "runtime_version": "2.0.0",
  "granted_permissions": ["input.control"]
}
```

Result:

```json
{
  "protocol_version": "1",
  "plugin": { "id": "rf.windows.platform", "name": "...", "version": "2.0.0" },
  "capabilities": ["Input.Keyboard", "Input.Mouse"]
}
```

The runtime rejects a plugin whose protocol **major** differs, and closes the
channel before returning an error.

### `describe`

Params: `{ "node_types": ["windows.Input.Keyboard"] }` — an empty list means
"everything".

Result: `{ "nodes": [NodeDescriptor, ...] }`. See
[`node-descriptor.schema.json`](../schema/node-descriptor.schema.json).

The descriptor is the whole reason the ecosystem works: the Studio builds its
palette and configuration forms from it, the agent selects capabilities from it,
and the runtime validates configuration against it. None of those consumers need
to know the plugin's implementation language.

### `execute`

Params:

```json
{
  "run_id": "0d0e...",
  "node_id": "keyboard",
  "node_type": "windows.Input.Keyboard",
  "config": { "keys": "ctrl+s" },
  "inputs": { "in": null },
  "variables": { "name": "World" },
  "timeout_ms": 30000
}
```

`config` is already interpolated by the runtime: a plugin never sees `{{name}}`.
`variables` is a read-only snapshot for context.

Result: `{ "outputs": { "out": "ctrl+s" }, "variables": {} }`.

### `cancel`

Params: `{ "run_id": "0d0e...", "node_id": "keyboard" }` (node optional).

The plugin flips the run's cancellation flag. Well-behaved plugins check it
between units of work; a plugin that ignores it is bounded by the runtime's
`timeout_ms`.

### `health` / `shutdown`

`health` returns `{ "status": "ok" }`. `shutdown` releases resources and ends the
read loop.

## Notifications

Plugin to runtime, never answered:

```json
{"jsonrpc":"2.0","method":"progress","params":{"run_id":"...","node_id":"ocr","progress":0.5,"message":"page 1 of 2"}}
{"jsonrpc":"2.0","method":"log","params":{"run_id":"...","node_id":"ocr","level":"info","message":"backend ready"}}
```

The runtime routes these into the run's event stream, so a Studio viewer or an
agent sees plugin progress exactly as it sees engine progress.

## Error codes

Standard JSON-RPC codes:

| Code | Meaning |
|------|---------|
| `-32700` | Parse error |
| `-32600` | Invalid request |
| `-32601` | Method not found |
| `-32602` | Invalid params |
| `-32603` | Internal error |

Application codes, mapped by the runtime onto `NodeError` variants:

| Code | `NodeError` | Meaning |
|------|-------------|---------|
| `-32001` | `InvalidConfig` | configuration rejected |
| `-32002` | `PermissionDenied` | capability refused |
| `-32003` | `Cancelled` | node was cancelled |
| `-32004` | `Timeout` | node exceeded its deadline |
| `-32005` | `Unsupported` | operation not implemented |
| `-32006` | `Execution` | generic failure |
| `-32007` | `Io` | I/O failure |

## Writing a plugin

A plugin is a `CapabilityRegistry` served over the protocol:

```rust
use std::sync::Arc;
use rf_core::CapabilityRegistry;
use rf_plugin::{serve_stdio, PluginServerInfo};

fn main() {
    rf_plugin::tracing_init();          // logs to stderr, never stdout

    let mut registry = CapabilityRegistry::new();
    registry.register(MyExecutor);

    let info = PluginServerInfo::new("com.example.myplugin", "My Plugin", "1.0.0")
        .with_capabilities(["My.Capability"]);
    serve_stdio(Arc::new(registry), info).expect("serve");
}
```

Implement `rf_core::NodeExecutor` for the capability itself. The same code can be
registered in-process by an embedded host, so a capability can start life as a
library and become a plugin without being rewritten — that is how
`rf-platform` and `rf-vision` are built.
