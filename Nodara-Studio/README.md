# Nodara Studio

The visual editor for Nodara. It is a pure client of the runtime's
HTTP/WebSocket API — it contains no execution logic and no knowledge of any
specific node type.

## Why it is built this way

The architecture document requires that adding OCR, browser, database or
third-party plugins must not require rebuilding the UI. So the editor never
hardcodes a node type. At start-up it:

1. connects to the runtime (`GET /api/v1/health`);
2. lists installed plugins (`GET /api/v1/plugins`);
3. fetches every node descriptor (`GET /api/v1/node-types`);
4. builds the palette from `category` and `display_name`;
5. builds each configuration form from that descriptor's `config_schema`;
6. badges nodes whose descriptor is `dangerous`.

Install a plugin, restart the runtime, reload the Studio — the new nodes are
simply there.

## Running it

Start the runtime first (from `Nodara-Core/`):

```bash
cargo run -p nodara-cli -- serve --in-process --plugin-dir plugins
```

Then, from this directory:

```bash
npm install
npm run dev          # http://localhost:4173
```

Vite proxies `/api` to `http://127.0.0.1:8710`, so there is no CORS setup in
development. Point it elsewhere with `NODARA_RUNTIME_URL`:

```bash
NODARA_RUNTIME_URL=http://192.168.1.10:8710 npm run dev
```

## Building

```bash
npm run build        # type-check, then bundle into dist/
```

The Tauri source icon is `src-tauri/app-icon.svg`; generated platform assets live
under `src-tauri/icons/`. Regenerate them after changing the source icon:

```bash
npx tauri icon src-tauri/app-icon.svg
```

## Desktop shell

`src-tauri/` is a Tauri v2 shell around the same web app:

```powershell
.\node_modules\.bin\tauri.cmd dev
.\node_modules\.bin\tauri.cmd build --debug --no-bundle
.\node_modules\.bin\tauri.cmd build --bundles nsis msi
```

The desktop build connects directly to `http://127.0.0.1:8710`. At startup it
reuses an existing runtime or starts a sibling `nodara-runtime.exe` automatically;
a runtime started by Studio is stopped when Studio exits. This makes the packaged
desktop app a one-click entry point while keeping the browser build unchanged.
The browser build still uses relative `/api` URLs so Vite or a reverse proxy can
route them.

Debug builds intentionally retain the console window for startup and runtime
logs. The child runtime is launched without an additional console window.

## Language

Studio supports English and Simplified Chinese. Use the `中文` / `English`
button in the toolbar to switch languages. The choice is stored locally and
also localizes node names, categories, configuration labels and descriptions.
Runtime-provided diagnostic text remains in its original language unless a
known validation code has a Studio translation.

## Canvas controls

* **Click** a node in the palette to add it near the canvas centre, or drag it to
  an exact position. A workflow can contain exactly one `core.Start`; the
  palette disables Start after one is present.
* Drag a node to move it. Drag from an output port to an input port to connect.
* Use the mouse wheel to zoom, middle-drag (or hold Space and drag) to pan, and
  the canvas toolbar to zoom in/out, reset to 100%, fit all nodes or apply an
  automatic left-to-right topology layout.
* Select a node or connection and press `Delete`, or right-click it and choose
  the delete command. Connections have a wide invisible hit target.
* Duplicate a selected node with `Ctrl+D` or the node context menu. Configuration
  and execution settings are preserved and the copy receives a fresh id. Start
  is single-instance, so it cannot be duplicated or imported more than once.
* Selecting a connection opens its Properties: choose whether it follows
  **Always**, **Success**, or **Failure**, then edit its label or guard
  expression directly. Failure edges provide explicit recovery paths; branch
  labels and colors are shown on the canvas.
* Drag the dividers between the left/right panels or above the bottom drawer to
  resize them; double-click a divider to restore its default size.
* A new document starts with a runnable `Start → Log → End` example so the
  first Run can be verified immediately.
* Undo and redo workflow edits with the toolbar buttons, `Ctrl+Z`, `Ctrl+Y` or
  `Ctrl+Shift+Z`; rapid typing is coalesced into a single history entry.
* Schema-driven forms mark required fields, show descriptions, use examples as
  placeholders, render nested object schemas as grouped controls, edit arrays as
  reorderable lists and offer a reset-to-default action when the Schema declares
  a default.
