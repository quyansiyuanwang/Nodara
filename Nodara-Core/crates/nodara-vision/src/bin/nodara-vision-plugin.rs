//! The official vision plugin: serves template matching and OCR over stdio.

use std::sync::Arc;

use nodara_core::CapabilityRegistry;
use nodara_plugin::{serve_stdio, PluginServerInfo};
use nodara_vision::{install_backend_from_env, register_vision};

fn main() {
    nodara_plugin::tracing_init();
    install_backend_from_env();

    let mut registry = CapabilityRegistry::new();
    register_vision(&mut registry);

    let info = PluginServerInfo::new(
        "nodara.vision",
        "Nodara-Core Vision",
        env!("CARGO_PKG_VERSION"),
    )
    .with_capabilities(nodara_vision::CAPABILITIES.iter().copied());

    if let Err(error) = serve_stdio(Arc::new(registry), info) {
        eprintln!("nodara-vision-plugin exited: {error}");
        std::process::exit(1);
    }
}
