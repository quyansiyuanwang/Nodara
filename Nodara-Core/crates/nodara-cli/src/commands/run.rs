//! `nodara-cli run`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use nodara_core::{
    AllowAllPolicy, AllowlistPolicy, AutoApprove, CapabilityPolicy, ChannelEventSink,
    DefaultPolicy, InMemoryAuditLog, JsonlAuditLog, RunControl, RunRequest, WorkflowEngine,
};
use nodara_schema::{EventEnvelope, ExecutionEvent, RunStatus};

use crate::error::{CliError, CliResult};
use crate::workflow_io;

/// Arguments accepted by `run`.
pub struct RunArgs {
    /// Workflow file.
    pub file: PathBuf,
    /// `key=value` variable overrides.
    pub variables: Vec<String>,
    /// Allowed capabilities or permissions.
    pub allow: Vec<String>,
    /// Allow everything.
    pub allow_all: bool,
    /// Audit file.
    pub audit: Option<PathBuf>,
    /// Plugin directories.
    pub plugin_dirs: Vec<PathBuf>,
    /// Register official capabilities in-process.
    pub in_process: bool,
    /// Suppress event output.
    pub quiet: bool,
    /// Emit the outcome as JSON.
    pub json: bool,
}

/// Execute a workflow in the current process.
pub fn execute(args: RunArgs) -> CliResult<()> {
    let loaded = workflow_io::load(&args.file)?;
    if let Some(migration) = &loaded.migrated {
        eprintln!(
            "note: migrated `{}` from schema {} to {}",
            args.file.display(),
            migration.from,
            migration.to
        );
    }

    let capabilities = super::build_capabilities(&args.plugin_dirs, args.in_process, true)?;
    for (id, message) in &capabilities.failures {
        eprintln!("warning: plugin `{id}` could not be loaded: {message}");
    }

    let policy = build_policy(&args);
    let audit: Arc<dyn nodara_core::AuditLog> = match &args.audit {
        Some(path) => Arc::new(JsonlAuditLog::open(path)?),
        None => Arc::new(InMemoryAuditLog::new()),
    };

    let engine = WorkflowEngine::new(Arc::new(capabilities.registry))
        .with_policy(policy)
        .with_approval(Arc::new(AutoApprove))
        .with_audit(audit.clone());

    let (sink, receiver) = ChannelEventSink::new(4096);
    let printing = !args.quiet && !args.json;
    let reader = if printing {
        Some(std::thread::spawn(move || {
            while let Ok(envelope) = receiver.recv() {
                print_event(&envelope);
            }
        }))
    } else {
        drop(receiver);
        None
    };

    let variables = workflow_io::parse_variables(&args.variables)?;
    let request = RunRequest::new(loaded.workflow)
        .with_variables(variables)
        .with_event_sink(Arc::new(sink));
    let started = Instant::now();
    let outcome = engine.run(request, &RunControl::new());
    capabilities.host.shutdown();

    if let Some(reader) = reader {
        let _ = reader.join();
    }

    if args.json {
        let payload = serde_json::json!({
            "run_id": outcome.run_id,
            "status": outcome.status,
            "nodes_executed": outcome.nodes_executed,
            "duration_ms": outcome.duration_ms,
            "variables": outcome.variables,
            "failure": outcome.failure,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else if !printing {
        println!(
            "{}: {} node(s) in {}ms",
            status_label(outcome.status),
            outcome.nodes_executed,
            outcome.duration_ms
        );
    }

    if let Some(failure) = &outcome.failure {
        eprintln!("failure [{}]: {}", failure.code, failure.message);
    }
    eprintln!(
        "audit: {} record(s) in {}ms wall clock",
        audit.records().len(),
        started.elapsed().as_millis()
    );

    if outcome.status == RunStatus::Completed {
        Ok(())
    } else {
        Err(CliError::Failed(format!(
            "run finished with status {:?}",
            outcome.status
        )))
    }
}

fn build_policy(args: &RunArgs) -> Arc<dyn CapabilityPolicy> {
    if args.allow_all {
        return Arc::new(AllowAllPolicy);
    }
    if !args.allow.is_empty() {
        return Arc::new(AllowlistPolicy::new(args.allow.clone()));
    }
    Arc::new(DefaultPolicy)
}

fn status_label(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Pending => "pending",
        RunStatus::Running => "running",
        RunStatus::Paused => "paused",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
    }
}

fn print_event(envelope: &EventEnvelope) {
    match &envelope.event {
        ExecutionEvent::RunStarted { workflow_id } => {
            println!("run started: {workflow_id}");
        }
        ExecutionEvent::NodeStarted { node_id, node_type } => {
            println!("  -> {node_id} ({node_type})");
        }
        ExecutionEvent::NodeFinished {
            node_id,
            duration_ms,
            ..
        } => {
            println!("  ok {node_id} in {duration_ms}ms");
        }
        ExecutionEvent::NodeFailed {
            node_id,
            code,
            message,
            ..
        } => {
            println!("  !! {node_id} [{code}] {message}");
        }
        ExecutionEvent::Log { message, .. } => println!("     {message}"),
        ExecutionEvent::NodeProgress { message, .. } => {
            if let Some(message) = message {
                println!("     {message}");
            }
        }
        ExecutionEvent::CapabilityDecision {
            capability,
            decision,
            ..
        } => {
            println!("     policy {capability}: {decision}");
        }
        ExecutionEvent::RunCompleted {
            nodes_executed,
            duration_ms,
        } => println!("run completed: {nodes_executed} node(s) in {duration_ms}ms"),
        ExecutionEvent::RunFailed { code, message } => println!("run failed [{code}]: {message}"),
        ExecutionEvent::RunCancelled { reason } => {
            println!("run cancelled: {}", reason.clone().unwrap_or_default());
        }
        ExecutionEvent::RunPaused => println!("run paused"),
        ExecutionEvent::RunResumed => println!("run resumed"),
        ExecutionEvent::EdgeActivated {
            edge_id,
            source,
            target,
            branch,
        } => println!("     edge {edge_id}: {source} -> {target} ({branch:?})"),
        ExecutionEvent::DataTransferred {
            edge_id,
            source_port,
            target_port,
            ..
        } => println!("     data {edge_id}: {source_port} -> {target_port}"),
    }
}
