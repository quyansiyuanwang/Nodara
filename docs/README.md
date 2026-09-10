# Documentation index

Everything published, in one place.

## Getting started

| Document | Contents |
|----------|----------|
| [../README.md](../README.md) | What the project is and how the pieces fit |
| [../QUICKSTART.md](../QUICKSTART.md) | End-to-end walkthrough in five minutes |
| [../ARCHITECTURE.md](../ARCHITECTURE.md) | Repository layout and dependency rules |
| [../examples/README.md](../examples/README.md) | Runnable example workflows |

## Core

| Document | Contents |
|----------|----------|
| [../RecognizerFramework/docs/architecture.md](../RecognizerFramework/docs/architecture.md) | Crate detail, execution model, policy and audit |
| [../RecognizerFramework/docs/node-authoring.md](../RecognizerFramework/docs/node-authoring.md) | Writing a capability, in process or as a plugin |
| [../RecognizerFramework/protocol/plugin-protocol.md](../RecognizerFramework/protocol/plugin-protocol.md) | JSON-RPC over stdio, lifecycle, error codes |
| [../RecognizerFramework/protocol/runtime-api.md](../RecognizerFramework/protocol/runtime-api.md) | HTTP and WebSocket API, diagnostic codes |
| [../RecognizerFramework/schema](../RecognizerFramework/schema) | Generated JSON Schema documents |
| [../RecognizerFramework/plugins/README.md](../RecognizerFramework/plugins/README.md) | Plugin layout and executable resolution |

## Clients

| Document | Contents |
|----------|----------|
| [../RecognizerFramework-Studio/README.md](../RecognizerFramework-Studio/README.md) | The visual editor |
| [../RecognizerFramework-Agent/README.md](../RecognizerFramework-Agent/README.md) | The planner and operator |

## Project

| Document | Contents |
|----------|----------|
| [../CHANGELOG.md](../CHANGELOG.md) | Version history |
| [../LICENSE](../LICENSE) | MIT licence |

## Regenerating the schemas

The JSON Schema documents are generated from the Rust types. After changing a
type in `rf-schema`, regenerate and commit them:

```bash
cd RecognizerFramework
cargo run -p rf-cli -- schema --out schema
```

CI fails if the checked-in schemas are stale, so they cannot drift unnoticed.
