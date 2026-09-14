//! `nodara-agent` — plan, run and replay workflows from natural language.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use nodara_agent::{
    audit::{self, AuditTrace},
    Agent, AgentConfig, AgentError, AgentResult, ExplainTarget, GuardrailPolicy, LlmProvider,
    MockProvider, OpenAiProvider, RuntimeClient, ToolPolicy,
};

#[derive(Parser)]
#[command(
    name = "nodara-agent",
    about = "Plan and operate Nodara-Core workflows from natural language",
    version
)]
struct Cli {
    /// Runtime base URL.
    #[arg(
        long,
        global = true,
        env = "NODARA_RUNTIME_URL",
        default_value = "http://127.0.0.1:8710"
    )]
    runtime: String,

    /// Write the decision trace here.
    #[arg(long, global = true, value_name = "FILE")]
    trace: Option<PathBuf>,

    /// Refuse node types that perform side effects.
    #[arg(long, global = true)]
    safe: bool,

    /// Allow only these node types (repeatable). Implies an allowlist.
    #[arg(long = "allow", global = true, value_name = "NODE_TYPE")]
    allow: Vec<String>,

    /// Print the execution report as Markdown when the command finishes.
    #[arg(long, global = true)]
    report: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the node types the runtime can execute.
    Capabilities {
        /// Emit as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Turn a goal into a workflow document.
    Plan {
        /// What the workflow should do.
        goal: String,
        /// Extra constraint, repeatable.
        #[arg(long = "constraint")]
        constraints: Vec<String>,
        /// Write the workflow here instead of stdout.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Replay these canned model replies instead of calling a provider.
        #[arg(long = "mock", value_name = "JSON", hide = true)]
        mock: Vec<String>,
        /// Validate through the runtime (default when the runtime is reachable).
        #[arg(long)]
        offline: bool,
        /// Modify this existing workflow instead of authoring a new one.
        #[arg(long = "from", value_name = "FILE")]
        from: Option<PathBuf>,
    },

    /// Plan a workflow and run it.
    Run {
        /// What the workflow should do.
        goal: String,
        /// Extra constraint, repeatable.
        #[arg(long = "constraint")]
        constraints: Vec<String>,
        /// Variables passed to the run, as a JSON object.
        #[arg(long, default_value = "{}")]
        variables: String,
        /// Seconds to wait for the run.
        #[arg(long, default_value_t = 300)]
        timeout: u64,
        /// Replay these canned model replies instead of calling a provider.
        #[arg(long = "mock", value_name = "JSON", hide = true)]
        mock: Vec<String>,
        /// Modify this existing workflow instead of authoring a new one.
        #[arg(long = "from", value_name = "FILE")]
        from: Option<PathBuf>,
    },

    /// Replay a recorded decision trace.
    Replay {
        /// Trace file written by `--trace`.
        file: PathBuf,
    },

    /// Explain a workflow, or why a run ended the way it did.
    Explain {
        /// Workflow JSON file.
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,
        /// A run to account for, in addition to the workflow.
        #[arg(long, value_name = "RUN_ID")]
        run: Option<String>,
    },

    /// List agent sessions and any approvals waiting on an operator.
    Sessions {
        /// Emit as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Show the runtime's audit log — what it allowed, refused and recorded.
    Audit {
        /// Restrict to one run.
        #[arg(long, value_name = "RUN_ID")]
        run: Option<String>,
        /// Emit as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Answer a pending approval that is blocking a run.
    Approve {
        /// Session the approval belongs to.
        session_id: String,
        /// Approval to decide.
        approval_id: String,
        /// Refuse it instead of granting it.
        #[arg(long)]
        deny: bool,
        /// Who is deciding, recorded in the audit trail.
        #[arg(long, default_value = "operator")]
        by: String,
    },

    /// Pause, resume or cancel a run.
    Control {
        /// Run to steer.
        run_id: String,
        /// What to do.
        #[arg(value_enum)]
        action: ControlAction,
    },
}

/// Run-control actions available from the command line.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum ControlAction {
    /// Suspend at the next node boundary.
    Pause,
    /// Continue.
    Resume,
    /// Stop.
    Cancel,
}

fn main() {
    tracing_subscriber_init();
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn tracing_subscriber_init() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init();
}

fn run() -> AgentResult<()> {
    let cli = Cli::parse();

    let mut guardrails = GuardrailPolicy::permissive();
    if cli.safe {
        guardrails = GuardrailPolicy::safe();
    }
    if !cli.allow.is_empty() {
        guardrails.tools = ToolPolicy::allow_only(cli.allow.clone());
    }

    let config = AgentConfig {
        runtime_url: cli.runtime.clone(),
        trace_path: cli.trace.clone(),
        guardrails,
        ..AgentConfig::default()
    };

    match cli.command {
        Command::Capabilities { json } => {
            let provider = offline_provider();
            let agent = Agent::new(&provider, config);
            let descriptors = agent.capabilities()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&descriptors)?);
            } else {
                for descriptor in descriptors {
                    let gate = if descriptor.dangerous { " [gated]" } else { "" };
                    println!(
                        "{:<28} {}{gate}",
                        descriptor.node_type, descriptor.display_name
                    );
                }
            }
            Ok(())
        }

        Command::Plan {
            goal,
            constraints,
            out,
            mock,
            offline,
            from,
        } => {
            let provider = build_provider(&mock)?;
            let mut config = config;
            config.base_workflow = read_base(&from)?;
            if offline {
                // Offline planning validates against the shipped schema rules
                // only: no capability discovery, no session publication, no
                // runtime round trip of any kind.
                config.offline = true;
                config.publish_session = false;
            }
            let agent = Agent::new(provider.as_ref(), config);
            let outcome = agent.plan(&goal, &constraints)?;
            print_outcome(&outcome)?;
            if cli.report {
                print!("{}", outcome.report.to_markdown());
            }
            if let (Some(workflow), Some(path)) = (&outcome.workflow, out) {
                write_workflow(workflow, &cli.runtime, &path)?;
                eprintln!("wrote {}", path.display());
            }
            if !outcome.accepted {
                std::process::exit(2);
            }
            Ok(())
        }

        Command::Run {
            goal,
            constraints,
            variables,
            timeout,
            mock,
            from,
        } => {
            let provider = build_provider(&mock)?;
            let mut config = config;
            config.auto_run = true;
            config.base_workflow = read_base(&from)?;
            config.run_timeout = Duration::from_secs(timeout);
            config.variables = serde_json::from_str(&variables)?;
            let agent = Agent::new(provider.as_ref(), config);
            let outcome = agent.plan_and_run(&goal, &constraints)?;
            print_outcome(&outcome)?;
            if cli.report {
                print!("{}", outcome.report.to_markdown());
            }
            if !outcome.accepted {
                std::process::exit(2);
            }
            if outcome.run.is_some() && !outcome.run_succeeded() {
                std::process::exit(3);
            }
            Ok(())
        }

        Command::Replay { file } => {
            let entries = AuditTrace::load(&file)?;
            println!("{}", audit::render(&entries));
            Ok(())
        }

        Command::Explain { file, run } => {
            let provider = build_provider(&[])?;
            let agent = Agent::new(provider.as_ref(), config);
            let mut target = match &file {
                Some(path) => {
                    let text = std::fs::read_to_string(path)?;
                    ExplainTarget::workflow(serde_json::from_str(&text)?)
                }
                None => ExplainTarget::default(),
            };
            if let Some(run_id) = run {
                target = target.with_run(run_id);
            }
            if target.workflow.is_none() && target.run_id.is_none() {
                eprintln!("nothing to explain: pass a workflow file, a run id, or both");
                std::process::exit(2);
            }
            println!("{}", agent.explain(&target)?);
            Ok(())
        }

        Command::Sessions { json } => {
            let client = RuntimeClient::new(cli.runtime.clone());
            let payload = client.sessions()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                print_sessions(&payload);
            }
            Ok(())
        }

        Command::Audit { run, json } => {
            let client = RuntimeClient::new(cli.runtime.clone());
            let records = client.audit(run.as_deref(), Some(200))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&records)?);
            } else if records.is_empty() {
                println!("no audit records");
            } else {
                for record in &records {
                    let decision = record
                        .get("decision")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    println!(
                        "{:<10} {:<20} {:<12} {}",
                        record
                            .get("category")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("-"),
                        record
                            .get("node_id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(""),
                        decision,
                        record
                            .get("message")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("")
                    );
                }
            }
            Ok(())
        }

        Command::Approve {
            session_id,
            approval_id,
            deny,
            by,
        } => {
            let client = RuntimeClient::new(cli.runtime.clone());
            let decision = if deny {
                nodara_schema::ApprovalDecision::Denied
            } else {
                nodara_schema::ApprovalDecision::Approved
            };
            let session = client.decide_approval(&session_id, &approval_id, decision, &by)?;
            eprintln!("session {session_id} is now {:?}", session.status);
            Ok(())
        }

        Command::Control { run_id, action } => {
            let client = RuntimeClient::new(cli.runtime.clone());
            let snapshot = match action {
                ControlAction::Pause => client.pause(&run_id)?,
                ControlAction::Resume => client.resume(&run_id)?,
                ControlAction::Cancel => client.cancel(&run_id)?,
            };
            eprintln!(
                "run {run_id} is now {}",
                snapshot
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown")
            );
            Ok(())
        }
    }
}

