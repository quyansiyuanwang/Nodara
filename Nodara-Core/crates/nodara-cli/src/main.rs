//! `nodara-cli` — the headless entry point to Nodara-Core.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod commands;
mod error;
mod workflow_io;

pub use error::{CliError, CliResult};

#[derive(Parser)]
#[command(
    name = "nodara-cli",
    about = "Nodara-Core command line: validate, run, simulate, inspect, migrate, serve",
    version,
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a workflow document.
    Validate {
        /// Workflow JSON file.
        file: PathBuf,
        /// Emit the report as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Execute a workflow.
    Run {
        /// Workflow JSON file.
        file: PathBuf,
        /// Set a run variable (`key=value`).
        #[arg(long = "var", value_name = "KEY=VALUE")]
        variables: Vec<String>,
        /// Allow a capability or permission. Repeatable.
        #[arg(long = "allow", value_name = "CAPABILITY")]
        allow: Vec<String>,
        /// Allow every capability.
        #[arg(long)]
        allow_all: bool,
        /// Append audit records to this file.
        #[arg(long, value_name = "FILE")]
        audit: Option<PathBuf>,
        /// Directory scanned for plugins. Repeatable.
        #[arg(long = "plugin-dir", value_name = "DIR")]
        plugin_dirs: Vec<PathBuf>,
        /// Register the official capabilities in-process instead of launching plugins.
        #[arg(long)]
        in_process: bool,
        /// Do not print execution events.
        #[arg(long)]
        quiet: bool,
        /// Emit the final outcome as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Report what a workflow would do, without executing it.
    Simulate {
        /// Workflow JSON file.
        file: PathBuf,
        /// Emit the plan as JSON.
        #[arg(long)]
        json: bool,
        /// Directory scanned for plugins. Repeatable.
        #[arg(long = "plugin-dir", value_name = "DIR")]
        plugin_dirs: Vec<PathBuf>,
        /// Register the official capabilities in-process.
        #[arg(long)]
        in_process: bool,
    },

    /// Print the structure of a workflow document.
    Inspect {
        /// Workflow JSON file.
        file: PathBuf,
        /// Emit the summary as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Upgrade a legacy workflow document to the current schema.
    Migrate {
        /// Source document.
        file: PathBuf,
        /// Destination file. Defaults to stdout.
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
        /// Assert that the source document is this schema major version.
        #[arg(long, value_name = "MAJOR")]
        from: Option<u32>,
        /// Assert that the result is this schema major version.
        #[arg(long, value_name = "MAJOR")]
        to: Option<u32>,
    },

    /// List plugins found under the configured directories.
    Plugins {
        /// Directory scanned for plugins. Repeatable.
        #[arg(long = "plugin-dir", value_name = "DIR")]
        plugin_dirs: Vec<PathBuf>,
        /// Emit the list as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Export the published JSON Schema documents.
    Schema {
        /// Directory to write the schemas into.
        #[arg(long, value_name = "DIR", default_value = "schema")]
        out: PathBuf,
        /// Print the schemas instead of writing files.
        #[arg(long)]
        stdout: bool,
        /// Directory scanned for plugins whose node types join the workflow
        /// schema. Repeatable.
        #[arg(long = "plugin-dir", value_name = "DIR")]
        plugin_dirs: Vec<PathBuf>,
        /// Publish the plain schema without the installed node catalog.
        #[arg(long)]
        no_capabilities: bool,
    },

    /// Start the headless runtime server.
    Serve {
        /// Interface to bind.
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to bind.
        #[arg(long, default_value_t = 8710)]
        port: u16,
        /// Directory scanned for plugins. Repeatable.
        #[arg(long = "plugin-dir", value_name = "DIR")]
        plugin_dirs: Vec<PathBuf>,
        /// Register the official capabilities in-process instead of launching plugins.
        #[arg(long)]
        in_process: bool,
        /// Allow a capability or permission. Repeatable.
        #[arg(long = "allow", value_name = "CAPABILITY")]
        allow: Vec<String>,
        /// Allow every capability.
        #[arg(long)]
        allow_all: bool,
        /// Append audit records to this file.
        #[arg(long, value_name = "FILE")]
        audit: Option<PathBuf>,
        /// Require an operator decision for every gated capability
        /// (raises a request against the owning agent session).
        #[arg(long)]
        require_approval: bool,
        /// Seconds to wait for that decision.
        #[arg(long, value_name = "SECONDS", default_value_t = 300)]
        approval_timeout: u64,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> CliResult<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { file, json } => commands::validate::execute(&file, json),
        Command::Run {
            file,
            variables,
            allow,
            allow_all,
            audit,
            plugin_dirs,
            in_process,
            quiet,
            json,
        } => commands::run::execute(commands::run::RunArgs {
            file,
            variables,
            allow,
            allow_all,
            audit,
            plugin_dirs,
            in_process,
            quiet,
            json,
        }),
        Command::Simulate {
            file,
            json,
            plugin_dirs,
            in_process,
        } => commands::simulate::execute(&file, json, &plugin_dirs, in_process),
        Command::Inspect { file, json } => commands::inspect::execute(&file, json),
        Command::Migrate {
            file,
            out,
            from,
            to,
        } => commands::migrate::execute(&file, out.as_deref(), from, to),
        Command::Plugins { plugin_dirs, json } => commands::plugins::execute(&plugin_dirs, json),
        Command::Schema {
            out,
            stdout,
            plugin_dirs,
            no_capabilities,
        } => commands::schema::execute(commands::schema::SchemaArgs {
            out,
            stdout,
            plugin_dirs,
            capabilities: !no_capabilities,
        }),
        Command::Serve {
            host,
            port,
            plugin_dirs,
            in_process,
            allow,
            allow_all,
            audit,
            require_approval,
            approval_timeout,
        } => commands::serve::execute(commands::serve::ServeArgs {
            host,
            port,
            plugin_dirs,
            in_process,
            allow,
            allow_all,
            audit,
            require_approval,
            approval_timeout: std::time::Duration::from_secs(approval_timeout),
        }),
    }
}
