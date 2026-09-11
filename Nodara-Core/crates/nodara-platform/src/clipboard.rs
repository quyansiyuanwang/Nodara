//! Clipboard node executor.

use nodara_core::{ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use nodara_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

use crate::native;

/// `system.Clipboard`
#[derive(Debug, Default)]
pub struct ClipboardExecutor;

impl NodeExecutor for ClipboardExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![PortDescriptor::new(
                "in",
                "In",
                PortKind::Input,
                ValueType::Any,
            )],
            outputs: vec![PortDescriptor::new(
                "out",
                "Out",
                PortKind::Output,
                ValueType::String,
            )],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "title": "Action",
                        "description": "Read the clipboard into the run, or replace its text.",
                        "enum": ["read", "write"],
                        "default": "read"
                    },
                    "text": {
                        "type": "string",
                        "title": "Text",
                        "description": "Text to place on the clipboard. Required when \
                                        `action` is `write`; supports `{{variable}}` \
                                        interpolation."
                    },
                    "output_var": {
                        "type": "string",
                        "title": "Output variable",
                        "description": "Variable receiving the clipboard text when reading."
                    }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
            dangerous: true,
            permissions: vec!["clipboard".to_string()],
            allows_additional_config: false,
            ..NodeDescriptor::new("system.Clipboard", "Clipboard", "System")
                .with_description("Reads or replaces the clipboard text")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let action = input.require_str("action")?;
        match action.as_str() {
            "read" => {
                let text = native::clipboard_read()
                    .map_err(|error| NodeError::Execution(error.to_string()))?
                    .unwrap_or_default();
                if let Some(output_var) = input.config_str("output_var") {
                    context.set_variable(output_var, serde_json::json!(text));
                }
                Ok(NodeOutput::new().with_output("out", serde_json::json!(text)))
            }
            "write" => {
                let text = input.require_str("text")?;
                native::clipboard_write(&text)
                    .map_err(|error| NodeError::Execution(error.to_string()))?;
                Ok(NodeOutput::new().with_output("out", serde_json::json!(text)))
            }
            other => Err(NodeError::InvalidConfig(format!(
                "unknown clipboard action `{other}`"
            ))),
        }
    }
}
