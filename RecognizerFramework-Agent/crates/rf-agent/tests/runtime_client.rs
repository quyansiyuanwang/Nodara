//! The agent's only route into a system is the runtime API, so the failure mode
//! when that route is unavailable must be a clear error rather than a panic or a
//! silent fallback.

use rf_agent::{AgentError, RuntimeClient};

#[test]
fn unreachable_runtime_reports_a_transport_error() {
    // Port 1 is reserved and never listening.
    let client = RuntimeClient::new("http://127.0.0.1:1");
    let error = client.health().expect_err("connection must fail");
    assert!(
        matches!(error, AgentError::Transport(_)),
        "expected a transport error, got {error:?}"
    );
}

#[test]
fn base_url_is_normalised() {
    assert_eq!(
        RuntimeClient::new("http://127.0.0.1:8710/").base(),
        "http://127.0.0.1:8710"
    );
}
