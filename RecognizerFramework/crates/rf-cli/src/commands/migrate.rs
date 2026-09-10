//! `rf-cli migrate`

use std::path::Path;

use rf_schema::migrate;

use crate::error::CliResult;
use crate::workflow_io;

/// Upgrade a legacy document to the current schema.
pub fn execute(file: &Path, out: Option<&Path>) -> CliResult<()> {
    let text = std::fs::read_to_string(file)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let (workflow, report) = migrate(value)?;

    match out {
        Some(path) => {
            workflow_io::write(path, &workflow)?;
            eprintln!(
                "migrated {} -> {}: {} ({} change(s))",
                report.from,
                report.to,
                path.display(),
                if report.changed {
                    "applied"
                } else {
                    "already current"
                }
            );
        }
        None => print!("{}", workflow_io::to_pretty(&workflow)?),
    }
    Ok(())
}
