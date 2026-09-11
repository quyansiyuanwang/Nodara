# Architecture

Nodara is the integration workspace for three product repositories plus the
example workflows they share.

```text
Nodara-Core/         core: schema, engine, plugin protocol, runtime, CLI
Nodara-Studio/  editor: Tauri shell + web front end
Nodara-Agent/   autonomy: planner and operator
examples/                    workflow documents in the current format
```

## Dependency direction

```text
        Nodara-Studio        Nodara-Agent
                    \                              /
                     \   HTTP + WebSocket + JSON  /
                      \                        /
                       v                      v
                  +--------------------------------+
                  |   Nodara-Core Runtime  |   nodara-runtime
                  +--------------------------------+
                              |
                  +-----------+-----------+
                  |                       |
            nodara-core (engine)        nodara-plugin (host)
                  |                       |
                  +----------+------------+
                             |
                        nodara-schema  (contracts)
```

Three rules follow from the architecture document, and the crate graph enforces
them:

1. **The core is the only dependency direction.** The Studio and the agent are
   clients. They depend on the published contract — JSON Schema and the HTTP API
   — and not on any core Rust internals.
2. **The Studio and the agent do not know about each other.** If they ever need
   to cooperate, they do it through the runtime's runs and events.
3. **Capabilities are plugins, not core modules.** `nodara-core` does not reference
   `nodara-platform` or `nodara-vision` at all. The runtime discovers them exactly as it
   would discover a third-party plugin.

The agent's isolation is the sharpest example: it links `nodara-schema` and nothing
else, so it physically cannot execute a capability without the runtime's policy
layer seeing the call. "Permission must not depend on the prompt" is enforced by
the dependency graph rather than by convention.

## Contracts

Three independent versions, never inferred from one another:

| Contract | Field | Current | Owner |
|----------|-------|---------|-------|
| Workflow document | `schema_version` | `2.0` | `nodara-schema` |
| Plugin / runtime wire | `protocol_version` | `1` | `nodara-schema`, `nodara-plugin` |
| Public HTTP API | `api_version` | `v1` | `nodara-runtime` |

A breaking change bumps a major. A minor change is forward compatible: unknown
fields survive a round trip and unknown node types are reported by
capability-aware validation, never by the parser.

The JSON Schema documents under `Nodara-Core/schema/` are generated from
the Rust types by `nodara-cli schema`, so the published documents and the
implementation cannot drift.

## Where to read more

* [Nodara-Core/docs/architecture.md](Nodara-Core/docs/architecture.md)
  — crate-by-crate detail, the execution model and the policy path.
* [Nodara-Core/protocol/](Nodara-Core/protocol) — the plugin
  protocol and the runtime API, with error tables.
* [Nodara-Core/docs/node-authoring.md](Nodara-Core/docs/node-authoring.md)
  — how to add a capability.
* [Nodara-Studio/README.md](Nodara-Studio/README.md)
  and [Nodara-Agent/README.md](Nodara-Agent/README.md).
