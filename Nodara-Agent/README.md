# Nodara-Core Agent

Turns a natural-language goal into a workflow and operates it. It is a **client**
of the runtime, not a component inside it.

## The dependency rule

```text
Nodara-Agent
    └── depends on Runtime Protocol / JSON Schema only
```

Concretely, this repository depends on exactly one crate from the core repo —
`nodara-schema`, the published contract — and talks to everything else over HTTP.
It has no access to the engine, the registry or the plugin host.

That is not a style preference. It is what makes the permission model real: the
agent physically cannot execute a capability without the runtime's policy layer
seeing the call first. The architecture document's rule that "permission must not
depend on the prompt" is enforced by the crate graph.

## What it does

```text
discover capabilities ──▶ select tools ──▶ plan ──▶ guardrails ──▶ publish preview
       ▲                                                              │
       │                                                              ▼
  re-plan from the failure              run (bound to the session)
       ▲                                                              │
       │                                                              ▼
       └──────── observe events (GET .../event-log) ──── report ──────┘
```

* **Tool selection.** Before prompting, the catalogue is narrowed to the node
  types that could plausibly serve the goal, so the prompt stays small and the
  model is not tempted into an irrelevant capability. Selection only removes
  candidates; it never widens authority.
* **The session is published first.** The Studio watches it, and a gated node
  later has somewhere to ask for approval — which is why the run is started
  *bound* to the session rather than attached afterwards.
* **Re-planning is bounded and selective.** A run that failed for a reason the
  model could fix (a bad configuration, a node that errored) becomes another
  planning turn with the failure as feedback. A run the runtime *refused* —
  policy, cancellation, a plugin crash — is not re-planned: no amount of
  re-planning changes the runtime's answer.
* **Observation uses the same events as the Studio.** The agent polls
  `GET /runs/{id}/event-log`, which returns the identical sequence the Studio
  streams over its WebSocket.

The planner is a loop rather than a single call: it drafts, asks the runtime to
validate, feeds the runtime's own diagnostics back to the model, and retries
until the document is accepted or the repair budget runs out. Validation is done
by the runtime, so the agent cannot talk itself into a workflow the runtime would
reject.

## Quick start

Start a runtime (from `Nodara-Core/`):

```bash
cargo run -p nodara-cli -- serve --in-process --plugin-dir plugins
```

See what the agent is allowed to plan with:

```bash
cargo run -p nodara-agent -- capabilities
```

Plan a workflow (needs `NODARA_LLM_API_KEY`, or use `--mock` for a canned reply):

```bash
export NODARA_LLM_API_KEY=sk-...
cargo run -p nodara-agent -- plan "open Notepad and type a greeting" --trace trace.jsonl

# Offline, with a scripted model reply:
cargo run -p nodara-agent -- plan "log hello" \
  --mock '{"schema_version":"2.1","id":"wf.hi","nodes":[{"id":"start","type":"core.Start"},{"id":"log","type":"core.Log","config":{"message":"hello"}},{"id":"end","type":"core.End"}],"edges":[{"id":"e1","kind":"control","source":"start","target":"log"},{"id":"e2","kind":"control","source":"log","target":"end"}]}'
```

Plan and run:

```bash
cargo run -p nodara-agent -- run "write the clipboard contents to the log" \
  --variables '{"expected":"hello"}' --timeout 120
```

Print the Markdown execution report:

```bash
cargo run -p nodara-agent -- --report run "log a greeting"
```

### Studio transport

Desktop Studio starts the hidden `studio --stream` subcommand with one JSON request on
stdin and consumes one JSON response from stdout. The request includes the goal,
optional base workflow, existing session id, provider settings and execution
mode. The response includes the final workflow, run snapshot, report, trace,
token count and session id. The process itself is short-lived; the conversation
and runtime session persist across turns.

```bash
echo '{"goal":"log hello","mode":"forbidden","provider":{"endpoint":"...","model":"..."}}' \
  | nodara-agent --runtime http://127.0.0.1:8710 studio

With `--stream`, stdout is JSONL: model deltas, validation, repair, plan, run
and terminal events. Without it, the original single JSON response remains
available for automation.
```

The desktop modes map to runtime approval as follows:

