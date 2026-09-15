//! `nodara-cli extensions`

use std::path::PathBuf;

use crate::error::CliResult;

/// List unified built-in, in-process and plugin extensions.
pub fn execute(plugin_dirs: &[PathBuf], in_process: bool, json: bool) -> CliResult<()> {
    let capabilities = super::build_capabilities(plugin_dirs, in_process, true)?;
    for (id, message) in &capabilities.failures {
        eprintln!("warning: plugin `{id}` could not be loaded: {message}");
    }
    let extensions = capabilities.extensions.descriptors();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "extensions": extensions }))?
        );
    } else if extensions.is_empty() {
        println!("no extensions registered");
    } else {
        for extension in &extensions {
            println!(
                "{} {} [{}] source={} loaded={} nodes={} capabilities={}",
                extension.id,
                extension.version,
                serde_json::to_value(extension.kind)?
                    .as_str()
                    .unwrap_or("other"),
                extension.source,
                extension.loaded,
                extension.node_types.len(),
                extension.capabilities.len()
            );
        }
    }
    capabilities.host.shutdown();
    Ok(())
}
