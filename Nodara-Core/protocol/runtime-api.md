# Runtime API

Base path: `/api/v1`. Both the Studio and the agent are ordinary clients of this
API; neither links against runtime internals.

## Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/api/v1` | Service identity and version axes |
| `GET` | `/api/v1/health` | Liveness plus capability counts |
| `GET` | `/api/v1/plugins` | Installed plugins and load failures |
| `GET` | `/api/v1/node-types` | Descriptors for every known node type |
| `GET` | `/api/v1/schema/{document}` | Published JSON Schema, composed for this deployment |
| `POST` | `/api/v1/workflows/validate` | Validate a workflow document |
| `POST` | `/api/v1/runs` | Start a run |
| `GET` | `/api/v1/runs` | List runs, newest first |
| `GET` | `/api/v1/runs/{id}` | Snapshot one run |
| `POST` | `/api/v1/runs/{id}/pause` | Suspend at the next node boundary |
| `POST` | `/api/v1/runs/{id}/resume` | Resume |
| `POST` | `/api/v1/runs/{id}/step` | Allow exactly one more node |
| `POST` | `/api/v1/runs/{id}/cancel` | Cancel, including in-flight plugin calls |
| `WS` | `/api/v1/runs/{id}/events` | Replay then stream execution events |
| `GET` | `/api/v1/runs/{id}/event-log` | The same event sequence over REST |
| `GET` | `/api/v1/agent/sessions` | Agent sessions and pending approvals |
| `POST` | `/api/v1/agent/sessions` | Create a session |
| `GET` | `/api/v1/agent/sessions/{id}` | One session |
| `POST` | `/api/v1/agent/sessions/{id}/messages` | Append a conversation turn |
| `POST` | `/api/v1/agent/sessions/{id}/plan` | Publish a plan preview |
| `POST` | `/api/v1/agent/sessions/{id}/status` | Set the session status |
| `POST` | `/api/v1/agent/sessions/{id}/approvals/{approval_id}` | Decide an approval |
| `GET` | `/api/v1/agent/approvals` | Every approval waiting on an operator |
| `GET` | `/api/v1/audit` | Audit records, filterable by run |

## Schemas and editor content hints

`GET /api/v1/schema/{document}` serves the same documents `nodara-cli schema`
publishes: `workflow`, `plugin-manifest`, `node-descriptor`, `execution-event`,
`agent-session` and `agent-tool-call`. The bare name and the published file name
both resolve, so `/schema/workflow` and `/schema/workflow.schema.json` are the
same document.

The workflow schema is **composed for the deployment**: every installed node
type contributes its `config_schema`, so the `type` field is an enum of the node
types this runtime can actually run and `config` is described per type.

```jsonc
{
  "$schema": "http://127.0.0.1:8710/api/v1/schema/workflow",
  "schema_version": "2.0",
  "id": "workflow.example",
  "nodes": [
    // `type` completes from the catalog, and the description of each value
    // comes from the node descriptor.
    { "id": "log", "type": "core.Log", "config": { "message": "hello", "level": "info" } }
    // `config` completes only the keys `core.Log` declares, with their
    // descriptions, defaults, enums and bounds.
  ]
}
```

Any editor that understands JSON Schema (VS Code, JetBrains IDEs, Neovim with
`jsonls`) turns that into completion, hover documentation and inline
diagnostics. A static equivalent ships in
[`schema/workflow.schema.json`](../schema/workflow.schema.json) and is what
`examples/*.json` reference; regenerate it with:

```bash
nodara-cli schema --out schema                          # built-ins + official capabilities
nodara-cli schema --plugin-dir path/to/plugins --out schema
nodara-cli schema --stdout --no-capabilities            # the plain, catalog-free schema
```

## Discover, then render

The Studio never hardcodes node types. At start-up it calls
`GET /api/v1/node-types`, and for every descriptor:

1. adds an entry to the palette grouped by `category`;
2. builds a configuration form from `config_schema` (JSON Schema);
3. shows a warning badge when `dangerous` is `true`;
4. filters node types by the `capabilities` the operator has enabled.

Installing a new plugin therefore changes the UI without a frontend rebuild.

## Edge outcomes

Edges default to `branch: "always"`. A `"success"` edge is only eligible after
the source node finishes successfully; a `"failure"` edge is only eligible after
the source executor returns an error. When a failure edge is active it handles
the error and activates the recovery path without requiring the node-wide
`continue_on_error` flag. Guard conditions are evaluated after the branch match,
so a failure path can still be narrowed with its own expression.

## Validation

`POST /workflows/validate`

```json
{
  "workflow": { "schema_version": "2.0", "id": "...", "nodes": [], "edges": [] },
  "options": { "reject_cycles": true, "require_end": true }
}
```

Response: a `ValidationReport` — a flat list of diagnostics with a stable `code`,
a `severity`, a JSON-pointer `path`, and an optional `hint`. Validation never
stops at the first problem, so an editor can underline everything at once and an
agent can repair everything in one pass.

Diagnostic codes:

