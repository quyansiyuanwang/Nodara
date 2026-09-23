//! Values a workflow cannot know yet.
//!
//! The runtime validator answers "is this document well formed", and it answers
//! it well: every node type exists, every edge points at a node, every variable
//! is declared. It cannot answer "does this workflow mean anything", because
//! `x = 960` is a perfectly valid coordinate.
//!
//! That gap is how a plan can validate, run, report success and still be
//! fiction: the model fills a click with a pixel position nothing in the run
//! ever measured. This module reports the one shape that can only come from a
//! guess — an **input position read from a variable no node in the workflow
//! writes** — so the planner can be asked, once, to derive it from an
//! observation instead.
//!
//! The review is deliberately narrow and deliberately non-blocking: a false
//! report costs one extra planning attempt, a hard block would break legitimate
//! workflows (an operator-supplied coordinate, a position taken from a
//! screenshot the operator attached in a previous turn).

use std::collections::BTreeSet;

use nodara_schema::Workflow;

/// Configuration fields that place something on screen.
const POSITION_FIELDS: &[&str] = &[
    "x",
    "y",
    "to_x",
    "to_y",
    "from_x",
    "from_y",
    "start_x",
    "start_y",
    "end_x",
    "end_y",
    "center_x",
    "center_y",
    "position_x",
    "position_y",
];

/// One reason a plan needs another look before it is run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Finding {
    /// Node that consumes the value.
    pub node_id: String,
    /// Configuration field that consumes it.
    pub field: String,
    /// Variable it is read from.
    pub variable: String,
    /// Operator-facing, and appended verbatim to the next planning attempt.
    pub message: String,
}

/// Node types that drive whatever is currently on screen.
fn is_input_node(node_type: &str) -> bool {
    node_type.starts_with("windows.Input.") || node_type.starts_with("vision.")
}

/// The constraint appended to the next planning attempt, or `None` when the plan
/// is informed.
pub fn observation_constraint(workflow: &Workflow) -> Option<String> {
    let findings = uninformed_inputs(workflow);
    if findings.is_empty() {
        return None;
    }
    let mut out = String::from(
        "The runtime accepted the draft, but nothing in it can produce these values:\n",
    );
    for finding in &findings {
        out.push_str("- ");
        out.push_str(&finding.message);
        out.push('\n');
    }
    out.push_str(
        "A value that only a run can observe — a screen position, a match, a window handle — \
         must come from an observation step in the same workflow: capture the screen and locate \
         the target with vision.TemplateMatch (or read it with vision.Ocr), then pass the \
         coordinate that node published. Values the operator supplies belong in `variables` and \
         must be documented as such.",
    );
    Some(out)
}

/// Input positions that no node in `workflow` produces.
///
/// A variable that is referenced but not declared is left to the runtime's own
/// `WF150` diagnostic; a variable that is declared but never written is the case
/// this review exists for.
pub fn uninformed_inputs(workflow: &Workflow) -> Vec<Finding> {
    let written = written_variables(workflow);
    let mut findings = Vec::new();
    let mut seen = BTreeSet::new();
    for node in &workflow.nodes {
        if !is_input_node(&node.node_type) {
            continue;
        }
        let serde_json::Value::Object(config) = &node.config else {
            continue;
        };
        for field in POSITION_FIELDS {
            let Some(serde_json::Value::String(raw)) = config.get(*field) else {
                continue;
            };
            for variable in referenced_variables(raw) {
                if written.contains(&variable) || !workflow.variables.contains_key(&variable) {
                    continue;
                }
                if !seen.insert((node.id.clone(), (*field).to_string(), variable.clone())) {
                    continue;
                }
                let default = workflow
                    .variables
                    .get(&variable)
                    .map(|declared| declared.value.to_string())
                    .unwrap_or_else(|| "null".to_string());
                findings.push(Finding {
                    node_id: node.id.clone(),
                    field: (*field).to_string(),
                    variable: variable.clone(),
                    message: format!(
                        "`{}` takes `{}` from `{}{}{}` (declared as {}), and no node in this \
                         workflow writes that variable: the value is a guess, not an observation.",
                        node.id, field, "{{", variable, "}}", default,
                    ),
                });
            }
        }
    }
    findings
}

/// Variables a node publishes into the run scope.
///
/// The set is intentionally generous — `output_var`, its suffixed variants, and
/// the `name` a `core.SetVariable` or a `core.CalculateMany` expression writes.
/// Matching too much only means fewer reviews, and a missed review leaves the
/// workflow exactly as it would have been before this module existed.
fn written_variables(workflow: &Workflow) -> BTreeSet<String> {
    let mut written = BTreeSet::new();
    for node in &workflow.nodes {
        collect_writes(&node.config, &mut written);
    }
    written
}

fn collect_writes(value: &serde_json::Value, written: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let writes = key == "output_var"
                    || key.starts_with("output_var_")
                    || key == "variable"
                    || key == "variables"
                    || key == "target_var"
                    || key == "name";
                if writes {
                    collect_names(child, written);
                }
                collect_writes(child, written);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_writes(item, written);
            }
        }
        _ => {}
    }
}