fn print_sessions(payload: &serde_json::Value) {
    let sessions = payload
        .get("sessions")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if sessions.is_empty() {
        println!("no sessions");
    }
    for session in &sessions {
        println!(
            "{}  {:<10}  {}",
            session
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("-"),
            session
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("-"),
            session
                .get("goal")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("-")
        );
    }
    let pending = payload
        .get("pending_approvals")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !pending.is_empty() {
        println!("\nawaiting approval:");
        for entry in &pending {
            let approval = &entry["approval"];
            println!(
                "  session {} approval {} -> {}",
                entry
                    .get("session_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("-"),
                approval
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("-"),
                approval
                    .get("reason")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("-")
            );
        }
    }
}

fn build_provider(mock: &[String]) -> AgentResult<Box<dyn LlmProvider>> {
    if !mock.is_empty() {
        return Ok(Box::new(MockProvider::new(mock.to_vec())));
    }
    // A missing key must fail loudly: silently falling back to an empty
    // scripted provider turned a configuration problem into a confusing
    // "the model did not produce a workflow" failure.
    OpenAiProvider::from_env()
        .map_err(|error| AgentError::Configuration(error.to_string()))
        .map(|provider| Box::new(provider) as Box<dyn LlmProvider>)
}

fn offline_provider() -> MockProvider {
    MockProvider::new([""])
}

/// Read the workflow a modification request starts from.
fn read_base(path: &Option<PathBuf>) -> AgentResult<Option<nodara_schema::Workflow>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&text)?))
}

