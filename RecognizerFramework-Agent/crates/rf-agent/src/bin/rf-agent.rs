//! `rf-agent` — plan, run and replay workflows from natural language.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use rf_agent::{
    audit::{self, AuditTrace},
    Agent, AgentConfig, AgentResult, GuardrailPolicy, LlmProvider, MockProvider, OpenAiProvider,
    RuntimeClient, ToolPolicy,
};

#[derive(Parser)]
#[command(
    name = "rf-agent",
    about = "Plan and operate RecognizerFramework workflows from natural language",
    version
)]
struct Cli {
    /// Runtime base URL.
    #[arg(
        long,
        global = true,
        env = "RF_RUNTIME_URL",
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
    },

    /// Replay a recorded decision trace.
    Replay {
        /// Trace file written by `--trace`.
        file: PathBuf,
    },

    /// List agent sessions and any approvals waiting on an operator.
    Sessions {
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
        } => {
            let provider = build_provider(&mock)?;
            let mut config = config;
            if offline {
                // Offline planning still validates locally against the shipped
                // schema rules, it just cannot consult the runtime's catalogue.
                config.runtime_url = cli.runtime.clone();
            }
            let agent = Agent::new(provider.as_ref(), config);
            let outcome = agent.plan(&goal, &constraints)?;
            print_outcome(&outcome)?;
            if cli.report {
                print!("{}", outcome.report.to_markdown());
            }
            if let (Some(workflow), Some(path)) = (&outcome.workflow, out) {
                write_workflow(workflow, &path)?;
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
        } => {
            let provider = build_provider(&mock)?;
            let mut config = config;
            config.auto_run = true;
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

        Command::Approve {
            session_id,
            approval_id,
            deny,
            by,
        } => {
            let client = RuntimeClient::new(cli.runtime.clone());
            let decision = if deny {
                rf_schema::ApprovalDecision::Denied
            } else {
                rf_schema::ApprovalDecision::Approved
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
    match OpenAiProvider::from_env() {
        Ok(provider) => Ok(Box::new(provider)),
        Err(error) => {
            eprintln!("warning: {error}; falling back to an empty scripted provider");
            Ok(Box::new(MockProvider::new([""])))
        }
    }
}

fn offline_provider() -> MockProvider {
    MockProvider::new([""])
}

fn print_outcome(outcome: &rf_agent::AgentOutcome) -> AgentResult<()> {
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

fn write_workflow(workflow: &rf_schema::Workflow, path: &PathBuf) -> AgentResult<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(workflow)?),
    )?;
    Ok(())
}
