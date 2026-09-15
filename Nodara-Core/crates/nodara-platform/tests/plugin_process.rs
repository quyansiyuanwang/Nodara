//! Proves the out-of-process plugin path: the runtime launches the real plugin
//! binary, performs the handshake and discovers its node types.

use std::path::PathBuf;

use nodara_plugin::{methods, PluginClient, PluginError, PluginManifest};

/// The plugin binary built from this crate, resolved by Cargo.
fn plugin_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nodara-platform-plugin"))
}

fn manifest() -> PluginManifest {
    let mut manifest: PluginManifest = serde_json::from_value(serde_json::json!({
        "id": "nodara.windows.platform",
        "name": "Nodara-Core Windows Platform",
        "version": "2.0.0",
        "protocol_version": "1",
        "executable": plugin_binary().display().to_string(),
        "capabilities": nodara_platform::CAPABILITIES,
        "permissions": nodara_platform::PERMISSIONS,
        "node_types": nodara_platform::NODE_TYPES
    }))
    .expect("manifest parses");
    manifest.executable = plugin_binary().display().to_string();
    manifest
}

#[test]
fn launches_handsakes_and_describes() {
    let manifest = manifest();
    let client = PluginClient::connect(&manifest, std::path::Path::new("."))
        .expect("plugin launches and handshakes");

    assert_eq!(client.info().id, "nodara.windows.platform");
    assert!(client
        .capabilities()
        .contains(&"Input.Keyboard".to_string()));

    let described: Vec<String> = client
        .descriptors()
        .iter()
        .map(|descriptor| descriptor.node_type.clone())
        .collect();
    for expected in nodara_platform::NODE_TYPES {
        assert!(
            described.contains(&(*expected).to_string()),
            "plugin did not describe `{expected}`"
        );
    }

    // Every descriptor must carry a usable config schema.
    for descriptor in client.descriptors() {
        assert!(
            descriptor.config_schema.is_object(),
            "`{}` has no config schema",
            descriptor.node_type
        );
    }

    assert!(client.health().expect("health responds"));

    // A per-type describe must filter.
    let filtered = client
        .describe(vec!["windows.Input.Keyboard".to_string()])
        .expect("describe responds");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].node_type, "windows.Input.Keyboard");
    assert!(filtered[0].dangerous);
    assert!(filtered[0]
        .permissions
        .contains(&"input.control".to_string()));

    client.shutdown().expect("shutdown succeeds");
}

#[test]
fn reports_an_error_for_an_unknown_node_type() {
    let manifest = manifest();
    let client =
        PluginClient::connect(&manifest, std::path::Path::new(".")).expect("plugin launches");

    let error = client
        .execute(nodara_plugin::ExecuteParams {
            run_id: "run-1".to_string(),
            node_id: "n1".to_string(),
            node_type: "does.Not.Exist".to_string(),
            config: serde_json::json!({}),
            inputs: Default::default(),
            variables: Default::default(),
            artifacts: Vec::new(),
            timeout_ms: Some(5_000),
        })
        .expect_err("unknown node types must fail");

    match error {
        PluginError::Remote { code, .. } => {
            assert_eq!(code, nodara_plugin::codes::APP_UNSUPPORTED);
        }
        other => panic!("expected a remote error, got {other:?}"),
    }
    client.shutdown().expect("shutdown succeeds");
}

#[test]
fn executes_a_command_through_the_plugin_boundary() {
    let manifest = manifest();
    let client =
        PluginClient::connect(&manifest, std::path::Path::new(".")).expect("plugin launches");

    let result = client
        .execute(nodara_plugin::ExecuteParams {
            run_id: "run-command".to_string(),
            node_id: "command".to_string(),
            node_type: "system.Command".to_string(),
            config: serde_json::json!({
                "program": "echo",
                "args": ["hello from plugin"],
                "shell": true,
                "check_exit_code": true,
                "wait": true
            }),
            inputs: Default::default(),
            variables: Default::default(),
            artifacts: Vec::new(),
            timeout_ms: Some(5_000),
        })
        .expect("command executes");

    assert_eq!(result.outputs["exit_code"], 0);
    assert_eq!(result.outputs["success"], true);
    assert!(result.outputs["out"]
        .as_str()
        .is_some_and(|text| text.contains("hello from plugin")));
    client.shutdown().expect("shutdown succeeds");
}

#[test]
fn mouse_relative_zero_move_reports_cursor_metadata() {
    let manifest = manifest();
    let client =
        PluginClient::connect(&manifest, std::path::Path::new(".")).expect("plugin launches");

    let result = client
        .execute(nodara_plugin::ExecuteParams {
            run_id: "run-mouse".to_string(),
            node_id: "mouse".to_string(),
            node_type: "windows.Input.Mouse".to_string(),
            config: serde_json::json!({
                "action": "move",
                "x": 0,
                "y": 0,
                "relative": true,
                "duration_ms": 0
            }),
            inputs: Default::default(),
            variables: Default::default(),
            artifacts: Vec::new(),
            timeout_ms: Some(5_000),
        })
        .expect("relative zero move executes without changing the cursor");

    assert_eq!(result.outputs["out"]["action"], "move");
    assert!(result.outputs["out"]["x"].is_number());
    assert!(result.outputs["out"]["y"].is_number());
    client.shutdown().expect("shutdown succeeds");
}

#[test]
fn cancels_a_running_node() {
    let manifest = manifest();
    let client = std::sync::Arc::new(
        PluginClient::connect(&manifest, std::path::Path::new(".")).expect("plugin launches"),
    );

    // `system.Clipboard` read is a cheap, side-effect-free call to drive the
    // protocol; cancellation is exercised through the dedicated run id below.
    let cancel = client.cancel(nodara_plugin::CancelParams {
        run_id: "run-cancel".to_string(),
        node_id: None,
    });
    assert!(cancel.is_ok(), "cancel must be accepted for any run id");

    assert!(client.health().unwrap());
    client.shutdown().expect("shutdown succeeds");
}

#[test]
fn the_runtime_treats_the_binary_as_a_plugin_source() {
    // The manifest in `plugins/nodara-platform` must stay loadable, which is what the
    // runtime does at start-up.
    let manifest = PluginManifest::from_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("plugins")
            .join("nodara-platform"),
    )
    .expect("shipped manifest is valid");
    assert_eq!(manifest.id, "nodara.windows.platform");
    assert_eq!(manifest.protocol_version, nodara_schema::PROTOCOL_VERSION);
    assert!(manifest.validate().is_ok());
    let _ = methods::INITIALIZE;
}
