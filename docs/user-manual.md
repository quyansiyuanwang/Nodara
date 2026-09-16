# Nodara User Manual

> Chinese: [user-manual.zh.md](user-manual.zh.md)

This manual covers the prebuilt Nodara 2.0.0 Windows x64 package. For source
development and packaging, see `README.md`, `QUICKSTART.md`, and
`scripts/build-artifacts.ps1`.

## Components

| Component | Executable | Purpose |
|---|---|---|
| CLI | `nodara-cli.exe` | Validate, simulate, run, migrate, export schemas, and serve |
| Runtime | `nodara-runtime.exe` | HTTP/WebSocket execution and policy service |
| Platform plugin | `nodara-platform-plugin.exe` | Keyboard, mouse, windows, capture, clipboard, process execution |
| Vision plugin | `nodara-vision-plugin.exe` | Template matching and OCR |
| Agent | `nodara-agent.exe` | Natural-language planning and operation through the runtime |
| Studio | `nodara-studio.exe` | Visual editor, events, approvals, and audit |
| Studio Web | `studio-web\` | Browser build requiring an `/api` reverse proxy |

CLI, Studio, and Agent all reach capabilities through the runtime. They do not
execute plugin nodes themselves.

## First run

From the extracted package directory:

```powershell
.\nodara-cli.exe validate .\examples\hello-world.json
.\nodara-cli.exe simulate .\examples\hello-world.json
.\nodara-cli.exe run .\examples\hello-world.json
```

Start the runtime in another terminal:

```powershell
.\nodara-runtime.exe
```

It listens on `http://127.0.0.1:8710/api/v1` and discovers plugins under its
adjacent `plugins\` directory. Check it with:

```powershell
Invoke-RestMethod http://127.0.0.1:8710/api/v1/health
Invoke-RestMethod http://127.0.0.1:8710/api/v1/plugins
```

Start the desktop editor:

```powershell
.\nodara-studio.exe
```

The desktop build connects directly to `http://127.0.0.1:8710`. It first probes
the port: an existing runtime is reused, while a missing runtime is started from
the sibling `nodara-runtime.exe` (up to an eight-second readiness wait). Only a
runtime started by Studio is stopped when Studio exits. The editor reconnects
every five seconds if the service is unavailable.

Debug builds intentionally keep the Studio console window open for startup and
runtime logs. The automatically started runtime does not create a second
console window.

If the badge still says `runtime unreachable`, verify that
`nodara-runtime.exe` is beside Studio and that another program is not using port
8710. Set `NODARA_RUNTIME_BIN` to select a different runtime executable.

### Language

Use the `中文` / `English` button in the toolbar. Studio persists the choice and localizes the editor UI and known node/configuration labels.
Unsaved edits are kept in the current browser tab, so changing language or
reloading the page restores the draft. Closing the tab clears this session-only
copy.

### Canvas controls

| Action | Method |
|---|---|
| Add a node | **Click** it in the palette, or drag it to an exact canvas position; exactly one `core.Start` is allowed |
| Move a node | Drag the node body |
| Create a connection | Drag from an output port to an input port or the target node body. A unique compatible port is inferred automatically; ambiguous data ports show a chooser. Press `Escape` to cancel |
| Create control and data together | Hold `Alt` while dragging from a node body, data output or execution output. Unique ports connect automatically, ambiguous data ports open a chooser, and partial success is reported without rolling back the valid edge |
| Themes and local colors | Choose Obsidian, Graphite, Ocean, Ember, Paper or High Contrast from the toolbar. Node and edge colors can be overridden in Properties and reset to theme defaults |
| Delete a node or connection | Select it and press `Delete`, or right-click it and choose delete |
| Edit a connection | Select it to edit label and guard, or right-click it to switch directly between Always, Success and Failure; the wide hit target makes thin edges easier to select |
| Automatic validation | Run after every edit; the status beside `Validate` shows the result and errors disable `Run` |
| Locate a problem | Click a Problems row that names a node or connection; the canvas centres it and opens Properties |
| Filter events | Use the Events toolbar to filter by type/node/message; Clear only clears the view, and Follow controls automatic scrolling |
| Resize the layout | Drag the dividers beside the left/right panels or above the bottom drawer; double-click to reset |
| Collapse sections | Expand or collapse Execution, Configuration and Variables in Properties; Studio remembers the state |
| Undo / redo | Use the toolbar buttons, `Ctrl+Z`, `Ctrl+Y`, or `Ctrl+Shift+Z` |
| Navigate the canvas | The world is unbounded and supports negative coordinates; wheel to zoom, middle-drag or Space+left-drag to pan. Reaching a viewport edge never pans automatically |
| Edge activity animation | Idle edges show a restrained direction cue; runtime `edge_activated` / `data_transferred` events move one slower arrow along the traversed edge. Pausing or finishing stops the arrow and retains the path highlight until the next run or clear |
| Auto layout | Use **Auto layout** to arrange the graph in left-to-right topology columns |
| Duplicate a node | Use `Ctrl+D` or the context menu; `core.Start` is single-instance and cannot be duplicated or imported twice |
| Bulk configuration | Use the Properties bulk form or floating quick card to set enabled, breakpoint, condition, delays, continue-on-error, retry and timeout together |
| Floating quick card | **Plain-click** a node without moving it to open the card; dragging, marquee selection, `Ctrl+click` and `Shift+click` never open it. After a multi-selection, plain-click any selected node to edit the shared execution fields |
| Node groups | Create a persistent group from a selection, drag the group frame as a unit, and rename, recolor, remove members or select the group from Properties |