| Code | Severity | Meaning |
|------|----------|---------|
| `WF100` | error | unsupported `schema_version` |
| `WF101` | error | empty workflow id |
| `WF102` | error | empty node id |
| `WF103` | error | duplicate node id |
| `WF104` | error | empty node type |
| `WF110` | error | empty edge id |
| `WF111` | error | duplicate edge id |
| `WF112` | error | edge source does not exist |
| `WF113` | error | edge target does not exist |
| `WF114` | warning | self-loop |
| `WF120` | error | no `core.Start` |
| `WF121` | error | multiple `core.Start` nodes |
| `WF122` | error | no `core.End` |
| `WF130` | error | cycle |
| `WF131` | warning | node unreachable from the entry point |
| `WF132` | warning | node with no outgoing edge |
| `WF140` | error | node type is not installed |
| `WF141` | error | config is not an object |
| `WF142` | error | required config key missing |
| `WF143` | warning | unknown config key |
| `WF144` | error | `result_port` is not declared by the node descriptor |
| `WF145` | error | `result_var` is empty |
| `WF150` | warning | `{{variable}}` is undeclared |

## Running

`POST /runs` returns `202 Accepted` with a snapshot:

```json
{
  "id": "5f0c...",
  "workflow_id": "workflow.hello-world",
  "status": "pending",
  "started_at_ms": 1736000000000,
  "nodes_executed": 0,
  "variables": {},
  "event_count": 0
}
```

`status` is one of `pending`, `running`, `paused`, `completed`, `failed`,
`cancelled`.

By default the runtime validates before starting and answers `422` with
`E_WORKFLOW_INVALID` plus the full report. Pass `"validate": false` to skip that
and let the engine fail at the offending node instead.

`variables` in the request body override the workflow's own defaults.

## Streaming events

`WS /api/v1/runs/{id}/events` first replays every event recorded so far, then
streams live ones. Because the replay is included, a client that connects late —
or reconnects after a drop — never misses the beginning of a run.

Each frame is an `EventEnvelope`:

```json
{
  "run_id": "5f0c...",
  "seq": 7,
  "timestamp_ms": 1736000001234,
  "event": { "type": "node_finished", "node_id": "greet", "outputs": {}, "duration_ms": 3 }
}
```

`seq` is assigned by the runtime and is strictly increasing within a run, so a
client can detect gaps after a reconnect.

Event types come from
[`execution-event.schema.json`](../schema/execution-event.schema.json):
`run_started`, `node_started`, `node_progress`, `node_finished`, `node_failed`,
`log`, `run_paused`, `run_resumed`, `run_cancelled`, `run_completed`,
`run_failed`, `capability_decision`.

`capability_decision` is emitted before every node executes. It is the observable
side of the policy layer: a UI can show which capability was asked for, and
whether it was allowed, denied, or required approval.

## Errors

Failures use a structured body:

```json
{ "code": "E_RUN_NOT_FOUND", "message": "no run with id `abc`" }
```

| Status | When |
|--------|------|
| `400` | malformed request body |
| `404` | unknown run |
| `409` | control command sent to a finished run |
| `422` | workflow failed validation before starting |

## Agent sessions, and why approvals live there

The architecture document requires that the Studio and the agent never call each
other, and that they cooperate through the runtime's session and event API. The
runtime stores sessions as **data**: it never builds a prompt, never calls a
model and never plans. The agent process produces all of that and publishes it
here; the Studio reads it back.

That is also how approvals work, and it is the reason they are enforced rather
than decorative:

```text
agent  --POST /agent/sessions-------->  session (draft)
agent  --POST /agent/sessions/{id}/plan> plan preview + validation report
agent  --POST /runs {session_id}------>  run bound to the session
                                          |
                       gated node reached |
                                          v
   policy says require_approval -> runtime raises an ApprovalRequest
                                   and BLOCKS the run thread
                                          |
studio --POST .../approvals/{aid}------>  decision released to the run
```

Three properties follow, and all three are covered by tests:

* a run bound to no session is **refused** when approval is required, because an
  unanswered request must never silently authorise a side effect;
* an approval that times out (`--approval-timeout`) is denied, not granted;
* the decision, who took it, and when, are recorded on the session and in the
  audit log.

Run `nodara-cli serve --require-approval` to make this the live behaviour; the
default (`auto_approve`) is the documented phase-one mode for unattended runs,
and it still records `capability_decision` events for every gated node.

### Reading events without a WebSocket

`WS /runs/{id}/events` is the stream for the Studio. `GET /runs/{id}/event-log`
returns the identical sequence as JSON, which is what the agent polls: a batch
command that already speaks REST does not need a WebSocket client to observe a
run. Both read the same runtime-assigned `seq`, so neither can miss an event the
other saw.

## The audit log

`GET /api/v1/audit` returns the durable account of what the runtime evaluated,
allowed, refused and recorded. Query parameters:

| Parameter | Meaning |
|-----------|---------|
| `run_id` | Restrict to one run |
| `limit` | Return at most this many records (newest kept) |

Each record carries a monotonic `seq`, a timestamp, a `category`
(`run_started`, `node_finished`, `capability_evaluated`, `approval`, `log`, ...),
and — for capability records — the `capability` and the `decision`.

The event stream and the audit log are not the same thing, and both are needed:
events are for live observation and a slow subscriber may fall behind, whereas
audit records are what an operator reviews afterwards. `--audit <FILE>` makes
them durable as JSON Lines; without it they are kept in memory for the life of
the process.
