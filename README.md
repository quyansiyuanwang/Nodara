# Nodara

> 中文: [README.zh.md](README.zh.md)


A plugin-based desktop automation platform. Workflows are JSON graphs; every
capability — keyboard, windows, OCR, or a third party's plugin — is a separate
process described by a manifest. Editors and AI agents drive the same runtime
over the same API.

```text
Nodara-Core/         core runtime, SDK, plugin protocol, CLI
Nodara-Studio/  visual workflow editor (Tauri + web)
Nodara-Agent/   natural-language planner and operator
examples/                    ready-to-run workflow documents
```

## What makes it different

**Editing and execution are separate.** The core has no UI. The Studio has no
execution logic. They meet at a versioned HTTP/WebSocket API.

**Capabilities are processes, not modules.** `nodara-core` never references the
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
cd Nodara-Core

# Validate, plan and run a workflow
cargo run -p nodara-cli -- validate ../examples/hello-world.json
cargo run -p nodara-cli -- simulate ../examples/hello-world.json
cargo run -p nodara-cli -- run      ../examples/hello-world.json
```

Expected output ends with:

```text
run completed: 5 node(s) in 3ms
```

### Start the runtime

```bash
cargo run -p nodara-cli -- serve --in-process --plugin-dir plugins
# runtime listening on http://127.0.0.1:8710/api/v1 (19 node type(s), 2 plugin(s))
```

`--in-process` registers the official capabilities directly; omit it to launch
`plugins/*/manifest.json` as real child processes instead.

### Open the editor

```bash
cd ../Nodara-Studio
npm install
npm run dev        # http://localhost:4173
```

### Plan with the agent

```bash
cd ../Nodara-Agent
NODARA_LLM_API_KEY=sk-... cargo run -p nodara-agent -- plan "open Notepad and type a greeting"
```

The desktop Studio starts its bundled runtime automatically. In the editor,
use the **Runs** tab to reopen any previous execution and its event stream.

Full walkthrough: [QUICKSTART.md](QUICKSTART.md).

## The three contracts

| Contract | Field | Current |
|----------|-------|---------|
| Workflow document | `schema_version` | `2.1` |
| Plugin / runtime wire | `protocol_version` | `1` |
| Public HTTP API | `api_version` | `v1` |

Workflow documents look like this ([full schema](Nodara-Core/schema/workflow.schema.json)):

```json
{
  "$schema": "../Nodara-Core/schema/workflow.schema.json",
  "schema_version": "2.1",
  "id": "workflow.hello-world",
  "metadata": { "name": "Hello World" },
  "nodes": [
    { "id": "start", "type": "core.Start" },
    { "id": "log", "type": "core.Log", "config": { "message": "Hello, {{name}}!" } },
    { "id": "end", "type": "core.End" }
  ],
  "edges": [
    { "id": "e1", "kind": "control", "source": "start", "target": "log" },
    { "id": "e2", "kind": "control", "source": "log", "target": "end" }
  ],
  "variables": { "name": { "value": "World" } }
}
```

Node types are namespaced: `core.*`, `system.*`, `windows.*`, `vision.*`,
`agent.*`, plus whatever a third-party plugin introduces.

The published schema is composed from the installed node descriptors, so
`"$schema"` gives an editor completion for node types and their configuration,
with descriptions, defaults and enums — regenerate it with
`nodara-cli schema --out schema`, or ask a running runtime for
`GET /api/v1/schema/workflow`.

## Capabilities

| Plugin | Node types | Requires |
|--------|-----------|----------|
| built in | `core.Start`, `core.End`, `core.Log`, `core.Calculate`, `core.SetVariable`, `system.Delay` | — |
| `nodara.windows.platform` | `windows.Input.Keyboard/Mouse/Text`, `windows.Window.Find/Focus/Capture`, `windows.Desktop.Capture`, `system.Clipboard`, `system.Command` | `input.control`, `window.control`, `screen.capture`, `clipboard`, `process.execute` |
| `nodara.vision` | `vision.TemplateMatch`, `vision.Ocr` | `vision.analyze` |

Nodes that declare permissions are gated: policy is consulted before every
execution, and the decision appears in the event stream and the audit log.

## Documentation

| Document | Contents |
|----------|----------|
| [docs/README.md](docs/README.md) · [中文](docs/README.zh.md) | Documentation index and the `name.md` / `name.zh.md` language convention |
| [docs/user-manual.md](docs/user-manual.md) · [中文](docs/user-manual.zh.md) | Packaged-product operations, API usage, security and troubleshooting |
| [docs/testing.md](docs/testing.md) · [中文](docs/testing.zh.md) | Debug/release acceptance checks and regression procedure |
| [docs/artifacts.md](docs/artifacts.md) · [中文](docs/artifacts.zh.md) | Artifact layout, checksums and reproducible packaging |
| [docs/project.md](docs/project.md) · [中文](docs/project.zh.md) | Detailed project guide: architecture, execution model, plugins, security, extension points |
| [docs/schema.md](docs/schema.md) · [中文](docs/schema.zh.md) | Every JSON Schema document field by field, and how `$schema` produces editor content hints |
| [docs/nodes.md](docs/nodes.md) · [中文](docs/nodes.zh.md) | Node catalogue: ports, permissions and every configuration key |
| [QUICKSTART.md](QUICKSTART.md) · [中文](QUICKSTART.zh.md) | End-to-end walkthrough in five minutes |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Repository layout and dependency rules |
| [Nodara-Core/docs/architecture.md](Nodara-Core/docs/architecture.md) | Crate detail, execution model, policy path |
| [Nodara-Core/protocol/](Nodara-Core/protocol) | Plugin protocol and runtime API |
| [Nodara-Core/docs/node-authoring.md](Nodara-Core/docs/node-authoring.md) | Writing a capability |
| [Nodara-Core/plugins/README.md](Nodara-Core/plugins/README.md) | Plugin layout and installation |
| [examples/README.md](examples/README.md) | The example workflows |
| [CHANGELOG.md](CHANGELOG.md) | Version history |

## Build and test

The complete test suite:

```powershell
cd Nodara-Core; cargo test --workspace
cd ../Nodara-Agent; cargo test --workspace
cd ../Nodara-Studio; npm test; npm run build
```

Build both runnable Windows x64 flavors, including NSIS/MSI and SHA-256 files:

```powershell
.\scripts\build-artifacts.ps1 -Configuration All
```

Results are written to the ignored `artifacts\debug\` and `artifacts\release\`
directories. See [docs/artifacts.md](docs/artifacts.md) for the layout.

## License

MIT — see [LICENSE](LICENSE).
