# RecognizerFramework

A plugin-based desktop automation platform. Workflows are JSON graphs; every
capability — keyboard, windows, OCR, or a third party's plugin — is a separate
process described by a manifest. Editors and AI agents drive the same runtime
over the same API.

```text
RecognizerFramework/         core runtime, SDK, plugin protocol, CLI
RecognizerFramework-Studio/  visual workflow editor (Tauri + web)
RecognizerFramework-Agent/   natural-language planner and operator
examples/                    ready-to-run workflow documents
```

## What makes it different

**Editing and execution are separate.** The core has no UI. The Studio has no
execution logic. They meet at a versioned HTTP/WebSocket API.

**Capabilities are processes, not modules.** `rf-core` never references the
Windows or vision capabilities. The runtime discovers them from a directory of
manifests, exactly like a third-party plugin would be discovered. A plugin crash
cannot take the runtime down, and a plugin can be upgraded on its own.

**The UI adapts to installed plugins.** Node types are not hardcoded anywhere.
The Studio fetches node descriptors and their JSON Schemas at start-up and builds
the palette and the configuration forms from them. Installing a plugin changes
the editor without rebuilding it.

**Permission does not depend on a prompt.** The agent links only the published
contract crate, so it cannot execute anything without the runtime's policy layer
seeing the call. Every capability decision is written to an audit log.

## Quick start

Requires Rust 1.75+ (and Node 20+ for the Studio).

```bash
cd RecognizerFramework

# Validate, plan and run a workflow
cargo run -p rf-cli -- validate ../examples/hello-world.json
cargo run -p rf-cli -- simulate ../examples/hello-world.json
cargo run -p rf-cli -- run      ../examples/hello-world.json
```

Expected output ends with:

```text
run completed: 5 node(s) in 3ms
```

### Start the runtime

```bash
cargo run -p rf-cli -- serve --in-process --plugin-dir plugins
# runtime listening on http://127.0.0.1:8710/api/v1 (16 node type(s), 2 plugin(s))
```

`--in-process` registers the official capabilities directly; omit it to launch
`plugins/*/manifest.json` as real child processes instead.

### Open the editor

```bash
cd ../RecognizerFramework-Studio
npm install
npm run dev        # http://localhost:4173
```

### Plan with the agent

```bash
cd ../RecognizerFramework-Agent
RF_LLM_API_KEY=sk-... cargo run -p rf-agent -- plan "open Notepad and type a greeting"
```

Full walkthrough: [QUICKSTART.md](QUICKSTART.md).

## The three contracts

| Contract | Field | Current |
|----------|-------|---------|
| Workflow document | `schema_version` | `2.0` |
| Plugin / runtime wire | `protocol_version` | `1` |
| Public HTTP API | `api_version` | `v1` |

Workflow documents look like this ([full schema](RecognizerFramework/schema/workflow.schema.json)):

```json
{
  "$schema": "../RecognizerFramework/schema/workflow.schema.json",
  "schema_version": "2.0",
  "id": "workflow.hello-world",
  "metadata": { "name": "Hello World" },
  "nodes": [
    { "id": "start", "type": "core.Start" },
    { "id": "log", "type": "core.Log", "config": { "message": "Hello, {{name}}!" } },
    { "id": "end", "type": "core.End" }
  ],
  "edges": [
    { "id": "e1", "source": "start", "target": "log" },
    { "id": "e2", "source": "log", "target": "end" }
  ],
  "variables": { "name": { "value": "World" } }
}
```

Node types are namespaced: `core.*`, `system.*`, `windows.*`, `vision.*`,
`agent.*`, plus whatever a third-party plugin introduces.

The published schema is composed from the installed node descriptors, so
`"$schema"` gives an editor completion for node types and their configuration,
with descriptions, defaults and enums — regenerate it with
`rf-cli schema --out schema`, or ask a running runtime for
`GET /api/v1/schema/workflow`.

## Capabilities

| Plugin | Node types | Requires |
|--------|-----------|----------|
| built in | `core.Start`, `core.End`, `core.Log`, `core.Calculate`, `core.SetVariable`, `system.Delay` | — |
| `rf.windows.platform` | `windows.Input.Keyboard/Mouse/Text`, `windows.Window.Find/Focus/Capture`, `windows.Desktop.Capture`, `system.Clipboard` | `input.control`, `window.control`, `screen.capture`, `clipboard` |
| `rf.vision` | `vision.TemplateMatch`, `vision.Ocr` | `vision.analyze` |

Nodes that declare permissions are gated: policy is consulted before every
execution, and the decision appears in the event stream and the audit log.

## Documentation

| Document | Contents |
|----------|----------|
| [QUICKSTART.md](QUICKSTART.md) | End-to-end walkthrough in five minutes |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Repository layout and dependency rules |
| [RecognizerFramework/docs/architecture.md](RecognizerFramework/docs/architecture.md) | Crate detail, execution model, policy path |
| [RecognizerFramework/protocol/](RecognizerFramework/protocol) | Plugin protocol and runtime API |
| [RecognizerFramework/docs/node-authoring.md](RecognizerFramework/docs/node-authoring.md) | Writing a capability |
| [RecognizerFramework/plugins/README.md](RecognizerFramework/plugins/README.md) | Plugin layout and installation |
| [examples/README.md](examples/README.md) | The example workflows |
| [CHANGELOG.md](CHANGELOG.md) | Version history |

## Testing

```bash
cd RecognizerFramework && cargo test --workspace
cd ../RecognizerFramework-Agent && cargo test --workspace
cd ../RecognizerFramework-Studio && npm run build
```

## License

MIT — see [LICENSE](LICENSE).
