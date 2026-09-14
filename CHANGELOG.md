# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — schema-driven content hints

### Added

- `nodara_schema::workflow_schema_for` composes the workflow JSON Schema from the
  installed node descriptors: `node.type` becomes an enum of the node types the
  deployment has, and each type contributes an `if`/`then` branch that points
  `node.config` at its own schema. The published schema therefore completes node
  types, configuration keys, defaults, enums and hover documentation, which is
  what the original single-process implementation got from generating one schema
  from every node model.
- `GET /api/v1/schema/{document}` serves those documents from a running runtime,
  so `$schema` can point at the deployment's own catalog.
- `nodara-cli schema` grew `--plugin-dir` (include plugin descriptors) and
  `--no-capabilities` (publish the catalog-free schema).
- Studio: the workflow model keeps `$schema` across new, import, export and
  agent-plan documents, and the Workflow JSON tab shows the active reference and
  can point it at the runtime.
- Every shipped node descriptor now documents its configuration: `title`,
  `description`, `default`, `enum`, bounds and examples per property.
- `scripts/build-artifacts.ps1` produces runnable Windows x64 debug and release
  ZIPs with tests, plugins, examples, schemas, documentation, PDB files for
  debug, NSIS/MSI for release, `build-info.json` and SHA-256 checksums.
- The Studio now has a source SVG and generated multi-platform application
  icons, allowing Tauri to create Windows resources and installers.

### Changed

- The Rust `Workflow` model round-trips `$schema` (`Workflow::schema_url`), so
  migration, load and save preserve an editor's content hints.
- The Tauri desktop build now connects directly to `http://127.0.0.1:8710`;
  browser builds continue to use relative `/api` URLs for proxying.
- Desktop Studio now starts a sibling runtime automatically when port 8710 is
  idle, reuses an existing runtime, and stops only the child it owns. Release
  installers bundle the runtime and official plugin manifests/binaries.
- Studio usability: palette items can be added with one click, connections are
  thicker with a wide hit target, node/connection context menus can delete them,
  and the left, right and bottom panes have draggable dividers.

### Documentation

- Bilingual guides, following the `name.md` (English) / `name.zh.md` (Chinese)
  convention: `docs/project.md` (the whole project), `docs/schema.md` (every
  JSON Schema document, field by field, plus how content hints are composed)
  and `docs/nodes.md` (ports, permissions and configuration for every shipped
  node type), with a matching `docs/README.md` index in both languages.
- `cargo test -p nodara-cli` now checks the node reference against the shipped
  catalogue: a node type, permission or configuration key that is not
  documented fails the build in both languages.
- Added packaged-product quick starts, user manuals, acceptance test guides and
  artifact guides in English and Chinese, linked from the root README files and
  the documentation index.

### Fixed

**Core**

- `windows.Input.Text` now holds `Shift` for uppercase letters; previously
  every capital was typed as lowercase.
- Secret variables no longer leave the engine: run snapshots, `run --json` and
  the variable scope shipped to plugin processes all carry masked values.
- Policies and approval requests now see the resolved node configuration, as
  the `CapabilityRequest::input` contract always documented.
- A malformed window selector is a configuration error instead of silently
  falling back to the foreground window.
- The expression evaluator rejects expressions nested beyond 128 levels
  (`E_TOO_COMPLEX`) instead of overflowing the stack.
- Desktop capture rejects non-positive or oversized width/height instead of
  casting them into huge allocations.
- `PluginHost::cancel_run`/`shutdown` no longer hold the plugin mutex across
  blocking RPCs.
- `core.Calculate` documentation now matches the grammar (`^`, comparisons,
  `&&`/`||` — not `**`/`sqrt`).

**Agent**

- A run the agent stops watching (timeout, lost contact) is cancelled instead
  of being left running unattended.
- Every error after a session is published marks the session failed, so the
  Studio no longer sees sessions stuck in `planning`.
- A missing API key fails the command immediately instead of degrading to an
  empty scripted provider.

**Studio**

- The periodic health poll no longer overwrites JSON the user is editing, and
  keeps the palette filter.
- Local validation detects directed cycles (the runtime rejects them later; the
  editor now says so immediately).
- Non-JSON HTTP responses surface a structured error instead of a raw
  `SyntaxError`.
- Newly added nodes start from their schema defaults.
- Palette drag-and-drop now uses pointer events, so nodes can be dropped at the
  pointer position even in WebView builds. Dragging also suppresses text
  selection and no longer re-renders the node being moved on every frame.
- Connection arrowheads use a small fixed user-space size. Tall links bend
  their final tangent so the arrow follows the visible approach instead of
  remaining horizontal.

### Fixed (continued)

**Core**

- `JsonlAuditLog` continues the sequence after reopening the same file instead
  of restarting at 0, and a failed write reports to stderr instead of being
  dropped silently.
- The WebSocket event stream no longer duplicates an event published between
  subscribing and replaying the history (replay and live stream are
  de-duplicated by sequence number).
- `windows.Input.Keyboard` accepts the `+` key in a chord (`ctrl++`), which
  previously produced a confusing "unknown key chord".
- `nodara-cli serve` builds the policy once (from `RuntimeConfig.policy`);
  the second, overriding copy of the same decision was removed.