* With no node selected, Properties edits workflow ID, name, description, tags,
  author and version, and manages workflow variables (default value,
  description, secret flag and session-only run overrides). The **Variables…**
  toolbar action opens all run overrides in one dialog for quick testing.
* The Properties panel exposes common execution settings for every node:
  enabled pass-through, run condition, pre/post delay, continue-on-error,
  retry count, retry delay and common result mapping (`result_var` /
  `result_port`). Plugin nodes also expose a per-attempt timeout. The node
  context menu can toggle a node on or off immediately.
* Every edit is validated automatically after a short debounce. The status
  beside **Validate** shows `checking`, `valid`, or the error count; errors
  disable **Run** until they are fixed and are listed in **Problems**.
* **Run** validates once more immediately before submitting the workflow, so a
  stale automatic result cannot start an invalid graph.

## Layout

```text
src/
├── main.ts                composition: connect, render, execute
├── styles.css             design tokens and layout
├── runtime/
│   ├── types.ts           the wire contract, mirrored from nodara-schema
│   └── client.ts          the only module that knows about HTTP and WebSocket
├── model/
│   └── workflow.ts        the document model and local sanity checks
├── schema/
│   └── form.ts            JSON Schema -> form fields
└── ui/
    ├── palette.ts         nodes discovered at runtime
    ├── canvas.ts          SVG graph editor: drag, connect, select, delete
    ├── inspector.ts       schema-driven configuration forms
    ├── event-log.ts       the runtime event stream
    ├── run-dialog.ts      temporary run-variable overrides
    ├── run-panel.ts       run history
    ├── extension-panel.ts unified extension registrations
    ├── feature-registry.ts ordered drawer/panel registration
    └── agent-panel.ts     agent sessions, plan preview, approval prompts
```

## What the editor does

* drag nodes from the palette, or double-click to add them;
* drag from an output port to an input port to connect;
* select a node or an edge and press `Delete` to remove it;
* edit configuration through generated forms, or edit the whole document as
  JSON in the **Workflow JSON** tab;
* `Validate` calls the runtime, so diagnostics match exactly what execution
  would enforce — including unknown node types and missing configuration;
* `Run`, `Pause`, `Resume`, `Step`, `Cancel` map one-to-one onto run control;
* the **Runs** tab lists runtime history and opens a selected run in the
  **Events** tab; the event stream highlights the running, finished and failed
  nodes on the canvas;
* the **Extensions** tab lists built-in, in-process and plugin registrations,
  including source, capabilities, node counts and load state;
* drawer tabs are generated by a `FeatureRegistry`, so built-in, host and future
  plugin UI contributions share one registration and ordering path.

## The Agent tab

The Studio never calls the agent. It reads `GET /api/v1/agent/sessions` — the
session API the architecture document designates as the channel between them —
and writes back exactly one thing: an operator's decision on a pending approval.

That decision is not cosmetic. When policy requires approval, the runtime's
approval handler is *blocking the run thread*; the Approve button in this panel
is what releases it. The session card shows:

* the goal and the conversation between operator, agent and runtime;
* the **plan preview** — the proposed graph, its node list, and the runtime's own
  validation diagnostics, with a button to load it into the editor;
* every approval request, with the node, the permissions it wants and the exact
  input it would receive, so the decision can be judged rather than rubber-stamped;
* a link to the run the session started, which opens in the Events tab.

The tab is marked when something is waiting, so an approval is not missed.

## The Audit tab

The plan's final migration step calls for an audit interface. This is it: the
durable record of every capability the runtime evaluated, every approval, and
every node outcome, read from `GET /api/v1/audit`. The table shows the time, the
run, the category, the node, the capability, the decision and the message, with
decisions and failures highlighted and a filter for the run currently being
watched.

## Tests

```bash
npm test
```

Thirty tests cover the six areas the architecture document lists:

| Area | File |
|------|------|
| Dynamic node discovery | `src/ui/palette.test.ts` |
| Schema-driven configuration forms | `src/schema/form.test.ts` |
| Node drop, connect and delete | `src/ui/canvas.test.ts` |
| Run-status visualisation | `src/ui/event-log.test.ts` |
| Runtime disconnect and recovery | `src/runtime/client.test.ts` |
| Document model and local validation | `src/model/workflow.test.ts` |
| Audit rendering | `src/ui/audit-panel.test.ts` |
