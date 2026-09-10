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
| `POST` | `/api/v1/workflows/validate` | Validate a workflow document |
| `POST` | `/api/v1/runs` | Start a run |
| `GET` | `/api/v1/runs` | List runs, newest first |
| `GET` | `/api/v1/runs/{id}` | Snapshot one run |
| `POST` | `/api/v1/runs/{id}/pause` | Suspend at the next node boundary |
| `POST` | `/api/v1/runs/{id}/resume` | Resume |
| `POST` | `/api/v1/runs/{id}/step` | Allow exactly one more node |
| `POST` | `/api/v1/runs/{id}/cancel` | Cancel, including in-flight plugin calls |
| `WS` | `/api/v1/runs/{id}/events` | Replay then stream execution events |

## Discover, then render

The Studio never hardcodes node types. At start-up it calls
`GET /api/v1/node-types`, and for every descriptor:

1. adds an entry to the palette grouped by `category`;
2. builds a configuration form from `config_schema` (JSON Schema);
3. shows a warning badge when `dangerous` is `true`;
4. filters node types by the `capabilities` the operator has enabled.

Installing a new plugin therefore changes the UI without a frontend rebuild.

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
