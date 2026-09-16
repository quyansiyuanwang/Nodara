# Changelog
## Unreleased

- Workflow `schema_version` 2.1 with explicit `control` and `data` edges.
- Legacy 2.0 documents now fail with `WF118` until migrated; migration removes ambiguous data-port mappings and reports those that must be recreated.
- New `edge_activated` and `data_transferred` runtime events drive accurate Studio edge animation and traversed-path highlighting.
- Studio adds an unbounded canvas, marquee/Ctrl/Shift multi-selection, group execution settings, data/execution ports, and persistent floating quick configuration.
- Desktop Studio adds a conversational Agent panel backed by `nodara-agent studio`, shared runtime sessions, provider settings, current/previous-plan baselines, and plan-only/manual/partial/automatic execution modes.
- Runtime `POST /runs` accepts per-run `approval: auto|session`; manual Agent runs start paused and automatic runs retain capability audit records.
- Studio Agent Provider/final-JSON expanders persist across polling and the page renders local controls before the runtime is reachable; the Properties rail uses denser card groups, and graph nodes are reduced to 200x108 with common-config summaries.
- Studio adds persistent workflow node groups in the metadata extension bag, group-frame selection/movement, unified multi-selection execution configuration, a workflow-wide group manager, grouped/persistent drawer tabs, tab badges and local Agent session search.
- Canvas nodes no longer render decorative vertical accent bars, and the floating quick configuration opens only after a click without node movement; dragging or marquee selection no longer opens it.
- Studio now keeps Inspector and Agent input focus/caret across document re-renders and session polling, adds a restrained idle edge-flow cue, and replaces flashy run-time pulses/glows with static highlights plus one slower moving arrow per active edge.

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — schema-driven content hints

### Added

- `windows.Window.Wait` now supports `mode=appear|disappear`, allowing workflows to wait for a window to close as well as open while retaining cancellation and timeout handling.
- Edge context menus can switch directly between Always, Success and Failure branches without moving to the Properties panel first.
- The Runs panel now reports accurate started/finished timestamps, duration, event and artifact counts, and supports text/status filtering. The previous `Started` column incorrectly displayed the finished time for completed runs.
- Added `core.CalculateMany` for ordered named calculations. Results are published as they are evaluated, so later expressions can reuse earlier names, and an optional `output_var` receives the complete result object.
- Keyboard nodes can repeat a chord with `repeat` / `repeat_interval_ms`, and mouse click actions accept `click_count` / `click_interval_ms` while retaining the legacy double-click interval.
- Canvas and Runtime validation now enforce explicit port names and value-type
  compatibility. Studio refuses incompatible connections immediately with a
  visible status message, while imported JSON receives the stable `WF115`,
  `WF116` and `WF117` diagnostics.
- `vision.TemplateMatch` supports a bounded search region, `fail_if_missing`, and publishes `center_x` / `center_y` in addition to the matched rectangle. The centre values can feed directly into later mouse or click nodes through typed templates.
- Node configuration templates now preserve JSON types according to the target
  schema: an exact value such as `{{match.x}}` remains an integer/number/boolean,
  while mixed strings such as `"x={{match.x}}"` still render as text. Dynamic
  coordinates, durations, thresholds and feature flags no longer require
  intermediate string conversion. Studio form fields numbered/binary expose a
  `{}` mode switch so these templates can be entered without hand-editing JSON.
- The Studio Events panel now pauses automatic follow when the operator scrolls up to inspect history and resumes follow when the list returns to the bottom.
- Studio Events can now inspect every `node_finished` output as collapsed JSON,
  even for nodes without a dedicated renderer. Image artifacts are discovered
  recursively through nested objects and arrays, so vision results and wrapped
  screenshots preview inline instead of leaving broken image links.
- Workflow nodes now support a persisted `breakpoint` flag. The engine pauses
  immediately before the selected node and emits a normal `run_paused` event;
  Studio exposes it in Execution settings and the node context menu, binds it to
  `F9`, and draws a visible marker on the canvas. Disabled and condition-pruned
  nodes do not trigger it.
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
- Studio opens a runnable `Start → Log → End` starter workflow on first
  launch and when creating a new document, so Run can be exercised immediately.
- Every node now supports common `result_var` and `result_port` output mapping.
  Any successful node can publish a selected output under a run-scope variable,
  independent of its node-specific configuration. Studio exposes both fields in
  Execution settings; validation reports `WF144`/`WF145` for invalid mappings
  and treats mapped names as declared for later `{{template}}` references.
