//! Prompt construction.
//!
//! The system prompt is assembled from what the runtime actually reports — the
//! node types it can execute, their configuration schemas and their permissions.
//! The model is therefore never guessing at a node type that does not exist, and
//! installing a plugin changes what the agent can plan without changing this
//! file.

use nodara_schema::{NodeDescriptor, SCHEMA_VERSION};

/// Build the system prompt for workflow generation.
pub fn system_prompt(descriptors: &[NodeDescriptor]) -> String {
    let catalogue = descriptors
        .iter()
        .map(describe_node)
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"You are the planning component of Nodara-Core. You translate an
operator's goal into a workflow document, and you are the only component allowed
to author one.

Reply with a single JSON object and nothing else. The object must have this
shape:

{{
  "schema_version": "{schema_version}",
  "id": "workflow.<short-name>",
  "metadata": {{ "name": "<human name>", "description": "<one sentence>" }},
  "nodes": [
    {{ "id": "<unique id>", "type": "<node type from the catalogue>",
       "config": {{ ... }}, "position": {{ "x": 0, "y": 0 }} }}
  ],
  "edges": [ {{ "id": "<unique id>", "source": "<node id>", "target": "<node id>" }} ],
  "variables": {{ "<name>": {{ "value": <default> }} }}
}}

Hard rules:

1. Exactly one node of type `core.Start` and at least one of type `core.End`.
2. `type` must be copied verbatim from the catalogue below. Never invent one.
3. Every `config` key must appear in that node type's schema; required keys must
   be present. Do not add keys the schema does not list.
4. `edges` must form a directed acyclic graph unless the operator asked for a
   loop. Every edge id must be unique.
5. Reference data between nodes with `{{{{variable_name}}}}` placeholders. A node
   publishes a variable by writing to the `output_var` its schema names.
6. Give every node a `position` so the editor can lay the graph out sensibly.
7. Prefer the smallest graph that accomplishes the goal — but never drop the
   step that observes what the goal depends on. Do not add logging nodes unless
   the operator asked to see progress.
8. You cannot see the screen, the filesystem, the clipboard or the desktop: you
   have no observation channel of your own. Everything you know about this
   machine is either in the catalogue below or produced by a node at run time.
9. Never invent a value that only a run can produce: screen coordinates, pixel
   colours, window titles, window handles, process ids, file contents, clipboard
   contents, match results or timestamps. A literal coordinate in a click, move,
   drag or scroll node is a defect, not a simplification.
10. When the goal depends on what is on screen, plan the observation first:
    capture it (`windows.Desktop.Capture` or `windows.Window.Capture`) and locate
    or read the target with `vision.TemplateMatch` / `vision.Ocr`, then feed the
    coordinate that node published (`center_x`, `center_y`) into the input node.
    The capture node's `output_var` receives an artefact, not a path.
11. `variables` is the operator's override surface. Declare a variable only for
    a value the operator supplies — a template image path, an expected text, a
    target window title — and reference it as `{{name}}`. Never declare a
    variable whose value the workflow itself is supposed to discover, and never
    give one a made-up default: nothing in the plan can know that value.
12. Before sending input to an application, make sure the right window is in
    front: use `windows.Window.Wait` / `windows.Window.Focus` instead of
    assuming the foreground window is already the target.
13. `vision.Ocr` needs an OCR backend the operator configures and returns text
    only; it cannot tell you where on the screen something is.

## Node catalogue

{catalogue}
"#,
        schema_version = SCHEMA_VERSION,
    )
}