| Studio mode | Runtime request |
|---|---|
| Plan only | No run request |
| Manual | `start_paused: true`, `approval: "session"` and an immediate paused result |
| Partial approval | `start_paused: false`, `approval: "session"` |
| Automatic | `start_paused: false`, `approval: "auto"` with audit retained |
### Watching and steering from a terminal

When the runtime is started with `--require-approval`, a gated node blocks until
an operator answers. These commands are the terminal equivalent of the Studio's
Agent panel:

```bash
# What is running, and what is waiting on a human?
cargo run -p nodara-agent -- sessions

# Release or refuse the run that is blocked.
cargo run -p nodara-agent -- approve <session-id> <approval-id>
cargo run -p nodara-agent -- approve <session-id> <approval-id> --deny --by alice

# Pause, resume or stop a run.
cargo run -p nodara-agent -- control <run-id> pause
cargo run -p nodara-agent -- control <run-id> resume
cargo run -p nodara-agent -- control <run-id> cancel

# What did the runtime allow, refuse and record?
cargo run -p nodara-agent -- audit
cargo run -p nodara-agent -- audit --run <run-id>
```

### Modifying an existing workflow

With `--from`, the model is asked to *modify* a document rather than author one,
and the prompt requires it to keep node ids and positions it was not asked to
change:

```bash
cargo run -p nodara-agent -- plan "add a delay of two seconds before the log" \
  --from examples/hello-world.json --out examples/hello-world.json
```

### Explaining a workflow or a failed run

The plan lists "解释节点和执行错误" among the agent's jobs. `explain` sends the
workflow, the runtime's own validation diagnostics and — when a run is given —
the run snapshot and its event log, and asks for prose rather than JSON:

```bash
cargo run -p nodara-agent -- explain examples/window-find.json
cargo run -p nodara-agent -- explain examples/window-find.json --run <run-id>
cargo run -p nodara-agent -- explain --run <run-id>
```

Replay a recorded session:

```bash
cargo run -p nodara-agent -- replay trace.jsonl
```

## Guardrails

| Flag | Effect |
|------|--------|
| `--safe` | refuse every node that is marked dangerous or declares a permission |
| `--allow <NODE_TYPE>` | restrict the agent to an explicit node allowlist |
| `--trace <FILE>` | record every decision as JSON Lines |

Budgets — model steps, tokens and wall-clock time — are configured through
`AgentConfig` (see `policy::Budget`) and are enforced before each call, so a
runaway loop stops with a clear reason instead of burning a quota.

These guardrails are the agent's *own* limits. The authoritative check is still
the runtime's policy layer, which evaluates every capability call and writes the
decision to the audit log.

## Provider configuration

| Variable | Meaning | Default |
|----------|---------|---------|
| `NODARA_LLM_ENDPOINT` | Chat-completions URL | `https://api.openai.com/v1/chat/completions` |
| `NODARA_LLM_MODEL` | Model name | `gpt-4o-mini` |
| `NODARA_LLM_API_KEY` | Credential (or `OPENAI_API_KEY`) | — |
| `NODARA_RUNTIME_URL` | Runtime base URL | `http://127.0.0.1:8710` |

Any server that speaks the OpenAI chat-completions shape works — a local
`llama.cpp` or vLLM instance included — because the wire format is the only thing
the provider assumes.

## Layout

```text
crates/nodara-agent/src/
├── lib.rs              re-exports and the dependency rule
├── provider.rs         LlmProvider trait, OpenAI-compatible adapter, mock
├── prompt.rs           system prompt built from the runtime's node catalogue
├── selector.rs         narrows the catalogue to the tools a goal needs
├── planner.rs          draft → validate → repair loop
├── policy.rs           budgets and node-type guardrails
├── runtime_client.rs   the HTTP client (the only route into a system)
├── audit.rs            JSON Lines decision trace and replay
├── report.rs           the Markdown execution report
├── agent.rs            session composition
└── bin/nodara-agent.rs     the command line
```

## Testing

```bash
cargo test
```

The tests cover the repair loop (including that diagnostics reach the model),
guardrail refusal, budget exhaustion, trace round-tripping, tool selection, the
transport error a caller sees when the runtime is down, and — against a scripted
runtime speaking real HTTP — session publication, plan previews, run observation,
re-planning after a fixable failure, refusing to re-plan after a policy refusal,
and pausing, resuming and cancelling a run.