- Failed executions now publish a structured `last_error` run variable with
  `code`, `message`, `node_id` and `retryable`. Failure branches can use it in
  guards and recovery logs, and validation treats the namespace as declared.
- Edges now support explicit `success` and `failure` branches. A failure edge
  handles executor errors and activates a recovery path without requiring a
  node-wide `continue_on_error`; Studio exposes the outcome in connection
  Properties and colors/labels the branch on the canvas.
- Regenerated the published workflow and agent-session schemas so editor hints
  include current execution controls and edge branches.
- Studio schema-driven forms now render array-valued configuration as an
  editable list with add, remove, reorder, default-reset, minimum and maximum
  item controls instead of falling back to a raw JSON textarea.
- Workflow Properties now edit the metadata author and workflow version fields,
  so these common document settings no longer require the JSON tab.
- Plugin manifests can now declare multiple `features`, including
  `integration`, `ui` and `policy` contributions. Each feature is validated and
  registered as its own extension with a stable `plugin/feature` id.
- Plugin artifacts now cross the process boundary instead of being trapped in
  the plugin's temporary store. Screenshots remain available after the run and
  are previewed inline under the producing node in the Studio Events tab.
- Configuration forms now show an accessible `?` tooltip for every documented
  field, and the bottom drawer starts at 300px, expands to 640px and remembers
  the user-selected height.
- `nodara-cli extensions` now lists the same unified built-in, in-process and
  plugin registrations as `GET /api/v1/extensions`, with JSON output for
  automation.
- Studio drawer tabs are now generated through an ordered `FeatureRegistry`.
  Built-in panels and future host/plugin UI contributions register through the
  same path instead of being hardcoded in `index.html` and `main.ts`.
- Built-in, in-process and plugin capabilities now share one
  `ExtensionRegistry`. The runtime exposes unified registration metadata at
  `GET /api/v1/extensions`, including source, kind, capabilities, permissions,
  node types and load state. Studio adds an **Extensions** tab for this view.
- Studio now has a **Variables…** run dialog. It collects every workflow variable
  in one place, supports temporary run-only overrides, masks stored secrets,
  validates numeric/JSON input before Run and can clear overrides individually.
- Studio now has a **Runs** tab that lists runtime run history. Any entry can be
  opened directly into its event stream, with status, workflow, node count and
  start time shown in the table.
- Studio now supports bounded workflow undo/redo snapshots through
  `Ctrl+Z`, `Ctrl+Y`, `Ctrl+Shift+Z` and toolbar buttons. Edits are coalesced so
  continuous typing does not create one history entry per character.
- The Studio canvas can automatically arrange nodes into deterministic
  left-to-right topology columns, preserving branches side by side.
- The Studio canvas now supports wheel zoom, middle/space-drag panning,
  fit-to-content and reset-to-100% controls, with the current zoom shown in the
  canvas toolbar.
- Studio’s schema-driven configuration forms now mark required fields, show
  inline descriptions, use schema examples as placeholders, apply common string
  constraints, render nested object schemas as editable groups instead of raw
  JSON and provide one-click reset-to-default controls.
- Studio can duplicate a node with `Ctrl+D` or the node context menu,
  preserving configuration, enabled state, delays and retry settings while
  assigning a fresh node id.
- Studio variables now support session-only run overrides. The workflow keeps
  its default value while `POST /runs` receives the overridden scope for the
  next executions.
- Studio can edit workflow metadata and variables without opening JSON: ID,
  name, description, tags, variable values, descriptions and secret flags are
  available in Properties, with add/delete actions for variables.
- Studio now shows connection properties when an edge is selected, allowing
  edge labels and guard expressions to be edited directly and validated
  automatically.
- Workflow nodes now support common execution controls: `enabled`, `condition`,
  `delay_before_ms`, `delay_after_ms`, `continue_on_error`, `timeout_ms`,
  `retry` and `retry_delay_ms`. The engine
  skips disabled nodes as pass-throughs, applies cancellation-aware delays and
  retries failed executions; Studio exposes them in Properties and a node
  context-menu enable/disable action.
- Studio now ships an English/Chinese i18n layer. The toolbar language selector
  is persisted locally and localizes the editor chrome, node/category names,
  configuration titles/descriptions, validation status, problems, events,
  approvals, audit headings and workflow controls.
