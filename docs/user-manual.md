# Nodara User Manual

> Chinese: [user-manual.zh.md](user-manual.zh.md)

This manual covers the prebuilt Nodara 2.0.0 Windows x64 package. For source
development and packaging, see `README.md`, `QUICKSTART.md`, and
`scripts/build-artifacts.ps1`.

## Components

| Component | Executable | Purpose |
|---|---|---|
| CLI | `nodara-cli.exe` | Validate, simulate, run, migrate, export schemas, and serve |
| Runtime | `nodara-runtime.exe` | HTTP/WebSocket execution and policy service |
| Platform plugin | `nodara-platform-plugin.exe` | Keyboard, mouse, windows, capture, clipboard |
| Vision plugin | `nodara-vision-plugin.exe` | Template matching and OCR |
| Agent | `nodara-agent.exe` | Natural-language planning and operation through the runtime |
| Studio | `nodara-studio.exe` | Visual editor, events, approvals, and audit |
| Studio Web | `studio-web\` | Browser build requiring an `/api` reverse proxy |

CLI, Studio, and Agent all reach capabilities through the runtime. They do not
execute plugin nodes themselves.

## First run

From the extracted package directory:

```powershell
.\nodara-cli.exe validate .\examples\hello-world.json
.\nodara-cli.exe simulate .\examples\hello-world.json
.\nodara-cli.exe run .\examples\hello-world.json
```

Start the runtime in another terminal:

```powershell
.\nodara-runtime.exe
```

It listens on `http://127.0.0.1:8710/api/v1` and discovers plugins under its
adjacent `plugins\` directory. Check it with:

```powershell
Invoke-RestMethod http://127.0.0.1:8710/api/v1/health
Invoke-RestMethod http://127.0.0.1:8710/api/v1/plugins
```

Start the desktop editor:

```powershell
.\nodara-studio.exe
```

The desktop build connects directly to `http://127.0.0.1:8710`. It first probes
the port: an existing runtime is reused, while a missing runtime is started from
the sibling `nodara-runtime.exe` (up to an eight-second readiness wait). Only a
runtime started by Studio is stopped when Studio exits. The editor reconnects
every five seconds if the service is unavailable.

Debug builds intentionally keep the Studio console window open for startup and
runtime logs. The automatically started runtime does not create a second
console window.

If the badge still says `runtime unreachable`, verify that
`nodara-runtime.exe` is beside Studio and that another program is not using port
8710. Set `NODARA_RUNTIME_BIN` to select a different runtime executable.

### Language

Use the `中文` / `English` button in the toolbar. Studio persists the choice and localizes the editor UI and known node/configuration labels.

### Canvas controls

| Action | Method |
|---|---|
| Add a node | **Click** it in the palette, or drag it to an exact canvas position; exactly one `core.Start` is allowed |
| Move a node | Drag the node body |
| Create a connection | Drag from an output port to an input port |
| Delete a node or connection | Select it and press `Delete`, or right-click it and choose delete |
| Edit a connection | Select it and edit its label or guard condition in Properties |
| Automatic validation | Run after every edit; the status beside `Validate` shows the result and errors disable `Run` |
| Resize the layout | Drag the dividers beside the left/right panels or above the bottom drawer; double-click to reset |

### Workflow settings and variables

When no node or connection is selected, Properties edits the workflow ID, name, description and tags. It also lists workflow variables and provides controls to add, edit or delete them, including descriptions and secret flags.

### Common node execution settings

Every node has an **Execution** section in Properties. These settings are part
of the workflow document and are honored by the runtime:

| Setting | Purpose |
|---|---|
| Enabled | Disable a node to skip it while passing its incoming branch through to its outgoing edges |
| Delay before (ms) | Wait before executing the node |
| Delay after (ms) | Wait after successful execution before activating outgoing branches |
| Continue on error | After retries are exhausted, continue through outgoing branches instead of failing the run |
| Retries | Number of additional attempts after a failed execution |
| Retry delay (ms) | Wait between failed attempts |

