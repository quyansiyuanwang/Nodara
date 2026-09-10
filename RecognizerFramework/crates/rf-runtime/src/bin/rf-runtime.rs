//! Standalone runtime binary.
//!
//! Loads plugins from disk only; official capabilities that ship as separate
//! plugins are discovered through `plugins/`.

use rf_runtime::{serve, RuntimeBuilder, RuntimeConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    rf_plugin::tracing_init();

    let mut config = RuntimeConfig::default();
    if let Ok(port) = std::env::var("RF_RUNTIME_PORT") {
        if let Ok(port) = port.parse() {
            config.port = port;
        }
    }
    if let Ok(dirs) = std::env::var("RF_PLUGIN_DIRS") {
        for dir in dirs.split(';').filter(|dir| !dir.trim().is_empty()) {
            config.plugin_dirs.push(dir.trim().into());
        }
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let state = RuntimeBuilder::new(config).build()?;
        serve(state).await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    Ok(())
}