### Compact layout and motion

The Studio uses a compact operator layout: toolbar controls, inspector fields, event rows and the default bottom drawer are intentionally dense. Node categories in the left palette are collapsible and remember their state; only **Core** opens initially. Drag the dividers to enlarge any pane when a particular task needs more room.

The Properties rail is an independently scrolling stack of compact cards. Graph nodes use tighter information blocks and show a short common-config summary instead of unused empty space. Agent Provider settings and final-JSON sections stay expanded across session polling and data refreshes. Themes are global Studio settings, while node and edge color overrides travel with the workflow.

The Agent workspace supports named Provider Profiles, prompt templates and per-turn extra instructions. API keys are stored in Windows Credential Manager and never enter localStorage, workflow JSON or logs. Model output streams token by token; Stop cancels only the current generation and leaves an active workflow untouched. Every result shows node/edge diffs, diagnostics, final JSON, decision Trace and execution actions. Loading or running a plan always requires explicit confirmation.

Audit provides summary cards, a timeline and an execution graph in the drawer, plus a full workspace view. Live runs update node and traversed-edge state, and selecting a graph node or timeline record opens structured and raw details. Runs without a workflow snapshot fall back to the record list.

Motion is part of execution feedback, not decoration:

- while idle, every edge carries a slow, low-contrast moving dash that keeps direction visible without visual noise;
- while running, the active node uses a static highlight without pulses, pop effects or heavy glow;
- activated control and data edges keep their highlight and show one slower moving arrow; the dash trail is only a faint cue;
- completed paths keep their highlight until the next run or **Clear**.
- Inspector and Agent inputs retain focus and caret position across automatic validation, canvas redraws and session refreshes.

If a desktop policy disables general operating-system animations, Studio still keeps its workflow-edge motion enabled so execution direction remains visible.

#### Node groups and unified configuration

Group definitions are stored in `metadata.extensions["studio.groups"]`, so they survive Studio import/export, Runtime validation and Agent-returned workflow JSON. A multi-selection exposes bulk execution settings and group membership in Properties. Group frames can be selected and moved as a unit; the context menu can rename or delete them, and the no-selection Properties view provides a workflow-wide group manager.

### Workflow settings and variables

When no node or connection is selected, Properties edits the workflow ID, name, description, tags, author and version. It also lists workflow variables and provides controls to add, edit or delete them, including descriptions and secret flags. Each variable can have a session-only **Run value override**, which is passed to `/runs` without changing the workflow default. Schema-valued arrays are edited as reorderable lists with add, remove and reset controls.

### Common node execution settings

Every node has an **Execution** section in Properties. These settings are part
of the workflow document and are honored by the runtime:

| Setting | Purpose |
|---|---|
| Enabled | Disable a node to skip it while passing its incoming branch through to its outgoing edges |
| Run condition | Optional expression; a false result skips the node and prunes its outgoing branches |
| Breakpoint before node | Pause automatically before this node executes; `Resume` continues and `Step` executes only this node |
| Delay before (ms) | Wait before executing the node |
| Delay after (ms) | Wait after successful execution before activating outgoing branches |
| Continue on error | After retries are exhausted, continue through eligible Always/Failure branches instead of failing the run |
| Timeout (ms) | Optional time limit for one plugin execution attempt |
| Retries | Number of additional attempts after a failed execution |
| Retry delay (ms) | Wait between failed attempts |
| Retry backoff | Keep retry delays fixed or double them after each failure |
| Maximum retry delay (ms) | Optional cap for the computed retry delay |
| Store result as | Publish one of this node's output ports under a run-scope variable for later nodes and expressions |
| Result port | Output port selected by Store result as; blank uses `out` or the first available output |