The node context menu also provides an immediate enable/disable action.

## CLI

| Command | Purpose | Executes nodes |
|---|---|---|
| `nodara-cli validate FILE` | Validate structure, graph, node types, and configuration | No |
| `nodara-cli simulate FILE` | Show order and required permissions | No |
| `nodara-cli run FILE` | Execute a workflow | Yes |
| `nodara-cli inspect FILE` | Print workflow structure | No |
| `nodara-cli migrate FILE --out NEW` | Upgrade a legacy document | No |
| `nodara-cli plugins` | List discovered plugin manifests | No |
| `nodara-cli schema --out DIR` | Export JSON Schema documents | No |
| `nodara-cli serve` | Start the full runtime service | Service |

Useful options include `--var key=value`, `--plugin-dir DIR`, `--allow
CAPABILITY`, `--allow-all`, `--audit FILE`, `--quiet`, and `--json`.

## Runtime configuration

The standalone runtime is configured through environment variables:

| Variable | Default | Purpose |
|---|---|---|
| `NODARA_RUNTIME_PORT` | `8710` | Listening port |
| `NODARA_PLUGIN_DIRS` | Empty | Additional plugin directories separated by `;` |
| `RUST_LOG` | Internal default | Rust tracing filter |

For `--require-approval`, audit files, and explicit policy allowlists, use the
source-built CLI:

```powershell
cargo run -p nodara-cli -- serve --plugin-dir plugins --require-approval --audit audit.jsonl
```

## Workflows

Workflows use `schema_version: "2.0"`, namespaced node types, and a directed
acyclic graph. Variables are referenced with `{{name}}`. The complete node and
permission reference is in [nodes.md](nodes.md). A running runtime publishes the
deployment-specific schema at:

```text
http://127.0.0.1:8710/api/v1/schema/workflow
```

## Agent

With the runtime running:

```powershell
.\nodara-agent.exe capabilities
.\nodara-agent.exe sessions
.\nodara-agent.exe audit
```

Set `NODARA_LLM_API_KEY`, `NODARA_LLM_ENDPOINT`, and `NODARA_LLM_MODEL` to use an
OpenAI-compatible provider. For deterministic tests without a provider, the
hidden `--mock '<workflow-json>'` option supplies a canned response.

`--safe` rejects privileged or side-effecting nodes. `--allow NODE_TYPE`
restricts planning to an explicit allowlist. Runtime policy remains the
authoritative execution guard.

## HTTP API

The public base path is `/api/v1`. Important endpoints include health, plugins,
node types, workflow schema, workflow validation, runs, event logs, agent
sessions, approvals, and audit. See
[Nodara-Core/protocol/runtime-api.md](../Nodara-Core/protocol/runtime-api.md).
Run control and approval are also available through `nodara-agent`:

```powershell
.\nodara-agent.exe control <run-id> pause
.\nodara-agent.exe control <run-id> resume
.\nodara-agent.exe control <run-id> cancel
.\nodara-agent.exe approve <session-id> <approval-id> --by tester
```

## Security

The packaged runtime uses `DefaultPolicy` and auto-approves privileged nodes.
Only run workflows you trust after reviewing `nodara-cli simulate` output.
Prefer `nodara-agent --safe` or `--allow`, and use `--require-approval` when
testing with a source-built runtime. Audit records are durable only when
`--audit FILE` is provided.

## Troubleshooting

| Symptom | Action |
|---|---|
| Runtime exits at startup | Check for a port conflict, especially 8710 |
| Studio says `runtime unreachable` | Verify `nodara-runtime.exe` is beside Studio, then check port 8710, firewall, and the debug console |
| Only 14 nodes | Verify both plugin executables are present next to their manifests |
| Unknown node type | Start the runtime with the plugin directory that defines it |
| Missing webview or application startup failure | Install WebView2 and VC++ 2015-2022 x64 Runtime |
| Agent reports no API key | Set `NODARA_LLM_API_KEY` or use `--mock` |
| Run waits for approval | Use `nodara-agent sessions`, then approve or deny it |