fn describe_node(descriptor: &NodeDescriptor) -> String {
    let required = descriptor
        .config_schema
        .get("required")
        .and_then(|value| value.as_array())
        .map(|required| {
            required
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let properties = descriptor
        .config_schema
        .get("properties")
        .and_then(|value| value.as_object())
        .map(|properties| {
            properties
                .iter()
                .map(|(key, schema)| {
                    let kind = schema
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("any");
                    let allowed = schema
                        .get("enum")
                        .and_then(|value| value.as_array())
                        .map(|values| {
                            format!(
                                " one of [{}]",
                                values
                                    .iter()
                                    .filter_map(|value| value.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        })
                        .unwrap_or_default();
                    format!("{key}: {kind}{allowed}")
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();

    let gate = if descriptor.dangerous || !descriptor.permissions.is_empty() {
        format!(
            " [gated: {}]",
            if descriptor.permissions.is_empty() {
                "requires approval".to_string()
            } else {
                descriptor.permissions.join(", ")
            }
        )
    } else {
        String::new()
    };

    format!(
        "- `{}` ({}){gate}\n    {}\n    config: {{{properties}}}\n    required: {{{required}}}",
        descriptor.node_type, descriptor.display_name, descriptor.description,
    )
}

/// Build the user turn for a planning request.
pub fn user_prompt(goal: &str, constraints: &[String]) -> String {
    if constraints.is_empty() {
        return goal.to_string();
    }
    format!(
        "{goal}\n\nConstraints from the operator:\n{}",
        constraints
            .iter()
            .map(|constraint| format!("- {constraint}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Build the repair turn after the runtime rejected a draft.
pub fn repair_prompt(draft: &str, diagnostics: &serde_json::Value) -> String {
    format!(
        "The runtime rejected this workflow document:\n\n{draft}\n\n\
It reported these diagnostics (code, severity, message, path):\n\n{diagnostics}\n\n\
Return a corrected workflow document. Fix every `error` diagnostic. Keep \
everything that was already correct, and reply with the JSON object only."
    )
}

/// Build the user turn for "modify this workflow".
///
/// Modification is a different task from generation: the model must keep
/// everything the operator did not ask to change, including node ids, so that
/// saved positions, external references and diffs stay meaningful.
pub fn modify_prompt(current: &str, instruction: &str) -> String {
    format!(
        "Here is the workflow the operator is currently editing:\n\n{current}\n\n\
Apply this change:\n\n{instruction}\n\n\
Return the whole document, not a patch. Keep every node id that still refers to \
the same step, keep existing `position` values, and keep configuration for nodes \
the change does not touch. Reply with the JSON object only."
    )
}

/// Build the user turn for explaining a workflow and, optionally, a run.
pub fn explain_prompt(
    workflow: Option<&str>,
    diagnostics: Option<&serde_json::Value>,
    run: Option<&serde_json::Value>,
    events: &serde_json::Value,
) -> String {
    let mut out = String::from(
        "Explain the following to the operator. Say what the workflow does, then, \
if a run is included, what happened and why it ended the way it did. Be \
specific: name nodes by their id, quote the diagnostic codes and the log lines \
that matter, and finish with the single most useful next action. Do not \
speculate beyond the evidence given. Plain prose, no JSON.\n\n",
    );
    if let Some(workflow) = workflow {
        out.push_str("## Workflow\n\n```json\n");
        out.push_str(workflow);
        out.push_str("\n```\n\n");
    }
    if let Some(diagnostics) = diagnostics {
        out.push_str("## Validation diagnostics\n\n```json\n");
        out.push_str(&diagnostics.to_string());
        out.push_str("\n```\n\n");
    }
    if let Some(run) = run {
        out.push_str("## Run snapshot\n\n```json\n");
        out.push_str(&run.to_string());
        out.push_str("\n```\n\n");
    }
    if !events.is_null() {
        out.push_str("## Execution events\n\n```json\n");
        out.push_str(&events.to_string());
        out.push_str("\n```\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalogue() -> Vec<NodeDescriptor> {
        vec![
            NodeDescriptor::new("core.Start", "Start", "Core"),
            NodeDescriptor::new("windows.Input.Mouse", "Mouse", "Input"),
            NodeDescriptor::new("vision.TemplateMatch", "Template Match", "Vision"),
        ]
    }

    /// The planner is the only component that authors a workflow, and it has no
    /// way to look at the machine. These rules are what stops it from filling a
    /// click with a coordinate nobody measured.
    #[test]
    fn forbids_values_only_a_run_can_produce() {
        let prompt = system_prompt(&catalogue());
        assert!(prompt.contains("You cannot see the screen"));
        assert!(prompt.contains("Never invent a value that only a run can produce"));
        assert!(prompt.contains("A literal coordinate in a click, move"));
        assert!(prompt.contains("plan the observation first"));
        assert!(prompt.contains("windows.Desktop.Capture"));
        assert!(prompt.contains("vision.TemplateMatch"));
        assert!(prompt.contains("Never declare a\n    variable whose value the workflow itself is supposed to discover"));
        assert!(prompt.contains("windows.Window.Wait` / `windows.Window.Focus"));
        assert!(prompt.contains("it cannot tell you where on the screen something is"));
    }

    #[test]
    fn still_describes_the_installed_catalogue() {
        let prompt = system_prompt(&catalogue());
        assert!(prompt.contains("`vision.TemplateMatch`"));
        assert!(prompt.contains(SCHEMA_VERSION));
    }
}