- `scripts/build-artifacts.ps1` produces runnable Windows x64 debug and release
  ZIPs with tests, plugins, examples, schemas, documentation, PDB files for
  debug, NSIS/MSI for release, `build-info.json` and SHA-256 checksums.
- The Studio now has a source SVG and generated multi-platform application
  icons, allowing Tauri to create Windows resources and installers.
- `POST /api/v1/runs` accepts `start_paused`, and Studio's **Step** action can
  start a paused run from idle and advance exactly one node per click.
- `windows.Desktop.Capture` Properties now includes a mouse-driven screen
  region picker. The desktop shell hides itself during capture, renders the
  fresh screenshot, converts the drag to source pixels and writes
  X/Y/Width/Height back into the node.
- Studio now previews artifacts logged as JSON in addition to artifacts in
  `node_finished` outputs, so logging a capture metadata variable shows the
  image inline.
- The platform plugin now provides `system.Command`. It can launch a program or
  shell command with arguments, working directory, environment variables and
  stdin; captures stdout/stderr, exit code and PID; supports synchronous or
  background execution; and terminates the full child process tree on node
  timeout or run cancellation. The capability is registered as the separate
  `process` plugin feature and requires `process.execute`. Studio expands its
  stdout, stderr, exit code and PID directly in Events.
- Window selection now supports executable **process name** and **visible-only**
  filters in addition to title and class. The shared selector is used by Find,
  Focus, window capture and input targeting, and Find publishes the resolved
  process and visibility in its output record.
- Keyboard input now supports `type`, `press` and `release` actions with an
  optional hold duration. Mouse input now supports explicit left/right/middle
  buttons, relative coordinates, configurable click/double-click timing and
  smooth `drag` with optional start coordinates and destination duration.
- Keyboard and text nodes can now target a window in the background without
  changing focus. Keyboard input sends `WM_KEYDOWN`/`WM_KEYUP`; text uses
  `WM_CHAR` by default and offers a direct `WM_SETTEXT` fallback for controls
  that ignore posted character messages. A clipboard-paste strategy writes and
  restores the clipboard around a posted Ctrl+V for applications that require
  paste semantics.
- Background mouse input now converts screen coordinates to the target window's
  client space and sends `WM_MOUSEMOVE` plus left/right/middle button messages,
  including smooth drag. It restores the original system cursor position after
  the operation.
- Added `windows.Window.Wait`, a cancellable polling node with configurable
  timeout and interval. It reuses the shared title/class/process/visibility
  selector and publishes the same window record as `windows.Window.Find`.
- Problems rows that reference a node or connection are now clickable and centre
  the target on the canvas for immediate correction.
- Studio now keeps an unsaved workflow draft in `sessionStorage`; language
  switches and accidental reloads restore the current tab's document, while
  closing the tab clears the session-only copy.
- Canvas connections can now be made by clicking an output port and then an
  input port, in addition to drag-and-drop. `Escape` cancels the pending
  connection.
- Studio Properties now groups Execution, Configuration and Variables into
  collapsible sections and remembers each section's expanded state.
- The Events drawer now has a filter field, visible/total count, clear button
  and Follow toggle so long runtime logs remain navigable.
- Node retries now support fixed or exponential backoff with an optional maximum
  delay. Both settings are editable directly in the Execution section.

### Fixed

- Manual Pause and Resume now publish `run_paused` / `run_resumed` events into the run history, so WebSocket clients and replayed event streams stay consistent with the run snapshot.

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
- Validation now checks variable references in node and edge conditions with
  `WF151`/`WF152`, in addition to existing `{{template}}` reference checks.
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
- Studio no longer starts in a false `pending` state, which previously left the
  Run button disabled even when no run was active.
- Studio validates automatically after every workflow edit (debounced while
  typing), displays the result beside Validate, disables Run on errors, and
  validates again immediately before starting a run.
- Studio enforces exactly one `core.Start` node at every editing boundary.
  The palette disables Start once one exists, direct additions and duplicates
  are rejected, and import/JSON/agent documents with multiple Starts are not
  applied.
- Runtime snapshots and Studio now handle `RunPaused`/`RunResumed`, so a
  start-paused run visibly returns to `paused` after each step instead of
  leaving Step disabled in a false running state.
- A fresh drawer height now starts at a useful `280-420px` viewport-aware size;
  previously a missing local-storage value was parsed as `0` and clamped to the
  140px minimum, making event images difficult to inspect.

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
