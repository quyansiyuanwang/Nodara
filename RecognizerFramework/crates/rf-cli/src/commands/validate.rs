//! `rf-cli validate`

use std::path::Path;

use rf_schema::{validate_with, Severity, ValidationReport};

use crate::error::{CliError, CliResult};
use crate::workflow_io;

/// Validate a workflow document.
pub fn execute(file: &Path, json: bool) -> CliResult<()> {
    let loaded = workflow_io::load(file)?;
    let capabilities = super::build_capabilities(&[], false, false)?;
    let report = validate_with(
        &loaded.workflow,
        &capabilities.registry,
        &Default::default(),
    );

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(file, &loaded, &report);
    }

    if report.is_valid() {
        Ok(())
    } else {
        Err(CliError::Invalid(report.error_count()))
    }
}

fn print_report(file: &Path, loaded: &workflow_io::LoadedWorkflow, report: &ValidationReport) {
    if let Some(migration) = &loaded.migrated {
        println!(
            "note: migrated `{}` from schema {} to {}",
            file.display(),
            migration.from,
            migration.to
        );
    }
    if report.diagnostics.is_empty() {
        println!("ok: {} is a valid workflow", file.display());
        return;
    }
    for diagnostic in &report.diagnostics {
        let severity = match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        println!(
            "{severity} [{}] {} ({})",
            diagnostic.code, diagnostic.message, diagnostic.path
        );
        if let Some(hint) = &diagnostic.hint {
            println!("      hint: {hint}");
        }
    }
    println!(
        "{} error(s), {} warning(s)",
        report.error_count(),
        report.warning_count()
    );
}