The node context menu also provides immediate enable/disable and breakpoint actions; `F9` toggles a breakpoint on the selected node.

### Run and debug

* **Run** validates once more, starts the workflow and executes continuously.
* **Step** is the reliable entry point for debugging short workflows. From idle,
  completed, failed or cancelled state it starts a paused run and executes the
  first node. Each later click executes exactly one node and returns to
  `paused`.
* **Pause**, **Resume** and **Cancel** steer the active run. Pause and Resume are recorded in the event stream, and a node breakpoint pauses the run immediately before that node even when execution started unpaused.
* The Events tab follows the WebSocket event stream and highlights the running,
  completed and failed node on the canvas. Scrolling up pauses automatic follow
  so history stays readable; scrolling back to the bottom restores it.

The same behavior is available over the API by starting a run with
`"start_paused": true`, then calling `POST /api/v1/runs/{id}/step`.

### Screenshots and artifacts

Window lookup, focus, window capture and the **Focus target window** option on
keyboard/mouse/text nodes share one selector:

* **Window title** matches the title text;
* **Window class** matches the Win32 class;
* **Process name** matches the owning executable case-insensitively, such as `notepad.exe`;
* **Exact match** switches all supplied filters from substring to equality;
* **Visible windows only** is enabled by default and excludes hidden windows.

When several windows match, the node chooses the largest. A Find node publishes
the resolved `process` and `visible` values alongside `handle`, `title`, `class`
and `rect`, so later branches can inspect what was selected.

Use `windows.Window.Wait` when an application starts asynchronously. It polls the
same selector with `mode=appear|disappear`, configurable `wait_timeout_ms` and
`poll_interval_ms`, emits
the same window record once found, and fails with `E_TIMEOUT` when the budget is
exhausted.

Input nodes expose additional real-world timing and movement controls:

* keyboard `action=type` taps the chord and supports `hold_ms`; `press` and
  `release` keep or release a chord explicitly;
* keyboard, text and mouse nodes can enable `background` to send input
  messages to a selected window without changing focus; text also offers the
  direct `set_text` fallback and a clipboard-paste strategy that restores the
  previous clipboard contents;
* keyboard `action=type` can repeat with `repeat` and `repeat_interval_ms`;
* mouse actions choose `left`, `right` or `middle` buttons;
* `click_count` and `click_interval_ms` control repeated clicks;
* `relative=true` treats X/Y as offsets from the current cursor;
* mouse `drag` supports optional `start_x/start_y`, destination `x/y` and a
  smooth `duration_ms`;
* background mouse input uses client coordinates, sends `WM_MOUSEMOVE` and
  button messages, and restores the original system cursor position afterward;
* `double_click_interval_ms` remains as a compatibility fallback for click
  intervals.

`background` requires a title, class or process selector and cannot be combined
with `focus`. Not every application consumes these messages, so test the target
control. The clipboard strategy
uses `SendMessageTimeoutW` and restores the previous clipboard value, but some
modern packaged applications still ignore background input.

Capture nodes publish artifact metadata such as `{ "id", "name", "content_type", "size" }`.
The image bytes are transferred from the plugin process into the run's artifact
store. In Studio, open the **Events** tab: the Capture node's `node_finished`
event shows an inline image preview and an **Open** link.

Artifact metadata logged by `core.Log` is also detected: a message such as
`{{screenshot}}` produces a `log` event with an inline image preview. Artifact
metadata is found recursively inside nested objects and arrays. Every
`node_finished` event also exposes its complete outputs in a collapsed JSON
section, which is useful for OCR, template matches and plugin-defined results.

`vision.TemplateMatch` can restrict its search to `region_x`, `region_y`,
`region_width` and `region_height`, optionally fail with `fail_if_missing`, and
publishes `center_x` / `center_y`. A later Mouse node can use those values
directly, for example `"x": "{{hit.center_x}}"` and
`"y": "{{hit.center_y}}"`.

