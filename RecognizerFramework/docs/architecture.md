# Architecture

## Dependency direction

```text
              rf-schema          contracts only; no I/O beyond reading a file
                  |
              rf-core            engine, executor SDK, policy, audit
                  |
              rf-plugin          JSON-RPC, transports, plugin host
                  |
              rf-runtime         HTTP + WebSocket API, run manager
                  |
              rf-cli             command line host

  rf-platform  \  capabilities implemented with rf-core, served as plugins
  rf-vision    /  by rf-plugin
```

Arrows point from a consumer to what it consumes. The important properties:

* `rf-core` **never** depends on `rf-runtime`, `rf-plugin`, `rf-cli` or any
  capability crate;
* `rf-runtime` never depends on `rf-platform` or `rf-vision`. The official
  capabilities reach it the same way a third-party plugin does — through
  `rf-plugin` — and the CLI decides whether to launch them as processes or
  register them in-process;
* the Studio and the agent are not crates in this repository at all. They are
  HTTP clients.

## Crates

| Crate | Responsibility | Depends on |
|-------|----------------|------------|
| `rf-schema` | Workflow, manifest, descriptor and event types; validation; graph algorithms; migration; JSON Schema generation | serde, schemars |
| `rf-core` | `NodeExecutor` SDK, `CapabilityRegistry`, `WorkflowEngine`, run control, policy, audit, events, built-in nodes | `rf-schema` |
| `rf-plugin` | JSON-RPC 2.0, stdio and in-process transports, plugin client, plugin server, discovery, host | `rf-schema`, `rf-core` |
| `rf-runtime` | Runtime composition, run manager, HTTP/WebSocket API | `rf-schema`, `rf-core`, `rf-plugin` |
| `rf-platform` | Windows input, windows, capture, clipboard | `rf-core`, `rf-schema`, `rf-plugin` (for its binary) |
| `rf-vision` | Template matching, pluggable OCR | `rf-core`, `rf-schema`, `rf-plugin` (for its binary) |
| `rf-cli` | `validate`, `run`, `simulate`, `inspect`, `migrate`, `plugins`, `schema`, `serve` | everything |
| `rf-testkit` | Workflow builder, recording/failing executors, in-process plugin harness, static node index | `rf-schema`, `rf-core`, `rf-plugin` |

## The three public interfaces

The architecture document names three interfaces. They map onto the code
directly:

**1. Rust SDK** — `rf_core::NodeExecutor`:

```rust
pub trait NodeExecutor: Send + Sync {
    fn descriptor(&self) -> NodeDescriptor;
    fn execute(
        &self,
        input: NodeInput,
        context: &mut ExecutionContext,
    ) -> Result<NodeOutput, NodeError>;
}
```

It is synchronous on purpose. Each run owns a dedicated thread, so a node may
block on a plugin round-trip, a Win32 call or a sleep without stalling an async
runtime. That is also why the engine needs no `async` colouring to coordinate
external processes.

**2. JSON Schema** — generated from the Rust types by `rf-cli schema`, so the
published documents and the code cannot drift. The workflow schema is *composed*
with the installed node catalog: `rf_schema::workflow_schema_for` folds each
descriptor's `config_schema` in under an `if`/`then` branch keyed on `type`, and
the runtime serves the result at `GET /api/v1/schema/workflow`. A workflow file
that declares that URL as its `$schema` therefore completes node types,
configuration keys, defaults and enums in any JSON-Schema-aware editor, which is
the experience the original single-process implementation got from generating
one schema from every node model.

**3. Runtime protocol** — JSON-RPC over stdio between runtime and plugin, and
HTTP/WebSocket between clients and runtime.

## Execution model

```text
RunManager.start
   |
   +--> WorkflowEngine.run  (dedicated OS thread)
          |
          +--> validate (capability-aware)
          +--> topological order + branch activation
          +--> for each activated node:
          |      control.await_permission()   <- pause / step / cancel
          |      policy.decide()              -> event + audit
          |      executor.execute()           -> local or plugin
          |      publish NodeFinished / activate guarded edges
          |
          +--> RunCompleted | RunFailed | RunCancelled
```

Two properties fall out of this shape:

* **Pause is deterministic.** The engine checks in at node boundaries, so a run
  always suspends between nodes, never mid-node. Cancellation is stronger:
  `RunControl` is visible to executors, and the long-running built-ins
  (`system.Delay`) poll it, so cancel is prompt even inside a node.
* **Branch pruning is data-driven.** An outgoing edge with no `condition` is
  always taken; one with a `condition` is taken when the expression evaluates
  truthy. Nodes that are never activated are never executed, which is what makes
  a single graph express both branches without a dedicated branch node.

## Policy and audit

Permission is enforced in the runtime, never in a prompt:

```text
engine --> ExecutionContext.authorize(node_type, permissions, dangerous, config)
                |
                +--> CapabilityPolicy.decide() -> Allow | Deny | RequireApproval
                |         RequireApproval --> ApprovalHandler.approve()
                |
                +--> event: capability_decision
                +--> audit: capability evaluated / approval granted or refused
```

The default policy allows safe nodes and requires approval for nodes that declare
permissions or are marked `dangerous` — exactly the list in the architecture
document: input control, file deletion, shell execution, network access,
credential access, browser login, system settings.

Because the decision is a data structure and every decision is audited, an agent
cannot widen its own authority by describing its intentions.

## Why the plugin boundary is real

`PluginsExecutor` (in `rf-plugin`) implements the *same* `NodeExecutor` trait as
the in-process built-ins, but forwards to a `PluginClient`. The engine cannot tell
the difference, and the test suite proves both paths:

* `crates/rf-plugin/tests/in_process_plugin.rs` drives a real `PluginServer`
  through `InProcessTransport` and executes it with the real engine;
* `crates/rf-platform/tests/plugin_process.rs` launches the real
  `rf-platform-plugin` binary, performs the handshake, describes its node types
  and shuts it down.

That is also why `rf-platform` and `rf-vision` build *two* artefacts from one
source: a library an embedded host can register directly, and a binary the
runtime can launch.

## Events

One event stream serves four consumers:

| Consumer | How it reads events |
|----------|---------------------|
| CLI | prints them live during `run` |
| Studio | `WS /api/v1/runs/{id}/events` |
| Agent | same WebSocket, or the run snapshot |
| Audit | the engine writes audit records alongside events |

Events are sequenced by the runtime before fan-out, so a reconnecting client can
detect a gap.