- A failed run-thread spawn surfaces as `E_THREAD` on the run instead of
  aborting the process (engine and runtime).
- `windows.Input.Keyboard` chords whose last key is written in uppercase
  (`Ctrl+S`) no longer extra-hold Shift. Capital Shift is only for
  `windows.Input.Text` typing a letter.
- `serve --in-process` (and `register_vision`) now honours `NODARA_OCR_COMMAND`,
  matching the vision plugin binary.
- `PluginHost::install_into` no longer replaces an in-process executor with a
  stdio plugin of the same node type, so `run`/`serve --in-process --plugin-dir`
  keeps the embedded official capabilities.
- `windows.Window.Capture` describes a full-window capture (including chrome),
  matching `GetWindowRect`.

**Agent**

- The step budget is charged *before* every model call, so `max_steps` caps
  the round trips it counts instead of detecting overspend afterwards.
- `--offline` is implemented: no capability discovery, no session publication,
  structure-only local validation — planning works with the runtime down.
- Provider HTTP errors include the provider's diagnostic body (rate limits,
  quota) instead of the bare status line.
- Ids passed on the command line are percent-encoded in URL paths and query
  values.

**Studio**

- A stale event-stream close no longer refreshes a newer run's state.
- The Agent panel reports the transition into a poll failure once instead of
  spamming the event log every 1.5 seconds.
- Concurrent validate/audit requests are sequenced so a slow earlier response
  cannot overwrite newer results.

### Changed (engineering)

- CI runs clippy for the agent workspace alongside core, and explains why the
  plugin-dependent and legacy examples are not part of plain validation.
- Core clippy `-D warnings`: newest-first lists and descriptor sorts use
  `sort_by_key` (unblocking Core CI).

## [2.0.0] — plugin-ecosystem rearchitecture

The repository was rebuilt around the contracts described in `.tmp/PLAN.md`:
three independent product repositories joined only by JSON Schema and the
runtime API.

### Added

**Core (`Nodara-Core/`)**

- `nodara-schema` — workflow (`schema_version` 2.0), plugin manifest, node descriptor
  and execution event contracts, all deriving `JsonSchema` so the published
  schemas are generated rather than hand-maintained.
- Structured validation that never stops at the first problem: stable diagnostic
  codes (`WF1xx`), severities, JSON-pointer paths and repair hints, plus
  capability-aware checks against the live registry.
- Graph algorithms (topological order, cycle detection, reachability) shared by
  the editor, the CLI and the agent.
- `nodara-cli migrate`, which upgrades legacy documents: namespaced node types,
  `from`/`to` to `source`/`target`, and configuration keys that changed meaning
  (`seconds` to `duration_ms`).
- `nodara-core` — the `NodeExecutor` SDK, `CapabilityRegistry`, `WorkflowEngine`,
  deterministic pause/resume/step/cancel, policy decisions and an audit log.
- A dependency-free expression evaluator with a documented grammar.
- `nodara-plugin` — JSON-RPC 2.0 over stdio, manifest discovery, a plugin host, an
  in-process transport for tests, and `serve_stdio` for writing plugins.
- `nodara-runtime` — the headless runtime with the HTTP/WebSocket API, a run manager
  and a policy layer.
- `nodara-platform` and `nodara-vision` — real Windows capabilities (input, windows,
  capture, clipboard) and vision (template matching, pluggable OCR), each built
  as both a library and a plugin binary.
- `nodara-cli` — `validate`, `run`, `simulate`, `inspect`, `migrate`, `plugins`,
  `schema` and `serve`.
- `nodara-testkit` — a workflow builder, recording and failing executors, and an
  in-process plugin harness.

**Studio (`Nodara-Studio/`)**

- A typed web client with no hardcoded node types: the palette and every
  configuration form are built from the runtime's descriptors.
- An SVG graph editor with drag, connect, select, delete, schema-driven
  properties and a live event viewer.
- A Tauri v2 shell around the same bundle.

**Agent (`Nodara-Agent/`)**

- A provider-neutral planner with a draft to validate to repair loop that feeds
  the runtime's own diagnostics back to the model.
- Guardrails, budgets, a JSON Lines decision trace and replay.

### Changed

- The workspace layout is now three sibling repositories instead of one crate
  tree with an embedded editor and an embedded agent.
- `nodara-platform` and `nodara-vision` are no longer core modules; the core does not
  reference them.
- Workflow documents use `schema_version` and namespaced node types
  (`core.Start`, `windows.Input.Keyboard`, `vision.Ocr`).
- Example workflows were rewritten in the current format; the legacy shape is
  kept as a migration fixture under `examples/legacy/`.

### Removed

- The `nodara-agent` crate from the core workspace (superseded by
  `Nodara-Agent`).
- The old `studio/` prototype from the core repository (superseded by
  `Nodara-Studio`).
- The `meval` dependency: expression evaluation is now in-crate, which also
  removes an unmaintained transitive dependency.

### Security

- Agent capability calls are authorised by the runtime, not by a prompt. The
  agent links only the contract crate, so it has no route to execution that
  bypasses policy.
- Every capability evaluation, approval and denial is written to the audit log.

## [1.0.0]

Initial Rust rewrite of the Python Nodara-Core: a workflow schema, a
graph-based executor, Windows platform adapters, vision helpers and a CLI.