fn print_outcome(outcome: &nodara_agent::AgentOutcome) -> AgentResult<()> {
    match &outcome.workflow {
        Some(workflow) => println!("{}", serde_json::to_string_pretty(workflow)?),
        None => eprintln!("the model did not produce a workflow document"),
    }
    if let Some(run) = &outcome.run {
        eprintln!(
            "run {} -> {} ({} node(s))",
            run.get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("-"),
            run.get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("-"),
            run.get("nodes_executed")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        );
    }
    Ok(())
}

/// Write a planned workflow to disk.
///
/// The document is stamped with the schema the runtime serves, so a file the
/// agent produces is completed by an editor exactly like a hand-written one —
/// node types, configuration keys, defaults and hover documentation — as long
/// as the model did not already choose a schema of its own.
fn write_workflow(
    workflow: &nodara_schema::Workflow,
    runtime_url: &str,
    path: &PathBuf,
) -> AgentResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let document = document_for_disk(workflow, runtime_url);
    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&document)?),
    )?;
    Ok(())
}

/// The document as it should be written: the planned workflow with the
/// runtime's schema URL filled in when the plan did not carry one.
fn document_for_disk(
    workflow: &nodara_schema::Workflow,
    runtime_url: &str,
) -> nodara_schema::Workflow {
    let mut document = workflow.clone();
    if document.schema_url.is_none() {
        let base = runtime_url.trim_end_matches('/');
        document.schema_url = Some(format!("{base}/api/v1/schema/workflow"));
    }
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow() -> nodara_schema::Workflow {
        let mut workflow = nodara_schema::Workflow::new("wf.planned");
        workflow.add_node(nodara_schema::Node::new("start", "core.Start"));
        workflow
    }

    #[test]
    fn planned_documents_point_at_the_runtime_schema() {
        let document = document_for_disk(&workflow(), "http://127.0.0.1:8710/");
        assert_eq!(
            document.schema_url.as_deref(),
            Some("http://127.0.0.1:8710/api/v1/schema/workflow")
        );
    }

    #[test]
    fn a_schema_the_model_chose_is_kept() {
        let mut workflow = workflow();
        workflow.schema_url = Some("./workflow.schema.json".to_string());
        let document = document_for_disk(&workflow, "http://127.0.0.1:8710");
        assert_eq!(
            document.schema_url.as_deref(),
            Some("./workflow.schema.json")
        );
    }
}
