//! HTTP server bootstrap.

use std::sync::Arc;

use crate::api;
use crate::error::{RuntimeError, RuntimeResult};
use crate::state::RuntimeState;

/// Serve the runtime API until the process is interrupted.
pub async fn serve(state: Arc<RuntimeState>) -> RuntimeResult<()> {
    let address = state.config.address();
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .map_err(|source| RuntimeError::Bind {
            address: address.clone(),
            source,
        })?;
    let router = api::router(state.clone());

    tracing::info!(
        address = %address,
        node_types = state.registry.node_types().len(),
        extensions = state.extensions.len(),
        plugins = state.host.summaries().len(),
        "runtime listening"
    );

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(RuntimeError::Io)?;

    state.host.shutdown();
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
