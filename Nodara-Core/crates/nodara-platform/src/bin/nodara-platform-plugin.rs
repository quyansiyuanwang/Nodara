//! The official platform plugin: serves the Windows capability set over stdio.

use std::sync::Arc;

use nodara_core::CapabilityRegistry;
use nodara_platform::register_platform;
use nodara_plugin::{serve_stdio, PluginServerInfo};

fn main() {
    nodara_plugin::tracing_init();

    let mut registry = CapabilityRegistry::new();
    register_platform(&mut registry);

    let info = PluginServerInfo::new(
        "nodara.windows.platform",
        "Nodara-Core Windows Platform",
        env!("CARGO_PKG_VERSION"),
    )
    .with_capabilities(nodara_platform::CAPABILITIES.iter().copied());

    if let Err(error) = serve_stdio(Arc::new(registry), info) {
        eprintln!("nodara-platform-plugin exited: {error}");
        std::process::exit(1);
    }
}
