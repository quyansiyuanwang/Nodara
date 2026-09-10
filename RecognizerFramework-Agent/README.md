# RecognizerFramework Agent

Turns a natural-language goal into a workflow and operates it. It is a **client**
of the runtime, not a component inside it.

## The dependency rule

```text
RecognizerFramework-Agent
    └── depends on Runtime Protocol / JSON Schema only
```

Concretely, this repository depends on exactly one crate from the core repo —
`rf-schema`, the published contract — and talks to everything else over HTTP.
It has no access to the engine, the registry or the plugin host.

That is not a style preference. It is what makes the permission model real: the
agent physically cannot execute a capability without the runtime's policy layer
seeing the call first. The architecture document's rule that "permission must not
depend on the prompt" is enforced by the crate graph.

## What it does

```text
operator goal
     │
     ▼
Prompt builder ── node catalogue fetched from GET /node-types
     │
     ▼
Planner ⇄ LLM provider        draft → validate → repair → validate …
     │
     ▼
Guardrails                    node allowlist, refuse dangerous/privileged nodes
     │
     ▼
Runtime tool client           POST /runs, GET /runs/{id}, pause/resume/cancel
     │
     ▼
Audit trace                   JSON Lines, replayable
```

The planner is a loop rather than a single call: it drafts, asks the runtime to
validate, feeds the runtime's own diagnostics back to the model, and retries
until the document is accepted or the repair budget runs out. Validation is done
by the runtime, so the agent cannot talk itself into a workflow the runtime would
reject.

## Quick start

Start a runtime (from `RecognizerFramework/`):

```bash
cargo run -p rf-cli -- serve --in-process --plugin-dir plugins
```

See what the agent is allowed to plan with:

```bash
cargo run -p rf-agent -- capabilities
```

Plan a workflow (needs `RF_LLM_API_KEY`, or use `--mock` for a canned reply):

```bash
export RF_LLM_API_KEY=sk-...
cargo run -p rf-agent -- plan "open Notepad and type a greeting" --trace trace.jsonl

# Offline, with a scripted model reply:
cargo run -p rf-agent -- plan "log hello" \
  --mock '{"schema_version":"2.0","id":"wf.hi","nodes":[{"id":"start","type":"core.Start"},{"id":"log","type":"core.Log","config":{"message":"hello"}},{"id":"end","type":"core.End"}],"edges":[{"id":"e1","source":"start","target":"log"},{"id":"e2","source":"log","target":"end"}]}'
```

Plan and run:

```bash
cargo run -p rf-agent -- run "write the clipboard contents to the log" \
  --variables '{"expected":"hello"}' --timeout 120
```

Replay a recorded session:

```bash
cargo run -p rf-agent -- replay trace.jsonl
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
| `RF_LLM_ENDPOINT` | Chat-completions URL | `https://api.openai.com/v1/chat/completions` |
| `RF_LLM_MODEL` | Model name | `gpt-4o-mini` |
| `RF_LLM_API_KEY` | Credential (or `OPENAI_API_KEY`) | — |
| `RF_RUNTIME_URL` | Runtime base URL | `http://127.0.0.1:8710` |

Any server that speaks the OpenAI chat-completions shape works — a local
`llama.cpp` or vLLM instance included — because the wire format is the only thing
the provider assumes.

## Layout

```text
crates/rf-agent/src/
├── lib.rs              re-exports and the dependency rule
├── provider.rs         LlmProvider trait, OpenAI-compatible adapter, mock
├── prompt.rs           system prompt built from the runtime's node catalogue
├── planner.rs          draft → validate → repair loop
├── policy.rs           budgets and node-type guardrails
├── runtime_client.rs   the HTTP client (the only route into a system)
├── audit.rs            JSON Lines decision trace and replay
├── agent.rs            session composition
└── bin/rf-agent.rs     the command line
```

## Testing

```bash
cargo test
```

The tests cover the repair loop (including that diagnostics reach the model),
guardrail refusal, budget exhaustion, trace round-tripping, and the transport
error a caller sees when the runtime is down.
