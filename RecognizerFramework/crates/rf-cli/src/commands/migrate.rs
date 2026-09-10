//! `rf-cli migrate`

use std::path::Path;

use rf_schema::{migrate, version::major_of, SCHEMA_VERSION};

use crate::error::{CliError, CliResult};
use crate::workflow_io;

/// Upgrade a legacy document to the current schema.
///
/// `from` and `to` are *assertions*, not instructions: migration always produces
/// the current schema. They exist so a script can state the version it believes
/// it is converting and fail loudly when that belief is wrong, rather than
/// silently migrating something unexpected.
pub fn execute(
    file: &Path,
    out: Option<&Path>,
    from: Option<u32>,
    to: Option<u32>,
) -> CliResult<()> {
    let text = std::fs::read_to_string(file)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;

    if let Some(expected) = to {
        let current = major_of(SCHEMA_VERSION);
        if expected != current {
            return Err(CliError::Failed(format!(
                "this build migrates to schema {current}, not {expected}"
            )));
        }
    }

    let (workflow, report) = migrate(value)?;

    if let Some(expected) = from {
        if major_of(&report.from) != expected {
            return Err(CliError::Failed(format!(
                "the source document is schema {}, not {expected}",
                report.from
            )));
        }
    }

    match out {
        Some(path) => {
            workflow_io::write(path, &workflow)?;
            eprintln!(
                "migrated {} -> {}: {}",
                report.from,
                report.to,
                path.display()
            );
            if !report.changed {
                eprintln!("note: the document already used the current schema");
            }
            for note in &report.notes {
                eprintln!("  - {note}");
            }
        }
        None => print!("{}", workflow_io::to_pretty(&workflow)?),
    }
    Ok(())
}
