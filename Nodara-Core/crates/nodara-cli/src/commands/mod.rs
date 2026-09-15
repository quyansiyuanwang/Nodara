//! Command implementations.

use std::path::PathBuf;
use std::sync::Arc;

use nodara_core::{builtin_extension, in_process_extension, CapabilityRegistry, ExtensionRegistry};
use nodara_plugin::PluginHost;

use crate::error::CliResult;

pub mod extensions;
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
    /// Unified extension registrations.
    pub extensions: ExtensionRegistry,
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
    nodara_core::register_builtins(&mut registry);
    let mut extensions = ExtensionRegistry::new();
    extensions.register(builtin_extension(registry.node_types()));
    if in_process {
        let before = registry.node_types();
        nodara_platform::register_platform(&mut registry);
        nodara_vision::register_vision(&mut registry);
        let added = registry
            .node_types()
            .into_iter()
            .filter(|node_type| !before.contains(node_type))
            .collect();
        extensions.register(in_process_extension(added));
    }

    let host = PluginHost::new();
    let mut failures = Vec::new();
    if launch_plugins {
        let roots: Vec<PathBuf> = plugin_dirs.to_vec();
        let discovered = nodara_plugin::discover_plugins(&roots);
        for error in &discovered.errors {
            failures.push((error.directory.display().to_string(), error.message.clone()));
        }
        host.register_discovered(&discovered);
        for (id, error) in host.load_all(&discovered) {
            failures.push((id, error.to_string()));
        }
    }
    host.install_into(&mut registry);
    for descriptor in host.extension_descriptors() {
        extensions.register(descriptor);
    }

    Ok(Capabilities {
        registry,
        host,
        extensions,
        failures,
    })
}
