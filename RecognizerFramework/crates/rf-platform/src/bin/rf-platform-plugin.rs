//! The official platform plugin: serves the Windows capability set over stdio.

use std::sync::Arc;

use rf_core::CapabilityRegistry;
use rf_platform::register_platform;
use rf_plugin::{serve_stdio, PluginServerInfo};

fn main() {
    rf_plugin::tracing_init();

    let mut registry = CapabilityRegistry::new();
    register_platform(&mut registry);

    let info = PluginServerInfo::new(
        "rf.windows.platform",
        "RecognizerFramework Windows Platform",
        env!("CARGO_PKG_VERSION"),
    )
    .with_capabilities(rf_platform::CAPABILITIES.iter().copied());

    if let Err(error) = serve_stdio(Arc::new(registry), info) {
        eprintln!("rf-platform-plugin exited: {error}");
        std::process::exit(1);
    }
}
