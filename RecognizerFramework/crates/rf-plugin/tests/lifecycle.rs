//! Plugin lifecycle failure modes.
//!
//! The architecture document requires that "a plugin crash must not take down
//! the host". That is only true if every way a plugin can fail is actually
//! handled, so each one is exercised here: it cannot be launched, it dies before
//! the handshake, it stops answering, and it dies mid-session.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rf_plugin::{
    methods, JsonRpcTransport, PluginClient, PluginError, PluginManifest, StdioTransport,
};

fn manifest_with(executable: &str) -> PluginManifest {
    serde_json::from_value(serde_json::json!({
        "id": "rf.test.lifecycle",
        "name": "Lifecycle",
        "version": "1.0.0",
        "protocol_version": "1",
        "executable": executable,
        "capabilities": ["Test.Capability"],
        "node_types": ["test.Node"]
    }))
    .expect("manifest parses")
}

/// Absolute path to `cmd.exe`.
///
/// A bare name would be resolved by the plugin loader relative to the plugin
/// directory first, which is exactly the behaviour under test elsewhere; here we
/// want the real shell, deterministically.
#[cfg(windows)]
fn cmd_exe() -> PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    PathBuf::from(root).join("System32").join("cmd.exe")
}

#[test]
fn a_missing_executable_is_reported_not_ignored() {
    let manifest = manifest_with("definitely-not-installed.exe");
    let error = PluginClient::connect(&manifest, Path::new("."))
        .expect_err("launching a missing binary must fail");
    assert!(
        matches!(error, PluginError::Launch { .. }),
        "expected a launch error, got {error:?}"
    );
}

/// A process that exits without ever speaking the protocol.
///
/// The handshake must fail promptly rather than block until the control timeout.
#[cfg(windows)]
#[test]
fn a_crash_before_the_handshake_does_not_hang() {
    let mut manifest = manifest_with(&cmd_exe().display().to_string());
    manifest.args = vec!["/c".to_string(), "exit".to_string()];

    let started = std::time::Instant::now();
    let error = PluginClient::connect(&manifest, Path::new("."))
        .expect_err("a process that never speaks the protocol cannot handshake");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the handshake should fail as soon as the channel closes"
    );
    assert!(
        matches!(
            error,
            PluginError::Disconnected | PluginError::Protocol(_) | PluginError::Json(_)
        ),
        "expected a channel error, got {error:?}"
    );
}

/// A process that stays alive but never answers.
#[cfg(windows)]
#[test]
fn a_silent_plugin_times_out_instead_of_blocking_forever() {
    let transport = StdioTransport::launch(
        "rf.test.silent",
        &cmd_exe(),
        &["/c".to_string(), "ping -n 10 127.0.0.1 >nul".to_string()],
        std::sync::Arc::new(rf_plugin::NullNotificationSink),
    )
    .expect("the helper process launches");

    let started = std::time::Instant::now();
    let error = transport
        .request(
            methods::HEALTH,
            serde_json::Value::Null,
            Duration::from_millis(250),
        )
        .expect_err("a silent plugin must time out");
    assert!(
        matches!(error, PluginError::Timeout { .. }),
        "expected a timeout, got {error:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    transport.close();
}

/// The real plugin, killed mid-session.
fn platform_binary() -> Option<PathBuf> {
    // Resolved by Cargo for the platform crate's own tests; here we look for the
    // sibling build output so the plugin suite is self-contained.
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("debug")
            .join("rf-platform-plugin.exe"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("release")
            .join("rf-platform-plugin.exe"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

#[test]
fn a_plugin_that_dies_mid_session_is_seen_as_disconnected() {
    let Some(binary) = platform_binary() else {
        eprintln!("skipping: rf-platform-plugin has not been built yet");
        return;
    };
    let manifest = manifest_with(&binary.display().to_string());
    let client = PluginClient::connect(&manifest, Path::new(".")).expect("handshake");
    assert!(client.health().expect("health responds"));

    // Kill the plugin the way an operator or a crash would.
    client.transport().close();

    let error = client
        .describe(Vec::new())
        .expect_err("the channel is gone");
    assert!(
        matches!(error, PluginError::Disconnected),
        "expected disconnection, got {error:?}"
    );
    // Liveness probes degrade to `false` rather than raising.
    assert!(!client.health().expect("health still answers"));
}
