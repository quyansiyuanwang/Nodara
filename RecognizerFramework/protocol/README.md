# Protocol

RecognizerFramework versions three independent contracts. Keeping them separate
is what lets a node type ship without a transport change, and a transport change
without touching workflow documents.

| Contract | Field | Current | Defined by |
|----------|-------|---------|------------|
| Workflow document | `schema_version` | `2.0` | `rf-schema` |
| Plugin / runtime wire | `protocol_version` | `1` | `rf-schema`, `rf-plugin` |
| Public HTTP API | `api_version` | `v1` | `rf-runtime` |

Compatibility rules:

* a **major** difference is a breaking change and must be rejected;
* a **minor** difference is forward compatible — unknown fields are preserved on
  round-trip and unknown node types are reported by capability-aware validation,
  never by the parser;
* no Rust type from an internal crate is ever a cross-process protocol type.
  Everything on the wire is a Serde data structure documented by these files.

## Documents

* [`plugin-protocol.md`](plugin-protocol.md) — JSON-RPC 2.0 over stdio, the
  plugin lifecycle, and the error code table.
* [`runtime-api.md`](runtime-api.md) — the HTTP and WebSocket API the Studio, the
  CLI and the agent all use.
* [`../schema/`](../schema) — generated JSON Schema for the workflow document,
  the plugin manifest, node descriptors and execution events.

## Generating the schemas

The JSON Schema documents are generated from the Rust definitions so they can
never drift:

```bash
cd RecognizerFramework
cargo run -p rf-cli -- schema --out schema
```

To inspect one without writing files:

```bash
cargo run -p rf-cli -- schema --stdout
```
