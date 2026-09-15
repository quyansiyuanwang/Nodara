# Examples

Every file here is a workflow document in the current format
(`schema_version: "2.0"`). The shape is defined by the generated
[workflow schema](../Nodara-Core/schema/workflow.schema.json).

| File | What it shows |
|------|----------------|
| `hello-world.json` | Variables, `{{interpolation}}`, `core.Calculate` |
| `delayed-log.json` | `system.Delay`, and a run you can cancel mid-flight |
| `branching.json` | Edge guards (`condition`) pruning branches |
| `result-mapping.json` | Common `result_var` / `result_port` output mapping on a built-in node |
| `failure-branch.json` | A `failure` edge recovering from a node error without `continue_on_error` |
| `capture-preview.json` | Captures the desktop and previews the PNG artifact in Studio |
| `window-find.json` | A plugin-provided node type (`windows.Window.Find`) |
| `legacy/v1-hello-world.json` | A pre-v2 document, kept as a migration fixture |

## Running them

All commands are run from `Nodara-Core/`.

```bash
# Check the document is structurally sound and every node type is installed
cargo run -p nodara-cli -- validate ../examples/hello-world.json

# Show the plan without executing anything
cargo run -p nodara-cli -- simulate ../examples/branching.json

# Execute
cargo run -p nodara-cli -- run ../examples/hello-world.json

# Override a variable
cargo run -p nodara-cli -- run ../examples/hello-world.json --var name=Codex
```

The `window-find.json` example uses a node type provided by the official
platform plugin. Either let the runtime launch it:

```bash
cargo run -p nodara-cli -- run ../examples/window-find.json --plugin-dir plugins
```

or register the same capabilities in-process:

```bash
cargo run -p nodara-cli -- run ../examples/window-find.json --in-process
```

## Migrating a legacy document

```bash
cargo run -p nodara-cli -- migrate ../examples/legacy/v1-hello-world.json
cargo run -p nodara-cli -- migrate ../examples/legacy/v1-hello-world.json --out upgraded.json
```

Migration rewrites node kinds to the namespaced convention
(`System.Delay` to `system.Delay`), converts `from`/`to` edges to
`source`/`target`, and translates configuration keys that changed meaning
(`seconds` to `duration_ms`).

## Authoring your own

Start from `hello-world.json`, then:

1. `nodara-cli simulate` to confirm the plan and the permissions it will need;
2. `nodara-cli validate` to confirm the document is valid;
3. `nodara-cli run` with `--allow` (or `--allow-all`) to execute.

The Studio writes exactly this format, so anything created there can be dropped
into this directory and run from the command line unchanged.
