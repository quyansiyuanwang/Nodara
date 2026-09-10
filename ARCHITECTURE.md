# Architecture

RCR is the integration workspace for three product repositories plus the
example workflows they share.

```text
RecognizerFramework/         core: schema, engine, plugin protocol, runtime, CLI
RecognizerFramework-Studio/  editor: Tauri shell + web front end
RecognizerFramework-Agent/   autonomy: planner and operator
examples/                    workflow documents in the current format
```

## Dependency direction

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

Three rules follow from the architecture document, and the crate graph enforces
them:

1. **The core is the only dependency direction.** The Studio and the agent are
   clients. They depend on the published contract — JSON Schema and the HTTP API
   — and not on any core Rust internals.
2. **The Studio and the agent do not know about each other.** If they ever need
   to cooperate, they do it through the runtime's runs and events.
3. **Capabilities are plugins, not core modules.** `rf-core` does not reference
   `rf-platform` or `rf-vision` at all. The runtime discovers them exactly as it
   would discover a third-party plugin.

The agent's isolation is the sharpest example: it links `rf-schema` and nothing
else, so it physically cannot execute a capability without the runtime's policy
layer seeing the call. "Permission must not depend on the prompt" is enforced by
the dependency graph rather than by convention.

## Contracts

Three independent versions, never inferred from one another:

| Contract | Field | Current | Owner |
|----------|-------|---------|-------|
| Workflow document | `schema_version` | `2.0` | `rf-schema` |
| Plugin / runtime wire | `protocol_version` | `1` | `rf-schema`, `rf-plugin` |
| Public HTTP API | `api_version` | `v1` | `rf-runtime` |

A breaking change bumps a major. A minor change is forward compatible: unknown
fields survive a round trip and unknown node types are reported by
capability-aware validation, never by the parser.

The JSON Schema documents under `RecognizerFramework/schema/` are generated from
the Rust types by `rf-cli schema`, so the published documents and the
implementation cannot drift.

## Where to read more

* [RecognizerFramework/docs/architecture.md](RecognizerFramework/docs/architecture.md)
  — crate-by-crate detail, the execution model and the policy path.
* [RecognizerFramework/protocol/](RecognizerFramework/protocol) — the plugin
  protocol and the runtime API, with error tables.
* [RecognizerFramework/docs/node-authoring.md](RecognizerFramework/docs/node-authoring.md)
  — how to add a capability.
* [RecognizerFramework-Studio/README.md](RecognizerFramework-Studio/README.md)
  and [RecognizerFramework-Agent/README.md](RecognizerFramework-Agent/README.md).
