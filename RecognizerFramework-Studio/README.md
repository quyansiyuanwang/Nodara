# RecognizerFramework Studio

The visual editor for RecognizerFramework. It is a pure client of the runtime's
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

Start the runtime first (from `RecognizerFramework/`):

```bash
cargo run -p rf-cli -- serve --in-process --plugin-dir plugins
```

Then, from this directory:

```bash
npm install
npm run dev          # http://localhost:4173
```

Vite proxies `/api` to `http://127.0.0.1:8710`, so there is no CORS setup in
development. Point it elsewhere with `RF_RUNTIME_URL`:

```bash
RF_RUNTIME_URL=http://192.168.1.10:8710 npm run dev
```

## Building

```bash
npm run build        # type-check, then bundle into dist/
```

## Desktop shell

`src-tauri/` is a Tauri v2 shell around the same web app:

```bash
npm run tauri dev
npm run tauri build
```

The shell adds nothing to the editor logic — it exists so the Studio can be
shipped as a desktop application instead of a browser tab.

## Layout

```text
src/
├── main.ts                composition: connect, render, execute
├── styles.css             design tokens and layout
├── runtime/
│   ├── types.ts           the wire contract, mirrored from rf-schema
│   └── client.ts          the only module that knows about HTTP and WebSocket
├── model/
│   └── workflow.ts        the document model and local sanity checks
├── schema/
│   └── form.ts            JSON Schema -> form fields
└── ui/
    ├── palette.ts         nodes discovered at runtime
    ├── canvas.ts          SVG graph editor: drag, connect, select, delete
    ├── inspector.ts       schema-driven configuration forms
    └── event-log.ts       the runtime event stream
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
* the **Events** tab streams the same events the CLI prints, and highlights the
  running, finished and failed nodes on the canvas.
