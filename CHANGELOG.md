# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.0.0] — plugin-ecosystem rearchitecture

The repository was rebuilt around the contracts described in `.tmp/PLAN.md`:
three independent product repositories joined only by JSON Schema and the
runtime API.

### Added

**Core (`RecognizerFramework/`)**

- `rf-schema` — workflow (`schema_version` 2.0), plugin manifest, node descriptor
  and execution event contracts, all deriving `JsonSchema` so the published
  schemas are generated rather than hand-maintained.
- Structured validation that never stops at the first problem: stable diagnostic
  codes (`WF1xx`), severities, JSON-pointer paths and repair hints, plus
  capability-aware checks against the live registry.
- Graph algorithms (topological order, cycle detection, reachability) shared by
  the editor, the CLI and the agent.
- `rf-cli migrate`, which upgrades legacy documents: namespaced node types,
  `from`/`to` to `source`/`target`, and configuration keys that changed meaning
  (`seconds` to `duration_ms`).
- `rf-core` — the `NodeExecutor` SDK, `CapabilityRegistry`, `WorkflowEngine`,
  deterministic pause/resume/step/cancel, policy decisions and an audit log.
- A dependency-free expression evaluator with a documented grammar.
- `rf-plugin` — JSON-RPC 2.0 over stdio, manifest discovery, a plugin host, an
  in-process transport for tests, and `serve_stdio` for writing plugins.
- `rf-runtime` — the headless runtime with the HTTP/WebSocket API, a run manager
  and a policy layer.
- `rf-platform` and `rf-vision` — real Windows capabilities (input, windows,
  capture, clipboard) and vision (template matching, pluggable OCR), each built
  as both a library and a plugin binary.
- `rf-cli` — `validate`, `run`, `simulate`, `inspect`, `migrate`, `plugins`,
  `schema` and `serve`.
- `rf-testkit` — a workflow builder, recording and failing executors, and an
  in-process plugin harness.

**Studio (`RecognizerFramework-Studio/`)**

- A typed web client with no hardcoded node types: the palette and every
  configuration form are built from the runtime's descriptors.
- An SVG graph editor with drag, connect, select, delete, schema-driven
  properties and a live event viewer.
- A Tauri v2 shell around the same bundle.

**Agent (`RecognizerFramework-Agent/`)**

- A provider-neutral planner with a draft to validate to repair loop that feeds
  the runtime's own diagnostics back to the model.
- Guardrails, budgets, a JSON Lines decision trace and replay.

### Changed

- The workspace layout is now three sibling repositories instead of one crate
  tree with an embedded editor and an embedded agent.
- `rf-platform` and `rf-vision` are no longer core modules; the core does not
  reference them.
- Workflow documents use `schema_version` and namespaced node types
  (`core.Start`, `windows.Input.Keyboard`, `vision.Ocr`).
- Example workflows were rewritten in the current format; the legacy shape is
  kept as a migration fixture under `examples/legacy/`.

### Removed

- The `rf-agent` crate from the core workspace (superseded by
  `RecognizerFramework-Agent`).
- The old `studio/` prototype from the core repository (superseded by
  `RecognizerFramework-Studio`).
- The `meval` dependency: expression evaluation is now in-crate, which also
  removes an unmaintained transitive dependency.

### Security

- Agent capability calls are authorised by the runtime, not by a prompt. The
  agent links only the contract crate, so it has no route to execution that
  bypasses policy.
- Every capability evaluation, approval and denial is written to the audit log.

## [1.0.0]

Initial Rust rewrite of the Python RecognizerFramework: a workflow schema, a
graph-based executor, Windows platform adapters, vision helpers and a CLI.
