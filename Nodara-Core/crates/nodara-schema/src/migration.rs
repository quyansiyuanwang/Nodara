//! Migration of legacy workflow documents to the current schema.
//!
//! Earlier revisions of the project shipped two incompatible shapes:
//!
//! * a "kind as bare string" form (`{"kind": "System.Log", ...}`)
//! * a "kind as tagged object" form (`{"kind": {"type": "system.log"}}`)
//!
//! Both are upgraded here rather than in the runtime, so that every consumer
//! can rely on a single canonical model.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::{SchemaError, SchemaResult};
use crate::version::SCHEMA_VERSION;
use crate::workflow::{Edge, Metadata, Node, Variable, Workflow};

/// Summary of what a migration changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MigrationReport {
    /// Version detected in the source document.
    pub from: String,
    /// Version produced.
    pub to: String,
    /// Human-readable notes about transformations applied.
    pub notes: Vec<String>,
    /// Whether anything actually changed.
    pub changed: bool,
}

/// Map a legacy node kind onto the canonical namespaced node type.
pub fn canonical_node_type(kind: &str) -> String {
    let normalised = kind.trim();
    let lower = normalised.to_ascii_lowercase();
    match lower.as_str() {
        "start" | "core.start" => "core.Start".to_string(),
        "exit" | "end" | "stop" | "core.end" => "core.End".to_string(),
        "system.log" | "systemlog" | "log" | "core.log" => "core.Log".to_string(),
        "system.delay" | "systemdelay" | "delay" | "wait" => "system.Delay".to_string(),
        "system.paste" | "systempaste" | "paste" | "clipboard.read" => "system.Paste".to_string(),
        "calculate" | "core.calculate" | "math" => "core.Calculate".to_string(),
        "input.keyboard" | "keyboard" | "windows.input.keyboard" => {
            "windows.Input.Keyboard".to_string()
        }
        "input.mouse" | "mouse" | "windows.input.mouse" => "windows.Input.Mouse".to_string(),
        "input.text" | "text" | "windows.input.text" => "windows.Input.Text".to_string(),
        "window.find" | "windows.window.find" | "findwindow" => "windows.Window.Find".to_string(),
        "window.capture" | "windows.window.capture" => "windows.Window.Capture".to_string(),
        "desktop.capture" | "screenshot" | "windows.desktop.capture" => {
            "windows.Desktop.Capture".to_string()
        }
        "vision.ocr" | "ocr" => "vision.Ocr".to_string(),
        "agent.decision" | "decision" => "agent.Decision".to_string(),
        _ => {
            // Already canonical (contains a dot and a namespace we do not remap).
            normalised.to_string()
        }
    }
}

/// Normalise a node's `kind`/`type` value into a node type string.
fn node_type_from_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(canonical_node_type(s)),
        serde_json::Value::Object(map) => map
            .get("type")
            .and_then(|v| v.as_str())
            .map(canonical_node_type),
        _ => None,
    }
}

fn normalise_config(value: Option<&serde_json::Value>) -> serde_json::Value {
    match value {
        Some(serde_json::Value::Object(_)) => value.cloned().unwrap(),
        Some(serde_json::Value::String(_)) | None => {
            serde_json::Value::Object(serde_json::Map::new())
        }
        Some(other) => serde_json::json!({ "value": other.clone() }),
    }
}

