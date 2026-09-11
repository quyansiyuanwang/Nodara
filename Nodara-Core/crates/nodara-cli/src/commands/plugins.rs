//! `nodara-cli plugins`

use std::path::PathBuf;

use nodara_plugin::discover_plugins;

use crate::error::CliResult;

/// List plugins found on disk without launching them.
pub fn execute(plugin_dirs: &[PathBuf], json: bool) -> CliResult<()> {
    let outcome = discover_plugins(plugin_dirs);

    if json {
        let payload = serde_json::json!({
            "plugins": outcome.plugins.iter().map(|plugin| {
                serde_json::json!({
                    "id": plugin.manifest.id,
                    "name": plugin.manifest.name,
                    "version": plugin.manifest.version,
                    "protocol_version": plugin.manifest.protocol_version,
                    "capabilities": plugin.manifest.capabilities,
                    "permissions": plugin.manifest.permissions,
                    "node_types": plugin.manifest.node_types,
                    "directory": plugin.directory.display().to_string(),
                })
            }).collect::<Vec<_>>(),
            "errors": outcome.errors.iter().map(|error| {
                serde_json::json!({
                    "directory": error.directory.display().to_string(),
                    "message": error.message,
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if outcome.plugins.is_empty() {
        println!("no plugins found");
    }
    for plugin in &outcome.plugins {
        println!(
            "{} {} (protocol {})",
            plugin.manifest.id, plugin.manifest.version, plugin.manifest.protocol_version
        );
        println!("  directory:   {}", plugin.directory.display());
        println!("  node types:  {}", plugin.manifest.node_types.join(", "));
        println!(
            "  capabilities: {}",
            plugin.manifest.capabilities.join(", ")
        );
        if !plugin.manifest.permissions.is_empty() {
            println!("  permissions: {}", plugin.manifest.permissions.join(", "));
        }
    }
    for error in &outcome.errors {
        eprintln!(
            "warning: {} could not be read: {}",
            error.directory.display(),
            error.message
        );
    }
    Ok(())
}
