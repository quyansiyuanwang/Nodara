//! The official vision plugin: serves template matching and OCR over stdio.

use std::sync::Arc;

use rf_core::CapabilityRegistry;
use rf_plugin::{serve_stdio, PluginServerInfo};
use rf_vision::{install_backend_from_env, register_vision};

fn main() {
    rf_plugin::tracing_init();
    install_backend_from_env();

    let mut registry = CapabilityRegistry::new();
    register_vision(&mut registry);

    let info = PluginServerInfo::new(
        "rf.vision",
        "RecognizerFramework Vision",
        env!("CARGO_PKG_VERSION"),
    )
    .with_capabilities(rf_vision::CAPABILITIES.iter().copied());

    if let Err(error) = serve_stdio(Arc::new(registry), info) {
        eprintln!("rf-vision-plugin exited: {error}");
        std::process::exit(1);
    }
}
