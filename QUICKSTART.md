# Quick start

> 中文: [QUICKSTART.zh.md](QUICKSTART.zh.md)

Five minutes, from an empty checkout to a running workflow, an editor and a
planning agent.

## 1. Build and run a workflow

### Prebuilt package

For an acceptance test without a Rust or Node toolchain, extract the release
package and run from its root:

```powershell
.\nodara-cli.exe validate .\examples\hello-world.json
.\nodara-cli.exe run .\examples\hello-world.json
```

See [docs/artifacts.md](docs/artifacts.md) for the package layout and integrity
checks.

### From source

```bash
cd Nodara-Core
cargo build
cargo run -p nodara-cli -- run ../examples/hello-world.json
```

```text
run started: workflow.hello-world
     policy core.Start: allow
  -> start (core.Start)
  ok start in 0ms
     ...
     Hello, World!
     ...
run completed: 5 node(s) in 3ms
audit: 20 record(s) in 4ms wall clock
```

Note the `policy` lines: they are the runtime recording what it allowed, before
it allowed it.

Try the other commands:

```bash
# Structure and plan, without executing anything
cargo run -p nodara-cli -- inspect  ../examples/hello-world.json
cargo run -p nodara-cli -- simulate ../examples/branching.json

# Override a variable
cargo run -p nodara-cli -- run ../examples/hello-world.json --var name=Codex
```

`simulate` tells you which nodes run, in what order, and which permissions they
will need — useful before running something that touches your desktop.

## 2. Start the runtime

The runtime is the process the editor and the agent talk to.

```bash
cargo run -p nodara-cli -- serve --in-process --plugin-dir plugins
# runtime listening on http://127.0.0.1:8710/api/v1 (16 node type(s), 2 plugin(s))
```

Check it from another terminal:

```bash
curl http://127.0.0.1:8710/api/v1/health
curl http://127.0.0.1:8710/api/v1/node-types
```

Two ways to load the official capabilities:

| Mode | Command | What happens |
|------|---------|--------------|
| in process | `--in-process` | the runtime registers them directly |
| plugin processes | `--plugin-dir plugins` | the runtime launches `nodara-platform-plugin.exe` and `nodara-vision-plugin.exe` and talks JSON-RPC to them |

The second is the architecture the project is built for; the first is convenient
for development. Both expose exactly the same node types.

## 3. Open the editor

```bash
cd ../Nodara-Studio
npm install
npm run dev
```

Open <http://localhost:4173>. The palette is populated from the runtime, so you
should see Core, System, Input, Window, Desktop and Vision categories.

Then:

1. drag **Log** onto the canvas and connect `Start → Log → End`;
2. select the Log node and set its message; the workflow validates
   automatically after the edit settles;
3. the new document already contains `Start → Log → End`; check the status
   beside **Validate**, and fix any errors listed in **Problems**;
4. press **Run** and watch the Events tab; nodes light up as they execute;
5. open `examples/capture-preview.json` on Windows to see the captured PNG rendered inline under the Capture event;
6. open **Runs**, refresh, and use **Open** to reload any recent run's event stream.

If you install a plugin and restart the runtime, reload the page: the new node
types are simply there. Nothing in the editor needed to change.

### Editor content hints

A workflow file that declares `$schema` is completed by any JSON-Schema-aware
editor: node types, configuration keys, defaults, enums and hover documentation.
The repository's `examples/*.json` already point at the published schema:

```json
{
  "$schema": "../Nodara-Core/schema/workflow.schema.json"
}
```

For workflow files that do not carry the field, map them once in your editor,
for example VS Code's `settings.json`:

```json
{
  "json.schemas": [
    {
      "fileMatch": ["/examples/*.json"],
      "url": "./Nodara-Core/schema/workflow.schema.json"
    }
  ]
}
```

The schema is generated from the same descriptors the runtime uses, so the hints
can never describe a node type the runtime does not have:

```bash
cd Nodara-Core
cargo run -p nodara-cli -- schema --out schema    # regenerate what editors read
cargo run -p nodara-cli -- schema --stdout --no-capabilities   # catalog-free view
```

A running runtime serves the same document, composed for *its* installed
plugins — point `$schema` at `http://127.0.0.1:8710/api/v1/schema/workflow`, or
use *Point $schema at the runtime* in the Studio's Workflow JSON tab.

## 4. Plan with the agent

```bash
cd ../Nodara-Agent
export NODARA_LLM_API_KEY=sk-...          # any OpenAI-compatible endpoint
cargo run -p nodara-agent -- plan "read the clipboard and log its contents" --trace trace.jsonl
cargo run -p nodara-agent -- replay trace.jsonl
```

`--safe` refuses every node that performs a side effect, and `--allow <TYPE>`
restricts the agent to an explicit node allowlist. The trace records every
decision: the goal, each model call, the runtime's validation verdict, the
guardrail result and the run outcome.

## 5. Write your own capability

Implement one trait and serve it:

```rust
use nodara_core::{ExecutionContext, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use nodara_schema::NodeDescriptor;

pub struct Slugify;

impl NodeExecutor for Slugify {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor::new("text.Slugify", "Slugify", "Text")
    }

    fn execute(&self, input: NodeInput, _context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let text = input.require_str("text")?;
        Ok(NodeOutput::new().with_output("out", serde_json::json!(text.to_lowercase())))
    }
}
```

See [Nodara-Core/docs/node-authoring.md](Nodara-Core/docs/node-authoring.md)
for the full walkthrough, including the manifest and the tests.

## Where things are

| I want to… | Look at |
|-----------|---------|
| run or debug a workflow | `nodara-cli validate / simulate / run / inspect` |
| drive the runtime from code | [protocol/runtime-api.md](Nodara-Core/protocol/runtime-api.md) |
| understand the plugin wire format | [protocol/plugin-protocol.md](Nodara-Core/protocol/plugin-protocol.md) |
| see the JSON contracts | [Nodara-Core/schema/](Nodara-Core/schema) |
| understand the crate layout | [Nodara-Core/docs/architecture.md](Nodara-Core/docs/architecture.md) |
| upgrade an old workflow | `nodara-cli migrate old.json --out new.json` |
