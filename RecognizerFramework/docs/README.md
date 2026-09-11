# RecognizerFramework documentation

Start here to understand the core repository.

## The detailed guides

The cross-repository guides live in the integration workspace; each exists in
English and Chinese (`name.md` / `name.zh.md`):

* [`../../docs/project.md`](../../docs/project.md) — the whole project:
  architecture, execution model, plugins, runtime, Studio, agent, security.
* [`../../docs/schema.md`](../../docs/schema.md) — every JSON Schema document,
  field by field, including how the workflow schema produces content hints.
* [`../../docs/nodes.md`](../../docs/nodes.md) — the node catalogue: ports,
  permissions and every configuration key.

## Orientation

* [`architecture.md`](architecture.md) — how the crates fit together and why the
  dependency arrows point the way they do.
* [`node-authoring.md`](node-authoring.md) — how to add a capability, either
  in-process or as a plugin.
* [`../protocol/`](../protocol) — the wire contracts: workflow format, plugin
  protocol, HTTP API.
* [`../schema/`](../schema) — generated JSON Schema documents.

## By role

**Using the framework**

1. Read the root [`README.md`](../../README.md) for the product overview.
2. Follow [`../../QUICKSTART.md`](../../QUICKSTART.md) to run a workflow.
3. Browse [`../../examples/`](../../examples) for ready-to-run documents.

**Writing a capability**

1. [`node-authoring.md`](node-authoring.md)
2. [`../protocol/plugin-protocol.md`](../protocol/plugin-protocol.md)
3. [`../plugins/README.md`](../plugins/README.md)

**Building a host (Studio, agent, embedded tool)**

1. [`architecture.md`](architecture.md)
2. [`../protocol/runtime-api.md`](../protocol/runtime-api.md)
3. `cargo run -p rf-cli -- serve` and call `GET /api/v1/node-types`

## The one-sentence version

Workflow documents, plugin manifests, node descriptors and execution events are
plain JSON with published schemas; everything else is an implementation detail
behind them.