To choose a `windows.Desktop.Capture` rectangle directly with the mouse:

1. select the Capture node on the canvas;
2. click **Select screen region** in Properties;
3. Studio hides itself, captures the current desktop through the runtime and
   reopens with the screenshot;
4. drag a rectangle with the left mouse button and check the live pixel
   readout;
5. click **Apply region** to write X, Y, Width and Height into the node.

The conversion uses the original image pixels, so it remains accurate when the
preview is scaled down. A browser build cannot hide the Studio window; arrange
the target window before capture.

Use `{{screenshot.id}}` when another node needs the artifact id, and
`{{screenshot.size}}` or `{{screenshot.content_type}}` for diagnostics. The
`examples/capture-preview.json` workflow demonstrates this.

For API debugging:

```text
GET /api/v1/runs/{run_id}/artifacts
GET /api/v1/runs/{run_id}/artifacts/{artifact_id}
```

The first endpoint returns metadata; the second returns the raw bytes with the
artifact MIME type. Artifacts remain available only while the runtime retains the
run, so inspect them in the same session.

### Running external commands

`system.Command` launches a program or shell command and publishes stdout,
stderr, the exit code and PID on output ports. Its common settings are editable
directly in Properties:

| Setting | Purpose |
|---|---|
| Program or command | Executable to launch; shell syntax and Windows built-ins work when **Run through shell** is enabled |
| Arguments | Argument list; each item is passed as a separate argument |
| Working directory / Environment variables | Control the child process working directory and additional environment |
| Standard input | Optional text; supports `{{variable}}` interpolation |
| Fail on non-zero exit | Enabled by default; disable it to inspect `exit_code`, `stdout` and `stderr` downstream |
| Wait for completion | Disable to launch in the background and return only `pid` |

Stdout is also exposed as `out`, so the common **Store result as** setting can
pass command output to later nodes without custom wiring. After completion, the
Events tab expands stdout, stderr, exit code and PID inline. Node timeout and
run cancellation terminate the complete child process tree. This node requires
the `process.execute` permission; `examples/system-command.json` is a runnable
example.

### Failure recovery context

When a node executor fails, the runtime exposes `last_error` to the rest of the
run: `last_error.code`, `last_error.message`, `last_error.node_id` and
`last_error.retryable`. Failure branches can use those values in guard
expressions and downstream nodes can interpolate them in messages.

### Extensions

The **Extensions** tab presents the runtime's unified registration registry. It
lists built-in, in-process and plugin entries with their source, capability and
node-type counts, and whether the executable side is loaded. Adding a new
runtime feature through the extension registry therefore makes it visible to
the same management surface.

### Runs and events

The **Runs** tab lists the runtime's execution history with status, workflow,
node/event/artifact counts, exact started and finished timestamps, and duration.
Filter by text or status, then select **Open** on any row to load its full event
stream into the **Events** tab. Refresh the list after starting or controlling a
run; completed, failed, running and paused rows are visually distinguished.

## CLI

| Command | Purpose | Executes nodes |
|---|---|---|
| `nodara-cli validate FILE` | Validate structure, graph, node types, and configuration | No |
| `nodara-cli simulate FILE` | Show order and required permissions | No |
| `nodara-cli run FILE` | Execute a workflow | Yes |
| `nodara-cli inspect FILE` | Print workflow structure | No |
| `nodara-cli migrate FILE --out NEW` | Upgrade a legacy document | No |
| `nodara-cli plugins` | List discovered plugin manifests | No |
| `nodara-cli extensions` | List unified built-in, in-process and plugin registrations | Yes, when plugin directories are supplied |
| `nodara-cli schema --out DIR` | Export JSON Schema documents | No |
| `nodara-cli serve` | Start the full runtime service | Service |

Useful options include `--var key=value`, `--plugin-dir DIR`, `--allow
CAPABILITY`, `--allow-all`, `--audit FILE`, `--quiet`, and `--json`.

## Runtime configuration

The standalone runtime is configured through environment variables:

| Variable | Default | Purpose |
|---|---|---|
| `NODARA_RUNTIME_PORT` | `8710` | Listening port |
| `NODARA_PLUGIN_DIRS` | Empty | Additional plugin directories separated by `;` |
| `RUST_LOG` | Internal default | Rust tracing filter |

For `--require-approval`, audit files, and explicit policy allowlists, use the
source-built CLI:

