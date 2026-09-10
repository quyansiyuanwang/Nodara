# Quick start

Five minutes, from an empty checkout to a running workflow, an editor and a
planning agent.

## 1. Build and run a workflow

```bash
cd RecognizerFramework
cargo build
cargo run -p rf-cli -- run ../examples/hello-world.json
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
cargo run -p rf-cli -- inspect  ../examples/hello-world.json
cargo run -p rf-cli -- simulate ../examples/branching.json

# Override a variable
cargo run -p rf-cli -- run ../examples/hello-world.json --var name=Codex
```

`simulate` tells you which nodes run, in what order, and which permissions they
will need — useful before running something that touches your desktop.

## 2. Start the runtime

The runtime is the process the editor and the agent talk to.

```bash
cargo run -p rf-cli -- serve --in-process --plugin-dir plugins
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
| plugin processes | `--plugin-dir plugins` | the runtime launches `rf-platform-plugin.exe` and `rf-vision-plugin.exe` and talks JSON-RPC to them |

The second is the architecture the project is built for; the first is convenient
for development. Both expose exactly the same node types.

## 3. Open the editor

```bash
cd ../RecognizerFramework-Studio
npm install
npm run dev
```

Open <http://localhost:4173>. The palette is populated from the runtime, so you
should see Core, System, Input, Window, Desktop and Vision categories.

Then:

1. drag **Log** onto the canvas and connect `Start → Log → End`;
2. select the Log node and set its message;
3. press **Validate** — problems appear in the drawer, from the runtime;
4. press **Run** and watch the Events tab; nodes light up as they execute.

If you install a plugin and restart the runtime, reload the page: the new node
types are simply there. Nothing in the editor needed to change.

## 4. Plan with the agent

```bash
cd ../RecognizerFramework-Agent
export RF_LLM_API_KEY=sk-...          # any OpenAI-compatible endpoint
cargo run -p rf-agent -- plan "read the clipboard and log its contents" --trace trace.jsonl
cargo run -p rf-agent -- replay trace.jsonl
```

`--safe` refuses every node that performs a side effect, and `--allow <TYPE>`
restricts the agent to an explicit node allowlist. The trace records every
decision: the goal, each model call, the runtime's validation verdict, the
guardrail result and the run outcome.

## 5. Write your own capability

Implement one trait and serve it:

```rust
use rf_core::{ExecutionContext, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use rf_schema::NodeDescriptor;

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

See [RecognizerFramework/docs/node-authoring.md](RecognizerFramework/docs/node-authoring.md)
for the full walkthrough, including the manifest and the tests.

## Where things are

| I want to… | Look at |
|-----------|---------|
| run or debug a workflow | `rf-cli validate / simulate / run / inspect` |
| drive the runtime from code | [protocol/runtime-api.md](RecognizerFramework/protocol/runtime-api.md) |
| understand the plugin wire format | [protocol/plugin-protocol.md](RecognizerFramework/protocol/plugin-protocol.md) |
| see the JSON contracts | [RecognizerFramework/schema/](RecognizerFramework/schema) |
| understand the crate layout | [RecognizerFramework/docs/architecture.md](RecognizerFramework/docs/architecture.md) |
| upgrade an old workflow | `rf-cli migrate old.json --out new.json` |