fn collect_names(value: &serde_json::Value, written: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::String(name) => {
            let name = name.trim();
            if !name.is_empty() && !name.contains("{{") {
                written.insert(name.to_string());
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_names(item, written);
            }
        }
        _ => {}
    }
}

/// `{{name}}` references inside one configuration value.
fn referenced_variables(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        if !name.is_empty() {
            found.push(name.to_string());
        }
        rest = &after[end + 2..];
    }
    found
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn workflow(value: serde_json::Value) -> Workflow {
        serde_json::from_value(value).expect("workflow parses")
    }

    /// The document an operator actually got back: a click whose position is a
    /// variable the plan made up and nothing ever measured.
    #[test]
    fn reports_a_click_position_nothing_produces() {
        let document = workflow(json!({
            "schema_version": "2.1",
            "id": "workflow.mudp3a4t",
            "nodes": [
                { "id": "start", "type": "core.Start" },
                { "id": "capture", "type": "windows.Desktop.Capture", "config": { "output_var": "screenshot" } },
                {
                    "id": "click_genshin",
                    "type": "windows.Input.Mouse",
                    "config": {
                        "action": "click",
                        "x": "{{genshin_taskbar_x}}",
                        "y": "{{genshin_taskbar_y}}"
                    }
                },
                { "id": "end", "type": "core.End" }
            ],
            "variables": {
                "genshin_taskbar_x": { "value": 960 },
                "genshin_taskbar_y": { "value": 1060 }
            }
        }));

        let findings = uninformed_inputs(&document);
        assert_eq!(findings.len(), 2, "got {findings:?}");
        assert_eq!(findings[0].field, "x");
        assert_eq!(findings[0].variable, "genshin_taskbar_x");
        assert_eq!(findings[0].node_id, "click_genshin");
        assert!(findings[0].message.contains("960"));
        assert!(observation_constraint(&document)
            .expect("a constraint")
            .contains("vision.TemplateMatch"));
    }

    /// The corrected shape: the position comes from the match the workflow ran.
    #[test]
    fn accepts_a_position_a_match_publishes() {
        let document = workflow(json!({
            "schema_version": "2.1",
            "id": "workflow.informed",
            "nodes": [
                { "id": "start", "type": "core.Start" },
                { "id": "capture", "type": "windows.Desktop.Capture", "config": { "output_var": "screenshot" } },
                {
                    "id": "find",
                    "type": "vision.TemplateMatch",
                    "config": { "frame": "{{screenshot.artifact}}", "template": "{{icon_template}}", "output_var": "match" }
                },
                {
                    "id": "click",
                    "type": "windows.Input.Mouse",
                    "config": { "action": "click", "x": "{{match.center_x}}", "y": "{{match.center_y}}" }
                },
                { "id": "end", "type": "core.End" }
            ],
            "variables": { "icon_template": { "value": "C:/icons/genshin.png" } }
        }));

        assert!(uninformed_inputs(&document).is_empty());
        assert!(observation_constraint(&document).is_none());
    }

    /// A variable that is referenced but never declared is the validator's job,
    /// not this review's.
    #[test]
    fn leaves_undeclared_variables_to_the_runtime_validator() {
        let document = workflow(json!({
            "schema_version": "2.1",
            "id": "workflow.undeclared",
            "nodes": [
                { "id": "start", "type": "core.Start" },
                {
                    "id": "click",
                    "type": "windows.Input.Mouse",
                    "config": { "x": "{{nobody_declared_this}}", "y": 10 }
                }
            ],
            "variables": {}
        }));

        assert!(uninformed_inputs(&document).is_empty());
    }

    /// A node that publishes the value is enough, whatever writes it.
    #[test]
    fn accepts_a_position_a_set_variable_node_writes() {
        let document = workflow(json!({
            "schema_version": "2.1",
            "id": "workflow.operator",
            "nodes": [
                { "id": "start", "type": "core.Start" },
                { "id": "set", "type": "core.SetVariable", "config": { "name": "target_x", "value": 120 } },
                {
                    "id": "click",
                    "type": "windows.Input.Mouse",
                    "config": { "x": "{{target_x}}", "y": 40 }
                }
            ],
            "variables": { "target_x": { "value": 0 } }
        }));

        assert!(uninformed_inputs(&document).is_empty());
    }

    /// Literal numbers are the operator's business, not this review's: an
    /// operator may know the coordinate, and a model that read a screenshot from
    /// a previous turn may legitimately quote one.
    #[test]
    fn ignores_literal_positions() {
        let document = workflow(json!({
            "schema_version": "2.1",
            "id": "workflow.literal",
            "nodes": [
                { "id": "start", "type": "core.Start" },
                {
                    "id": "click",
                    "type": "windows.Input.Mouse",
                    "config": { "x": 960, "y": 1060 }
                }
            ],
            "variables": {}
        }));

        assert!(uninformed_inputs(&document).is_empty());
    }

    #[test]
    fn finds_every_reference_in_one_value() {
        assert_eq!(referenced_variables("{{a}} + {{ b }}"), vec!["a", "b"]);
        assert_eq!(referenced_variables("{{unterminated"), Vec::<String>::new());
        assert_eq!(referenced_variables("plain"), Vec::<String>::new());
    }
}