```powershell
cargo run -p nodara-cli -- serve --plugin-dir plugins --require-approval --audit audit.jsonl
```

## Workflows

Workflows use `schema_version: "2.1"`, namespaced node types, and a directed
acyclic graph. Variables are referenced with `{{name}}`. An exact template
keeps the target field's JSON type, so `"{{match.x}}"` works directly in a
numeric coordinate or duration field; mixed text such as `"x={{match.x}}"`
remains a string. Studio's numeric and boolean fields expose a `{}` button that
switches between a direct value and a template expression. The complete node and
permission reference is in [nodes.md](nodes.md). A running runtime publishes the
deployment-specific schema at:

```text
http://127.0.0.1:8710/api/v1/schema/workflow
```

## Agent

Desktop Studio owns the conversational Agent lifecycle. Open the bottom **Agent** tab or expand the full workspace, configure one of several named Provider Profiles, enter a goal, choose a prompt template, baseline and execution mode, then Send. API keys are stored per Profile in Windows Credential Manager and never enter localStorage, workflow JSON or logs. Model output streams token by token; Stop cancels only the current generation. Results include node/edge diffs, diagnostics, final JSON and decision Trace, and never replace the canvas automatically.

| Mode | Behavior |
|---|---|
| Plan only | Validate and return a plan without a run action |
| Manual | Confirm the plan, start paused, then Resume from Studio |
| Partial approval | Run safe nodes automatically and wait for each dangerous/privileged approval |
| Automatic | Run and auto-approve while preserving capability decisions and audit records |

Agent output never replaces the canvas automatically. Review the final JSON and diagnostics, then Load, Validate, Run this plan or Open audit. Browser Studio shows a desktop-only message because the process/JSON pipe is owned by the Tauri shell.

The same capability is available for automation through `nodara-agent studio`, which reads one structured request from stdin and writes one structured response to stdout. Desktop Studio uses `nodara-agent studio --stream` to receive JSONL progress events.


With the runtime running:

```powershell
.\nodara-agent.exe capabilities
.\nodara-agent.exe sessions
.\nodara-agent.exe audit
```

Set `NODARA_LLM_API_KEY`, `NODARA_LLM_ENDPOINT`, and `NODARA_LLM_MODEL` to use an
OpenAI-compatible provider. For deterministic tests without a provider, the
hidden `--mock '<workflow-json>'` option supplies a canned response.

`--safe` rejects privileged or side-effecting nodes. `--allow NODE_TYPE`
restricts planning to an explicit allowlist. Runtime policy remains the
authoritative execution guard.

## Run variables

Use the toolbar **Variables…** action to review every workflow variable before a
run. Values entered there are session-only overrides: they are sent to `/runs`
without changing the workflow document. Secret values are masked, numeric and
JSON inputs are validated before Run is enabled, and each override can be
cleared independently.

## HTTP API

The public base path is `/api/v1`. Important endpoints include health, plugins,
node types, workflow schema, workflow validation, runs, event logs, agent
sessions, approvals, and audit. See
[Nodara-Core/protocol/runtime-api.md](../Nodara-Core/protocol/runtime-api.md).
Run control and approval are also available through `nodara-agent`:

```powershell
.\nodara-agent.exe control <run-id> pause
.\nodara-agent.exe control <run-id> resume
.\nodara-agent.exe control <run-id> cancel
.\nodara-agent.exe approve <session-id> <approval-id> --by tester
```

## Security

The packaged runtime uses `DefaultPolicy` and auto-approves privileged nodes.
Only run workflows you trust after reviewing `nodara-cli simulate` output.
Prefer `nodara-agent --safe` or `--allow`, and use `--require-approval` when
testing with a source-built runtime. Audit records are durable only when
`--audit FILE` is provided.

## Troubleshooting

| Symptom | Action |
|---|---|
| Runtime exits at startup | Check for a port conflict, especially 8710 |
| Studio says `runtime unreachable` | Verify `nodara-runtime.exe` is beside Studio, then check port 8710, firewall, and the debug console |
| Only 14 nodes | Verify both plugin executables are present next to their manifests |
| Unknown node type | Start the runtime with the plugin directory that defines it |
| Missing webview or application startup failure | Install WebView2 and VC++ 2015-2022 x64 Runtime |
| Agent reports no API key | Set `NODARA_LLM_API_KEY` or use `--mock` |
| Run waits for approval | Use `nodara-agent sessions`, then approve or deny it |
