# Documentation

> 中文版：[README.zh.md](README.zh.md)

Everything published, in one place.

## Language convention

Every guide in this directory exists twice:

| File | Language |
|---|---|
| `name.md` | English (default) |
| `name.zh.md` | Chinese |

The two files are kept in sync structurally: same sections, same anchors, same
examples. When a document changes, change both.

## Start here

| Document | Contents |
|---|---|
| [project.md](project.md) · [中文](project.zh.md) | Detailed project guide: architecture, execution model, plugins, runtime, Studio, agent, security, extension points |
| [user-manual.md](user-manual.md) · [中文](user-manual.zh.md) | Operating the packaged product: CLI, runtime, Studio, Agent, API, security and troubleshooting |
| [testing.md](testing.md) · [中文](testing.zh.md) | Debug/release acceptance checklist, integrity checks and regression procedure |
| [artifacts.md](artifacts.md) · [中文](artifacts.zh.md) | Artifact layout, checksums, debug/release differences and reproducible packaging |
| [schema.md](schema.md) · [中文](schema.zh.md) | Every JSON Schema document, field by field, and how the workflow schema produces editor content hints |
| [nodes.md](nodes.md) · [中文](nodes.zh.md) | Node catalogue: ports, permissions and every configuration key |

## Getting started

| Document | Contents |
|---|---|
| [../README.md](../README.md) | What the project is and how the pieces fit |
| [../QUICKSTART.md](../QUICKSTART.md) · [中文](../QUICKSTART.zh.md) | End-to-end walkthrough in five minutes |
| [../ARCHITECTURE.md](../ARCHITECTURE.md) | Repository layout and dependency rules |
| [../examples/README.md](../examples/README.md) | Runnable example workflows |

## Core

| Document | Contents |
|---|---|
| [../Nodara-Core/docs/architecture.md](../Nodara-Core/docs/architecture.md) | Crate detail, execution model, policy and audit |
| [../Nodara-Core/docs/node-authoring.md](../Nodara-Core/docs/node-authoring.md) | Writing a capability, in process or as a plugin |
| [../Nodara-Core/protocol/plugin-protocol.md](../Nodara-Core/protocol/plugin-protocol.md) | JSON-RPC over stdio, lifecycle, error codes |
| [../Nodara-Core/protocol/runtime-api.md](../Nodara-Core/protocol/runtime-api.md) | HTTP and WebSocket API, schema endpoints, diagnostic codes |
| [../Nodara-Core/schema](../Nodara-Core/schema) | Generated JSON Schema documents |
| [../Nodara-Core/plugins/README.md](../Nodara-Core/plugins/README.md) | Plugin layout and executable resolution |

## Clients

| Document | Contents |
|---|---|
| [../Nodara-Studio/README.md](../Nodara-Studio/README.md) | The visual editor |
| [../Nodara-Agent/README.md](../Nodara-Agent/README.md) | The planner and operator |

## Project

| Document | Contents |
|---|---|
| [../CHANGELOG.md](../CHANGELOG.md) | Version history |
| [../LICENSE](../LICENSE) | MIT licence |

## Regenerating the schemas

The JSON Schema documents are generated from the Rust types and the installed
node descriptors. After changing a type or a descriptor, regenerate and commit
them:

```bash
cd Nodara-Core
cargo run -p nodara-cli -- schema --out schema
```

`workflow.schema.json` is composed from the node catalogue; `--no-capabilities`
publishes the catalogue-free version and `--plugin-dir` includes plugins. CI
fails if the checked-in schemas are stale, so they cannot drift unnoticed.

The node reference is checked against the shipped catalogue by
`cargo test -p nodara-cli`, which keeps [nodes.md](nodes.md) and
[nodes.zh.md](nodes.zh.md) honest.
