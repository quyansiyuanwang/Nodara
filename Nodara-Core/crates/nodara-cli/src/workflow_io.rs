//! Reading and writing workflow documents.

use std::path::Path;

use nodara_schema::{MigrationReport, Workflow};

use crate::error::{CliError, CliResult};

/// A workflow loaded from disk, plus a note about any migration applied.
pub struct LoadedWorkflow {
    /// The workflow document.
    pub workflow: Workflow,
    /// Migration summary, when the source was a legacy document.
    pub migrated: Option<MigrationReport>,
}

/// Read a workflow, upgrading it automatically when it uses a legacy shape.
pub fn load(file: &Path) -> CliResult<LoadedWorkflow> {
    let text = std::fs::read_to_string(file)?;
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let declared = value
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(nodara_schema::SCHEMA_VERSION);
    if declared != nodara_schema::SCHEMA_VERSION {
        return Err(CliError::Failed(format!(
            "WF118: workflow schema `{declared}` must be migrated to `{}` first; run `nodara-cli migrate`",
            nodara_schema::SCHEMA_VERSION
        )));
    }

    let workflow: Workflow = serde_json::from_value(value)?;
    Ok(LoadedWorkflow {
        workflow,
        migrated: None,
    })
}
/// Serialize a workflow with stable formatting.
pub fn to_pretty(workflow: &Workflow) -> CliResult<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(workflow)?))
}

/// Write a workflow to disk, creating parent directories as needed.
pub fn write(file: &Path, workflow: &Workflow) -> CliResult<()> {
    if let Some(parent) = file.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(file, to_pretty(workflow)?)?;
    Ok(())
}

/// Parse `key=value` pairs supplied on the command line.
pub fn parse_variables(
    pairs: &[String],
) -> CliResult<std::collections::BTreeMap<String, serde_json::Value>> {
    let mut variables = std::collections::BTreeMap::new();
    for pair in pairs {
        let (key, raw) = pair
            .split_once('=')
            .ok_or_else(|| CliError::InvalidVariable(pair.clone()))?;
        let value: serde_json::Value = serde_json::from_str(raw)
            .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
        variables.insert(key.trim().to_string(), value);
    }
    Ok(variables)
}
