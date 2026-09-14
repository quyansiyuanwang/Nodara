# Node reference

> 中文版：[nodes.zh.md](nodes.zh.md)

This is the catalogue of the node types the default build ships: five core
nodes, two system nodes, three input nodes, three window nodes, desktop capture
and two vision nodes. A deployment can add more by installing plugins — call
`GET /api/v1/node-types`, or run `nodara-cli simulate <workflow>` to see which node
types a document needs — and the published workflow schema grows with them.

Each entry lists the ports, the policy behaviour and every configuration key.
The same information is machine-readable from `GET /api/v1/node-types`, and
[`docs/schema.md`](schema.md#5-node-descriptor) explains the descriptor fields.

## How to read an entry

* **Ports** — `in`/`out` port names and their abstract value types
  (`any`, `string`, `number`, `object`, `image`, `window`). Edges connect an
  output to an input; when no port is named, `out` → `in` is assumed.
* **Policy** — a node with permissions is *privileged*. Under the default
  policy privileged nodes require an operator approval before they run; with
  `--allow-all` everything runs, and with an allowlist everything not listed is
  refused. Permissions are also what `--safe` (agent) and `--allow` (CLI) act
  on.
* **Required** — a required key must be present or validation reports `WF142`.
  A key with a default is usually optional.
* **Templates** — any string value accepts `{{variable}}` interpolation.

### Permissions and the default policy

| Permission | Nodes | Default policy |
|---|---|---|
| `input.control` | `windows.Input.Keyboard`, `windows.Input.Text`, `windows.Input.Mouse` | approval required |
| `window.control` | `windows.Window.Focus` | approval required |
| `screen.capture` | `windows.Desktop.Capture`, `windows.Window.Capture` | approval required |
| `clipboard` | `system.Clipboard` | approval required |
| `vision.analyze` | `vision.TemplateMatch`, `vision.Ocr` | approval required |
| — | `core.*`, `system.Delay`, `windows.Window.Find` | allowed |

### Summary

| Node type | Category | Ports | Permission |
|---|---|---|---|
| `core.Start` | Core | out `out` | — |
| `core.End` | Core | in `in` | — |
| `core.Log` | Core | in `in` → out `out` | — |
| `core.Calculate` | Core | in `in` → out `result` | — |
| `core.SetVariable` | Core | in `in` → out `out` | — |
| `system.Delay` | System | in `in` → out `out` | — |
| `system.Clipboard` | System | in `in` → out `out` | `clipboard` |
| `windows.Input.Keyboard` | Input | in `in` → out `out` | `input.control` |
| `windows.Input.Text` | Input | in `in` → out `out` | `input.control` |
| `windows.Input.Mouse` | Input | in `in` → out `out` | `input.control` |
| `windows.Window.Find` | Window | in `in` → out `window` | — |
| `windows.Window.Focus` | Window | in `in` → out `out` | `window.control` |
| `windows.Window.Capture` | Window | in `in` → out `artifact` | `screen.capture` |
| `windows.Desktop.Capture` | Desktop | in `in` → out `artifact` | `screen.capture` |
| `vision.TemplateMatch` | Vision | in `in` → out `match` | `vision.analyze` |
| `vision.Ocr` | Vision | in `in` → out `text` | `vision.analyze` |

## Core

### `core.Start` — Start

Entry point of the workflow. Every runnable document needs exactly one
(`WF120` when missing, `WF121` when there are several). It takes no
configuration.

* Ports: out `out` (any)
* Policy: always allowed

```json
{ "id": "start", "type": "core.Start", "label": "Start" }
```

### `core.End` — End

Terminates the workflow. A document that must terminate needs one (`WF122`).

* Ports: in `in` (any)
* Policy: always allowed

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `code` | integer | no | `0` | Process exit code reported by the runtime when the workflow finishes. |

```json
{ "id": "end", "type": "core.End", "config": { "code": 0 } }
```

### `core.Log` — Log

Writes a message to the run log and the event stream. The message is also
published on the `out` port, which makes it useful for chaining.

* Ports: in `in` (any) → out `out` (string)
* Policy: always allowed

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `message` | string | **yes** | — | Message template; supports `{{variable}}` interpolation. Example: `Hello, {{name}}!` |
| `level` | string | no | `info` | Log severity: `debug`, `info`, `warn` or `error`. |

```json
{ "id": "greet", "type": "core.Log",
  "config": { "message": "Hello, {{name}}!", "level": "info" } }
```

### `core.Calculate` — Calculate

Evaluates an arithmetic and comparison expression against the run scope and
publishes the result as a variable. Supports `+`, `-`, `*`, `/`, `%`, `^`,
comparisons and `&&`/`||`.

* Ports: in `in` (any) → out `result` (number)
* Policy: always allowed

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `expression` | string | **yes** | — | Arithmetic expression; may read other variables. Examples: `2 + 2 * 3`, `{{price}} * {{quantity}}` |
| `output_var` | string | **yes** | — | Variable the result is published under, e.g. `answer`. |

```json
{ "id": "compute", "type": "core.Calculate",
  "config": { "expression": "2 + 2 * 3", "output_var": "answer" } }
```

### `core.SetVariable` — Set Variable

Publishes an arbitrary JSON value into the run scope. Strings support
`{{variable}}` interpolation, so this is also how a workflow normalises or
renames data.

* Ports: in `in` (any) → out `out` (any)
* Policy: always allowed

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `name` | string | **yes** | — | Variable name to publish the value under. |
| `value` | any | no | — | Value to store; any JSON value is accepted, strings interpolate. |

```json
{ "id": "remember", "type": "core.SetVariable",
  "config": { "name": "greeting", "value": "Hello, {{name}}!" } }
```

## System

### `system.Delay` — Delay

Waits for a fixed duration. The wait is interruptible: cancellation is observed
while sleeping, so `cancel` is prompt even during a long delay.

* Ports: in `in` (any) → out `out` (any)
* Policy: always allowed

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `duration_ms` | integer | **yes** | `0` | How long to wait, in milliseconds. Minimum `0`. |

```json
{ "id": "wait", "type": "system.Delay", "config": { "duration_ms": 1500 } }
```

### `system.Clipboard` — Clipboard

Reads the clipboard into the run, or replaces its text.

* Ports: in `in` (any) → out `out` (string)
* Policy: **privileged** — permission `clipboard`

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `action` | string | **yes** | `read` | `read` or `write`. |
| `text` | string | when `action` is `write` | — | Text to place on the clipboard; supports `{{variable}}`. |
| `output_var` | string | no | — | Variable receiving the clipboard text when reading. |

```json
{ "id": "read_clip", "type": "system.Clipboard",
  "config": { "action": "read", "output_var": "clip" } }
```

## Input

All three input nodes are **privileged** (`input.control`) and send input to the
focused window.

### `windows.Input.Keyboard` — Keyboard

Sends a key or key chord.

* Ports: in `in` (any) → out `out` (string)
* Policy: **privileged** — permission `input.control`

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `keys` | string | **yes** | — | Key or chord; modifiers and keys are joined with `+`. Examples: `ctrl+shift+s`, `win+i`, `enter` |

```json
{ "id": "open_settings", "type": "windows.Input.Keyboard",
  "config": { "keys": "win+i" } }
```

### `windows.Input.Text` — Text

Types literal text into the focused window.

* Ports: in `in` (any) → out `out` (string)
* Policy: **privileged** — permission `input.control`

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `text` | string | **yes** | — | Literal text to type. |
| `interval_ms` | integer | no | `10` | Delay between keystrokes; `0` types as fast as the window accepts input. Minimum `0`. |

```json
{ "id": "type_note", "type": "windows.Input.Text",
  "config": { "text": "Hello from Nodara", "interval_ms": 10 } }
```

### `windows.Input.Mouse` — Mouse

Moves the cursor and synthesises mouse buttons. Absolute `x`/`y` are omitted for
clicking where the cursor already is.

* Ports: in `in` (any) → out `out` (object: `action`, `moved`, `foreground`)
* Policy: **privileged** — permission `input.control`

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `action` | string | **yes** | `click` | `move`, `click`, `double_click`, `right_click`, `middle_click`, `down` or `up`. |
| `x` | integer | no | — | Absolute screen X in pixels. |
| `y` | integer | no | — | Absolute screen Y in pixels. |

```json
{ "id": "click_ok", "type": "windows.Input.Mouse",
  "config": { "action": "click", "x": 640, "y": 480 } }
```

## Window

### `windows.Window.Find` — Find Window

Locates exactly one window and publishes a record for later nodes. It reads only
window metadata, so it carries no permission.

* Ports: in `in` (any) → out `window` (window)
* Policy: always allowed

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `title` | string | no | — | Window title to match; substring match unless `exact`. Examples: `Notepad`, `Settings` |
| `class` | string | no | — | Win32 window class name to match, e.g. `Notepad`. |
| `exact` | boolean | no | `false` | Require title and class to match exactly. |
| `foreground` | boolean | no | `false` | Use the foreground window; overrides `title` and `class`. |
| `output_var` | string | **yes** | — | Variable receiving the record: `handle`, `title`, `class`, `rect`. |

```json
{ "id": "find", "type": "windows.Window.Find",
  "config": { "title": "Notepad", "output_var": "notepad" } }
```

### `windows.Window.Focus` — Focus Window

Brings a matched window to the foreground.

* Ports: in `in` (any) → out `out` (window)
* Policy: **privileged** — permission `window.control`

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `title` | string | no | — | Window title to match; substring match unless `exact`. |
| `class` | string | no | — | Win32 window class name to match. |
| `exact` | boolean | no | `false` | Require title and class to match exactly. |
| `foreground` | boolean | no | `false` | Use the foreground window; overrides `title` and `class`. |

```json
{ "id": "focus", "type": "windows.Window.Focus",
  "config": { "title": "Notepad" } }
```

### `windows.Window.Capture` — Capture Window

Captures a window and stores it as an image artefact that later vision nodes can
read by id.

* Ports: in `in` (any) → out `artifact` (image)
* Policy: **privileged** — permission `screen.capture`

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `title` | string | no | — | Window title to match; substring match unless `exact`. |
| `class` | string | no | — | Win32 window class name to match. |
| `exact` | boolean | no | `false` | Require title and class to match exactly. |
| `foreground` | boolean | no | `false` | Use the foreground window; overrides `title` and `class`. |
| `output_var` | string | **yes** | — | Variable receiving the captured artefact metadata. |

```json
{ "id": "grab", "type": "windows.Window.Capture",
  "config": { "title": "Notepad", "output_var": "shot" } }
```

## Desktop

### `windows.Desktop.Capture` — Capture Desktop

Captures the whole primary display, or a rectangle of it.

* Ports: in `in` (any) → out `artifact` (image)
* Policy: **privileged** — permission `screen.capture`

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `x` | integer | no | `0` | Left edge of the region, in screen pixels. |
| `y` | integer | no | `0` | Top edge of the region, in screen pixels. |
| `width` | integer | no | full display width | Width of the region in pixels. Minimum `1`. |
| `height` | integer | no | full display height | Height of the region in pixels. Minimum `1`. |
| `output_var` | string | **yes** | — | Variable receiving the captured artefact metadata. |

```json
{ "id": "grab", "type": "windows.Desktop.Capture",
  "config": { "x": 0, "y": 0, "width": 800, "height": 600, "output_var": "shot" } }
```

## Vision

### `vision.TemplateMatch` — Template Match

Locates a template image inside a captured frame using normalized
cross-correlation (ZNCC), and publishes the match.

* Ports: in `in` (any) → out `match` (object)
* Policy: **privileged** — permission `vision.analyze`

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `frame` | string | **yes** | — | Artefact id from a capture node, or a path to an image file to search. |
| `template` | string | **yes** | — | Artefact id or image path of the template to find. |
| `threshold` | number | no | `0.8` | Minimum ZNCC score for a match; between `-1` and `1`. |
| `output_var` | string | **yes** | — | Variable receiving `found`, `score`, `x`, `y`, `width`, `height`. |

```json
{ "id": "find_button", "type": "vision.TemplateMatch",
  "config": { "frame": "shot", "template": "C:/images/ok.png",
              "threshold": 0.85, "output_var": "hit" } }
```

### `vision.Ocr` — OCR

Extracts text from an image through the configured OCR backend. OCR needs an
injected backend (`NODARA_OCR_COMMAND`); without one the node fails with a clear
"no backend" error rather than guessing.

* Ports: in `in` (any) → out `text` (string)
* Policy: **privileged** — permission `vision.analyze`

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `image` | string | **yes** | — | Artefact id from a capture node, or a path to an image file to read. |
| `language` | string | no | — | BCP-47 language tag passed to the backend when it supports one. Examples: `en-US`, `zh-CN` |
| `output_var` | string | **yes** | — | Variable receiving the recognised text. |

```json
{ "id": "read_text", "type": "vision.Ocr",
  "config": { "image": "shot", "language": "en-US", "output_var": "text" } }
```

## Adding node types

Install a plugin and its node types appear here — in the palette, in the agent's
capability list and in the published schema — after:

```bash
nodara-cli schema --plugin-dir path/to/plugins --out schema
```

See [node-authoring.md](../Nodara-Core/docs/node-authoring.md) for the
implementation and [schema.md](schema.md#10-writing-configuration-schemas-that-help-users)
for the configuration-schema conventions that produce good hints.
