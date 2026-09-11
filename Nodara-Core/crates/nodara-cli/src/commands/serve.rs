//! `nodara-cli serve`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nodara_core::{AllowAllPolicy, AllowlistPolicy, CapabilityPolicy, DefaultPolicy};
use nodara_runtime::{PolicyMode, RuntimeBuilder, RuntimeConfig};

use crate::error::CliResult;

/// Arguments accepted by `serve`.
pub struct ServeArgs {
    /// Interface to bind.
    pub host: String,
    /// Port to bind.
    pub port: u16,
    /// Plugin directories.
    pub plugin_dirs: Vec<PathBuf>,
    /// Register official capabilities in-process.
    pub in_process: bool,
    /// Allowed capabilities.
    pub allow: Vec<String>,
    /// Allow everything.
    pub allow_all: bool,
    /// Audit file.
    pub audit: Option<PathBuf>,
    /// Require an operator decision for every gated capability.
    pub require_approval: bool,
    /// How long to wait for that decision.
    pub approval_timeout: Duration,
}

/// Start the runtime server.
pub fn execute(args: ServeArgs) -> CliResult<()> {
    nodara_plugin::tracing_init();

    let policy = policy_mode(&args);
    let explicit = explicit_policy(&args);
    let mut config = RuntimeConfig {
        host: args.host.clone(),
        port: args.port,
        autoload_plugins: !args.in_process,
        policy,
        auto_approve: !args.require_approval,
        approval_timeout: args.approval_timeout,
        audit_path: args.audit.clone(),
        ..RuntimeConfig::default()
    };
    config.plugin_dirs = if args.plugin_dirs.is_empty() {
        config.plugin_dirs
    } else {
        args.plugin_dirs.clone()
    };

    let mut builder = RuntimeBuilder::new(config);
    if args.in_process {
        builder.register_set(nodara_platform::register_platform);
        builder.register_set(nodara_vision::register_vision);
    }
    if let Some(policy) = explicit {
        builder = builder.with_policy(policy);
    }

    let state = builder.build()?;
    println!(
        "runtime listening on http://{}/api/v1 ({} node type(s), {} plugin(s))",
        state.config.address(),
        state.registry.node_types().len(),
        state.host.summaries().len()
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        nodara_runtime::serve(state).await?;
        Ok::<(), nodara_runtime::RuntimeError>(())
    })?;
    Ok(())
}

fn policy_mode(args: &ServeArgs) -> PolicyMode {
    if args.allow_all {
        PolicyMode::AllowAll
    } else if !args.allow.is_empty() {
        PolicyMode::Allowlist(args.allow.clone())
    } else {
        PolicyMode::Default
    }
}

fn explicit_policy(args: &ServeArgs) -> Option<Arc<dyn CapabilityPolicy>> {
    if args.allow_all {
        return Some(Arc::new(AllowAllPolicy));
    }
    if !args.allow.is_empty() {
        return Some(Arc::new(AllowlistPolicy::new(args.allow.clone())));
    }
    let _ = DefaultPolicy;
    None
}