/// Migrate any recognised legacy document to the current schema version.
///
/// A document already at the current schema version is returned unchanged with a
/// report whose `changed` flag is `false`.
pub fn migrate(value: serde_json::Value) -> SchemaResult<(Workflow, MigrationReport)> {
    let object = value
        .as_object()
        .ok_or_else(|| SchemaError::Migration("document root must be a JSON object".to_string()))?;

    let declared = object
        .get("schema_version")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            object.get("version").map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
        })
        .unwrap_or_else(|| "1".to_string());

    if crate::version::is_compatible_schema_version(&declared)
        && object.contains_key("schema_version")
        && object.contains_key("nodes")
    {
        let workflow: Workflow = serde_json::from_value(value)?;
        return Ok((
            workflow,
            MigrationReport {
                from: declared,
                to: SCHEMA_VERSION.to_string(),
                notes: vec!["document already used the current schema".to_string()],
                changed: false,
            },
        ));
    }

    let mut notes = Vec::new();
    let id = object
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("workflow.migrated")
        .to_string();
    let name = object
        .get("name")
        .or_else(|| object.get("metadata").and_then(|m| m.get("name")))
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_string();
    let description = object
        .get("description")
        .or_else(|| object.get("metadata").and_then(|m| m.get("description")))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let mut workflow = Workflow {
        // A migrated document keeps pointing an editor at the same schema.
        schema_url: object
            .get("$schema")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        schema_version: SCHEMA_VERSION.to_string(),
        id,
        metadata: Metadata {
            name,
            description,
            ..Metadata::default()
        },
        nodes: Vec::new(),
        edges: Vec::new(),
        variables: BTreeMap::new(),
    };

    let raw_nodes = object
        .get("nodes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if raw_nodes.is_empty() {
        return Err(SchemaError::Migration(
            "source document contains no nodes".to_string(),
        ));
    }

    for (index, raw) in raw_nodes.iter().enumerate() {
        let node_object = raw.as_object().ok_or_else(|| {
            SchemaError::Migration(format!("node at index {index} is not a JSON object"))
        })?;
        let node_id = node_object
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("node{index}"));

        let kind_value = node_object
            .get("type")
            .or_else(|| node_object.get("kind"))
            .ok_or_else(|| {
                SchemaError::Migration(format!("node `{node_id}` has neither `type` nor `kind`"))
            })?;
        let node_type = node_type_from_value(kind_value).ok_or_else(|| {
            SchemaError::Migration(format!("node `{node_id}` has an unrecognised kind"))
        })?;

        if node_type.starts_with("core.")
            || node_type.starts_with("system.")
            || node_type.starts_with("windows.")
            || node_type.starts_with("vision.")
            || node_type.starts_with("agent.")
        {
            // Recognised namespace; nothing to note beyond the mapping itself.
        } else {
            notes.push(format!(
                "node `{node_id}` kept its original type `{node_type}`"
            ));
        }

        let mut node = Node::new(node_id.clone(), node_type);
        node.label = node_object
            .get("label")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        node.config = normalise_config(node_object.get("config"));
        normalise_legacy_config(&node.node_type, &mut node.config);
        workflow.nodes.push(node);
    }
    notes.push("migrated nodes to the namespaced node-type convention".to_string());

    let raw_edges = object
        .get("edges")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for (index, raw) in raw_edges.iter().enumerate() {
        let edge_object = raw.as_object().ok_or_else(|| {
            SchemaError::Migration(format!("edge at index {index} is not a JSON object"))
        })?;
        let source = edge_object
            .get("source")
            .or_else(|| edge_object.get("from"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SchemaError::Migration(format!("edge at index {index} has no source"))
            })?;
        let target = edge_object
            .get("target")
            .or_else(|| edge_object.get("to"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SchemaError::Migration(format!("edge at index {index} has no target"))
            })?;
        let edge_id = edge_object
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("e{}", index + 1));

        let mut edge = Edge::new(edge_id, source, target);
        edge.condition = edge_object
            .get("condition")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        edge.label = edge_object
            .get("label")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        workflow.edges.push(edge);
    }
    if !raw_edges.is_empty() {
        notes.push("migrated edges to `source`/`target` field names".to_string());
    }

    if let Some(raw_variables) = object.get("variables").and_then(|v| v.as_object()) {
        for (key, raw) in raw_variables {
            let variable = match raw {
                serde_json::Value::Object(map)
                    if map.contains_key("value")
                        || map.contains_key("description")
                        || map.contains_key("secret") =>
                {
                    serde_json::from_value::<Variable>(raw.clone()).unwrap_or(Variable {
                        value: raw.clone(),
                        ..Variable::default()
                    })
                }
                other => Variable {
                    value: other.clone(),
                    ..Variable::default()
                },
            };
            workflow.variables.insert(key.clone(), variable);
        }
    }

    Ok((
        workflow,
        MigrationReport {
            from: declared,
            to: SCHEMA_VERSION.to_string(),
            notes,
            changed: true,
        },
    ))
}

