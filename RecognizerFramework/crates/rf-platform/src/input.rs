//! Keyboard, mouse and text node executors.

use std::time::Duration;

use rf_core::{ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use rf_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

use crate::keys::{self, KeyStroke};
use crate::native;

fn port(name: &str, display: &str, kind: PortKind, value_type: ValueType) -> PortDescriptor {
    PortDescriptor::new(name, display, kind, value_type)
}

fn config_schema(properties: serde_json::Value, required: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn dangerous_descriptor(descriptor: NodeDescriptor, permission: &str) -> NodeDescriptor {
    NodeDescriptor {
        dangerous: true,
        permissions: vec![permission.to_string()],
        ..descriptor
    }
}

fn press(key: KeyStroke) {
    if key.shift {
        native::key(keys::vk::SHIFT, false);
    }
    native::key(key.virtual_key, false);
    native::key(key.virtual_key, true);
    if key.shift {
        native::key(keys::vk::SHIFT, true);
    }
}

/// `windows.Input.Keyboard`
#[derive(Debug, Default)]
pub struct KeyboardExecutor;

impl NodeExecutor for KeyboardExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        dangerous_descriptor(
            NodeDescriptor {
                inputs: vec![port("in", "In", PortKind::Input, ValueType::Any)],
                outputs: vec![port("out", "Out", PortKind::Output, ValueType::String)],
                config_schema: config_schema(
                    serde_json::json!({
                        "keys": {
                            "type": "string",
                            "title": "Key chord",
                            "description": "Key or chord to send to the focused window. \
                                            Modifiers and keys are joined with `+`.",
                            "examples": ["ctrl+shift+s", "win+i", "enter"]
                        }
                    }),
                    &["keys"],
                ),
                allows_additional_config: false,
                ..NodeDescriptor::new("windows.Input.Keyboard", "Keyboard", "Input")
                    .with_description("Sends a key or key chord to the focused window")
            },
            "input.control",
        )
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let chord = input.require_str("keys")?;
        context.check_cancelled()?;
        let (modifiers, stroke) = keys::parse_chord(&chord)
            .ok_or_else(|| NodeError::InvalidConfig(format!("unknown key chord `{chord}`")))?;
        for modifier in &modifiers {
            native::key(*modifier, false);
        }
        press(stroke);
        for modifier in modifiers.iter().rev() {
            native::key(*modifier, true);
        }
        context.log(rf_schema::LogLevel::Info, format!("sent keys `{chord}`"));
        Ok(NodeOutput::new().with_output("out", serde_json::json!(chord)))
    }
}

/// `windows.Input.Text`
#[derive(Debug, Default)]
pub struct TextExecutor;

impl NodeExecutor for TextExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        dangerous_descriptor(
            NodeDescriptor {
                inputs: vec![port("in", "In", PortKind::Input, ValueType::Any)],
                outputs: vec![port("out", "Out", PortKind::Output, ValueType::String)],
                config_schema: config_schema(
                    serde_json::json!({
                        "text": {
                            "type": "string",
                            "title": "Text",
                            "description": "Literal text to type into the focused window."
                        },
                        "interval_ms": {
                            "type": "integer",
                            "title": "Key interval (ms)",
                            "minimum": 0,
                            "default": 10,
                            "description": "Delay between keystrokes. `0` types as fast as \
                                            the window accepts input."
                        }
                    }),
                    &["text"],
                ),
                allows_additional_config: false,
                ..NodeDescriptor::new("windows.Input.Text", "Text", "Input")
                    .with_description("Types literal text into the focused window")
            },
            "input.control",
        )
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let text = input.require_str("text")?;
        let interval = input.config_i64("interval_ms").unwrap_or(10).max(0) as u64;
        for character in text.chars() {
            context.check_cancelled()?;
            let name = character.to_string();
            let stroke = keys::resolve(&name).ok_or_else(|| {
                NodeError::Unsupported(format!("character `{character}` cannot be typed"))
            })?;
            press(stroke);
            if interval > 0 {
                std::thread::sleep(Duration::from_millis(interval));
            }
        }
        Ok(NodeOutput::new().with_output("out", serde_json::json!(text)))
    }
}

/// `windows.Input.Mouse`
#[derive(Debug, Default)]
pub struct MouseExecutor;

impl NodeExecutor for MouseExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        dangerous_descriptor(
            NodeDescriptor {
                inputs: vec![port("in", "In", PortKind::Input, ValueType::Any)],
                outputs: vec![port("out", "Out", PortKind::Output, ValueType::Object)],
                config_schema: config_schema(
                    serde_json::json!({
                        "action": {
                            "type": "string",
                            "title": "Action",
                            "description": "Mouse action to perform.",
                            "enum": ["move", "click", "double_click", "right_click", "middle_click", "down", "up"],
                            "default": "click"
                        },
                        "x": {
                            "type": "integer",
                            "title": "X",
                            "description": "Absolute screen X in pixels. Omitted for `click` \
                                            and `move` keeps the current position."
                        },
                        "y": {
                            "type": "integer",
                            "title": "Y",
                            "description": "Absolute screen Y in pixels. Omitted for `click` \
                                            and `move` keeps the current position."
                        }
                    }),
                    &["action"],
                ),
                allows_additional_config: false,
                ..NodeDescriptor::new("windows.Input.Mouse", "Mouse", "Input")
                    .with_description("Moves the cursor and synthesises mouse buttons")
            },
            "input.control",
        )
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let action = input.require_str("action")?;
        context.check_cancelled()?;

        let moved = match (input.config_i64("x"), input.config_i64("y")) {
            (Some(x), Some(y)) => {
                native::set_cursor(x as i32, y as i32)
                    .map_err(|error| NodeError::Execution(error.to_string()))?;
                true
            }
            _ => false,
        };

        let (button, count) = match action.as_str() {
            "move" => (None, 0),
            "click" => (Some(native::MouseButton::Left), 1),
            "double_click" => (Some(native::MouseButton::Left), 2),
            "right_click" => (Some(native::MouseButton::Right), 1),
            "middle_click" => (Some(native::MouseButton::Middle), 1),
            "down" => (Some(native::MouseButton::Left), 0),
            "up" => (Some(native::MouseButton::Left), 0),
            other => {
                return Err(NodeError::InvalidConfig(format!(
                    "unknown mouse action `{other}`"
                )))
            }
        };

        if let Some(button) = button {
            let map = |error: crate::PlatformError| NodeError::Execution(error.to_string());
            match action.as_str() {
                "down" => native::mouse_button(button, true).map_err(map)?,
                "up" => native::mouse_button(button, false).map_err(map)?,
                _ => {
                    for _ in 0..count {
                        native::mouse_button(button, true).map_err(map)?;
                        native::mouse_button(button, false).map_err(map)?;
                    }
                }
            }
        }

        let cursor = native::foreground_window();
        Ok(NodeOutput::new().with_output(
            "out",
            serde_json::json!({ "action": action, "moved": moved, "foreground": cursor }),
        ))
    }
}
