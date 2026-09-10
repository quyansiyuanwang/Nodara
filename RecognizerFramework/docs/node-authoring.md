# Authoring a node

A capability is one type implementing `NodeExecutor`. You can stop there and
register it in-process, or serve it as a plugin — the same code, no rewrite.

## 1. Implement the executor

```rust
use rf_core::{ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use rf_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

pub struct SlugifyExecutor;

impl NodeExecutor for SlugifyExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![PortDescriptor::new("in", "In", PortKind::Input, ValueType::String)],
            outputs: vec![PortDescriptor::new("out", "Out", PortKind::Output, ValueType::String)],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Text to slugify" }
                },
                "required": ["text"],
                "additionalProperties": false
            }),
            allows_additional_config: false,
            ..NodeDescriptor::new("text.Slugify", "Slugify", "Text")
                .with_description("Converts text to a URL-safe slug")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        // `resolved_config` already has {{placeholders}} substituted.
        let text = input.require_str("text")?;
        context.check_cancelled()?;

        let slug: String = text
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();

        context.log(rf_schema::LogLevel::Info, format!("slugified {text}"));
        Ok(NodeOutput::new().with_output("out", serde_json::json!(slug)))
    }
}
```

Rules worth internalising:

* **Declare the config schema.** The Studio builds its form from it, validation
  checks required keys against it, and the agent reads it to fill in values.
* **Use `resolved_config`.** `input.config` is the raw document; the placeholder
  form is already resolved for you.
* **Check cancellation in loops.** `context.check_cancelled()?` makes `cancel`
  prompt.
* **Publish variables, don't mutate globals.** `NodeOutput::with_variable` adds
  to the run scope for later nodes and for `{{interpolation}}`.
* **Mark side effects.** Set `dangerous: true` and list `permissions` so the
  policy layer can gate the node.

## 2a. Register it in-process

```rust
use rf_core::{register_builtins, CapabilityRegistry, WorkflowEngine};
use std::sync::Arc;

let mut registry = CapabilityRegistry::new();
register_builtins(&mut registry);
registry.register(SlugifyExecutor);

let engine = WorkflowEngine::new(Arc::new(registry));
```

## 2b. Or serve it as a plugin

```rust
use std::sync::Arc;
use rf_core::CapabilityRegistry;
use rf_plugin::{serve_stdio, PluginServerInfo};

fn main() {
    rf_plugin::tracing_init();                  // logs go to stderr

    let mut registry = CapabilityRegistry::new();
    registry.register(SlugifyExecutor);

    let info = PluginServerInfo::new("com.example.text", "Text Tools", "1.0.0")
        .with_capabilities(["Text.Slugify"]);
    serve_stdio(Arc::new(registry), info).expect("serve the plugin");
}
```

Then drop a `manifest.json` next to the binary:

```json
{
  "id": "com.example.text",
  "name": "Text Tools",
  "version": "1.0.0",
  "protocol_version": "1",
  "executable": "text-tools-plugin.exe",
  "capabilities": ["Text.Slugify"],
  "permissions": [],
  "node_types": ["text.Slugify"]
}
```

## 3. Test it

`rf-testkit` removes the boilerplate:

```rust
use rf_testkit::{in_process_plugin, install_plugin, builtin_registry, WorkflowBuilder};
use rf_core::{CapabilityRegistry, RunControl, RunRequest, WorkflowEngine};
use std::sync::Arc;

# fn slugify_registry() -> CapabilityRegistry { CapabilityRegistry::new() }
let mut server = CapabilityRegistry::new();
server.register(SlugifyExecutor);

let plugin = in_process_plugin("com.example.text", server);
let mut registry = builtin_registry();
install_plugin(&mut registry, &plugin);

let workflow = WorkflowBuilder::new("wf.slug")
    .node("slug", "text.Slugify", serde_json::json!({ "text": "Hello World" }))
    .edge("start", "slug")
    .end()
    .edge("slug", "end")
    .build();

let engine = WorkflowEngine::new(Arc::new(registry));
let outcome = engine.run(RunRequest::new(workflow), &RunControl::new());
assert!(outcome.is_success());
```

## Checklist

- [ ] `descriptor()` has a stable, namespaced `node_type`
- [ ] `config_schema` declares every key and its required ones
- [ ] `permissions` and `dangerous` reflect the real side effects
- [ ] long operations call `context.check_cancelled()?`
- [ ] results are returned through ports and variables, not printed
- [ ] `manifest.json` matches what `describe` actually returns
- [ ] a test exercises the happy path and at least one failure