/// Rewrite configuration keys that changed meaning between revisions.
///
/// The legacy `System.Delay` node took `seconds`; the current `system.Delay`
/// takes `duration_ms`. Converting here keeps migration a pure data operation
/// that the runtime never has to know about.
fn normalise_legacy_config(node_type: &str, config: &mut serde_json::Value) {
    let Some(map) = config.as_object_mut() else {
        return;
    };
    if node_type == "system.Delay" && !map.contains_key("duration_ms") {
        if let Some(seconds) = map.remove("seconds").and_then(|value| value.as_f64()) {
            let millis = (seconds * 1000.0).round().max(0.0) as i64;
            map.insert("duration_ms".to_string(), serde_json::json!(millis));
        }
    }
    if node_type == "core.Log" && !map.contains_key("message") {
        if let Some(text) = map.remove("text") {
            map.insert("message".to_string(), text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_legacy_kinds_to_namespaces() {
        assert_eq!(canonical_node_type("Start"), "core.Start");
        assert_eq!(canonical_node_type("System.Log"), "core.Log");
        assert_eq!(canonical_node_type("system.log"), "core.Log");
        assert_eq!(
            canonical_node_type("Input.Keyboard"),
            "windows.Input.Keyboard"
        );
        assert_eq!(
            canonical_node_type("windows.Window.Find"),
            "windows.Window.Find"
        );
    }

    #[test]
    fn migrates_kind_as_string_document() {
        let legacy = serde_json::json!({
            "version": 2,
            "id": "legacy.hello",
            "nodes": [
                {"id": "start", "kind": "Start", "config": {}},
                {"id": "log", "kind": "System.Log", "config": {"message": "hi"}}
            ],
            "edges": [{"from": "start", "to": "log"}]
        });
        let (workflow, report) = migrate(legacy).expect("migrates");
        assert!(report.changed);
        assert_eq!(workflow.schema_version, SCHEMA_VERSION);
        assert_eq!(workflow.nodes[0].node_type, "core.Start");
        assert_eq!(workflow.nodes[1].node_type, "core.Log");
        assert_eq!(workflow.edges[0].source, "start");
        assert_eq!(workflow.edges[0].id, "e1");
    }

    #[test]
    fn migrates_tagged_kind_object() {
        let legacy = serde_json::json!({
            "version": "2.0",
            "id": "legacy.tagged",
            "nodes": [
                {"id": "start", "kind": {"type": "start"}, "config": "Start"},
                {"id": "log", "kind": {"type": "system.log"}, "config": {"message": "hi"}}
            ],
            "edges": [{"source": "start", "target": "log"}]
        });
        let (workflow, _) = migrate(legacy).expect("migrates");
        assert_eq!(workflow.nodes[0].node_type, "core.Start");
        assert!(workflow.nodes[0].config.is_object());
        assert_eq!(workflow.nodes[1].node_type, "core.Log");
    }

    #[test]
    fn current_documents_are_left_untouched() {
        let mut wf = Workflow::new("wf.current");
        wf.add_node(Node::new("start", "core.Start"));
        let value = serde_json::to_value(&wf).unwrap();
        let (migrated, report) = migrate(value).expect("migrates");
        assert!(!report.changed);
        assert_eq!(migrated, wf);
    }

    #[test]
    fn a_legacy_schema_reference_survives_migration() {
        let legacy = serde_json::json!({
            "$schema": "../Nodara-Core/schema/workflow.schema.json",
            "version": 1,
            "id": "legacy.hinted",
            "nodes": [{"id": "start", "kind": "Start", "config": {}}],
            "edges": []
        });
        let (workflow, report) = migrate(legacy).expect("migrates");
        assert!(report.changed);
        assert_eq!(
            workflow.schema_url.as_deref(),
            Some("../Nodara-Core/schema/workflow.schema.json")
        );
    }
}
