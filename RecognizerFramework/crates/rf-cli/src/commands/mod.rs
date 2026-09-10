//! Command implementations.

use std::path::PathBuf;
use std::sync::Arc;

use rf_core::CapabilityRegistry;
use rf_plugin::PluginHost;

use crate::error::CliResult;

pub mod inspect;
pub mod migrate;
pub mod plugins;
pub mod run;
pub mod schema;
pub mod serve;
pub mod simulate;
pub mod validate;

/// The capability set a command should work against.
pub struct Capabilities {
    /// Runnable node types.
    pub registry: CapabilityRegistry,
    /// Installed plugins.
    pub host: Arc<PluginHost>,
    /// Plugins that failed to launch.
    pub failures: Vec<(String, String)>,
}

/// Assemble the capability set from in-process official plugins and/or
/// discovered plugin processes.
pub fn build_capabilities(
    plugin_dirs: &[PathBuf],
    in_process: bool,
    launch_plugins: bool,
) -> CliResult<Capabilities> {
    let mut registry = CapabilityRegistry::new();
    rf_core::register_builtins(&mut registry);
    if in_process {
        rf_platform::register_platform(&mut registry);
        rf_vision::register_vision(&mut registry);
    }

    let host = PluginHost::new();
    let mut failures = Vec::new();
    if launch_plugins {
        let roots: Vec<PathBuf> = plugin_dirs.to_vec();
        let discovered = rf_plugin::discover_plugins(&roots);
        for error in &discovered.errors {
            failures.push((error.directory.display().to_string(), error.message.clone()));
        }
        host.register_discovered(&discovered);
        for (id, error) in host.load_all(&discovered) {
            failures.push((id, error.to_string()));
        }
    }
    host.install_into(&mut registry);

    Ok(Capabilities {
        registry,
        host,
        failures,
    })
}
