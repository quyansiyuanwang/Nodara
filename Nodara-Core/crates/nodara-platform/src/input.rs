//! Keyboard, mouse and text node executors.

use std::time::Duration;

use nodara_core::{ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use nodara_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

use crate::keys::{self, KeyStroke};
use crate::native;
use crate::window::{self, WindowSelector};

fn port(name: &str, display: &str, kind: PortKind, value_type: ValueType) -> PortDescriptor {
    PortDescriptor::new(name, display, kind, value_type)
}

fn input_window_properties() -> serde_json::Value {
    let mut properties = serde_json::json!({
        "focus": {
            "type": "boolean",
            "title": "Focus target window",
            "description": "Find and focus a target window before sending input. When false, input goes to the current foreground window.",
            "default": false
        }
    });
    if let (Some(base), Some(shared)) = (
        properties.as_object_mut(),
        window::window_match_properties().as_object(),
    ) {
        for (key, value) in shared {
            base.insert(key.clone(), value.clone());
        }
    }
    properties
}

fn config_schema(mut properties: serde_json::Value, required: &[&str]) -> serde_json::Value {
    if let (Some(base), Some(extra)) = (
        properties.as_object_mut(),
        input_window_properties().as_object(),
    ) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn focus_target(input: &NodeInput) -> NodeResult<()> {
    if !input.config_bool("focus").unwrap_or(false) {
        return Ok(());
    }
    let selector = WindowSelector {
        title: input.config_str("title").map(str::to_string),
        class: input.config_str("class").map(str::to_string),
        process: input.config_str("process").map(str::to_string),
        exact: input.config_bool("exact").unwrap_or(false),
        visible_only: input.config_bool("visible_only").unwrap_or(true),
        foreground: false,
    };
    if selector.title.is_none() && selector.class.is_none() && selector.process.is_none() {
        return Err(NodeError::InvalidConfig(
            "`focus` requires a target window `title`, `class` or `process`".to_string(),
        ));
    }
    let record =
        window::find(&selector).map_err(|error| NodeError::Execution(error.to_string()))?;
    native::focus(record.id).map_err(|error| NodeError::Execution(error.to_string()))?;
    Ok(())
}

fn dangerous_descriptor(descriptor: NodeDescriptor, permission: &str) -> NodeDescriptor {
    NodeDescriptor {
        dangerous: true,
        permissions: vec![permission.to_string()],
        ..descriptor
    }
}

fn key_down(key: KeyStroke) {
    if key.shift {
        native::key(keys::vk::SHIFT, false);
    }
    native::key(key.virtual_key, false);
}

fn key_up(key: KeyStroke) {
    native::key(key.virtual_key, true);
    if key.shift {
        native::key(keys::vk::SHIFT, true);
    }
}

fn tap(context: &ExecutionContext, key: KeyStroke, hold_ms: u64) -> NodeResult<()> {
    key_down(key);
    let wait = wait_interruptible(context, hold_ms);
    key_up(key);
    wait
}

fn parse_mouse_button(value: &str) -> NodeResult<native::MouseButton> {
    match value.to_ascii_lowercase().as_str() {
        "left" => Ok(native::MouseButton::Left),
        "right" => Ok(native::MouseButton::Right),
        "middle" => Ok(native::MouseButton::Middle),
        other => Err(NodeError::InvalidConfig(format!(
            "unknown mouse button `{other}`"
        ))),
    }
}

fn mouse_button_name(button: native::MouseButton) -> &'static str {
    match button {
        native::MouseButton::Left => "left",
        native::MouseButton::Right => "right",
        native::MouseButton::Middle => "middle",
    }
}

fn cursor_position() -> NodeResult<(i32, i32)> {
    native::cursor_position().map_err(|error| NodeError::Execution(error.to_string()))
}

fn configured_point(input: &NodeInput, x_key: &str, y_key: &str) -> NodeResult<Option<(i32, i32)>> {
    match (input.config_i64(x_key), input.config_i64(y_key)) {
        (Some(x), Some(y)) => Ok(Some((x as i32, y as i32))),
        (None, None) => Ok(None),
        _ => Err(NodeError::InvalidConfig(format!(
            "`{x_key}` and `{y_key}` must be supplied together"
        ))),
    }
}

fn resolve_point(origin: (i32, i32), point: (i32, i32), relative: bool) -> (i32, i32) {
    if relative {
        (
            origin.0.saturating_add(point.0),
            origin.1.saturating_add(point.1),
        )
    } else {
        point
    }
}

fn wait_interruptible(context: &ExecutionContext, duration_ms: u64) -> NodeResult<()> {
    let mut remaining = duration_ms;
    while remaining > 0 {
        context.check_cancelled()?;
        let slice = remaining.min(25);
        std::thread::sleep(Duration::from_millis(slice));
        remaining -= slice;
    }
    Ok(())
}

fn move_cursor_smooth(
    context: &ExecutionContext,
    from: (i32, i32),
    to: (i32, i32),
    duration_ms: u64,
) -> NodeResult<()> {
    if from == to {
        return Ok(());
    }
    let steps = (duration_ms / 10).max(1);
    let step_delay = duration_ms / steps;
    for step in 1..=steps {
        context.check_cancelled()?;
        let ratio = step as f64 / steps as f64;
        let x = from.0 as f64 + (to.0 - from.0) as f64 * ratio;
        let y = from.1 as f64 + (to.1 - from.1) as f64 * ratio;
        native::set_cursor(x.round() as i32, y.round() as i32)
            .map_err(|error| NodeError::Execution(error.to_string()))?;
        if step < steps && step_delay > 0 {
            std::thread::sleep(Duration::from_millis(step_delay));
        }
    }
    Ok(())
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
                        },
                        "action": {
                            "type": "string",
                            "title": "Action",
                            "description": "Type taps and releases the chord. Press holds it down; Release releases a previously held chord.",
                            "enum": ["type", "press", "release"],
                            "default": "type"
                        },
                        "hold_ms": {
                            "type": "integer",
                            "title": "Hold duration (ms)",
                            "description": "For the Type action, time between key-down and key-up.",
                            "minimum": 0,
                            "default": 0
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
        let action = input.config_str("action").unwrap_or("type");
        let hold_ms = input.config_i64("hold_ms").unwrap_or(0).max(0) as u64;
        context.check_cancelled()?;
        focus_target(&input)?;
        let (modifiers, stroke) = keys::parse_chord(&chord)
            .ok_or_else(|| NodeError::InvalidConfig(format!("unknown key chord `{chord}`")))?;
        match action {
            "type" => {
                for modifier in &modifiers {
                    native::key(*modifier, false);
                }
                let tap_result = tap(context, stroke, hold_ms);
                for modifier in modifiers.iter().rev() {
                    native::key(*modifier, true);
                }
                tap_result?;
            }
            "press" => {
                for modifier in &modifiers {
                    native::key(*modifier, false);
                }
                key_down(stroke);
            }
            "release" => {
                key_up(stroke);
                for modifier in modifiers.iter().rev() {
                    native::key(*modifier, true);
                }
            }
            other => {
                return Err(NodeError::InvalidConfig(format!(
                    "unknown keyboard action `{other}`"
                )))
            }
        }
        context.log(
            nodara_schema::LogLevel::Info,
            format!("keyboard {action} `{chord}`"),
        );
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
        focus_target(&input)?;
        for character in text.chars() {
            context.check_cancelled()?;
            let stroke = keys::resolve_char(character).ok_or_else(|| {
                NodeError::Unsupported(format!("character `{character}` cannot be typed"))
            })?;
            tap(context, stroke, 0)?;
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
                            "enum": ["move", "click", "double_click", "right_click", "middle_click", "down", "up", "drag"],
                            "default": "click"
                        },
                        "button": {
                            "type": "string",
                            "title": "Mouse button",
                            "description": "Button used for click, double-click, down, up and drag actions.",
                            "enum": ["left", "right", "middle"],
                            "default": "left"
                        },
                        "x": {
                            "type": "integer",
                            "title": "X",
                            "description": "Target X in pixels. Relative to the current cursor when `relative` is enabled. For drag this is the destination X."
                        },
                        "y": {
                            "type": "integer",
                            "title": "Y",
                            "description": "Target Y in pixels. Relative to the current cursor when `relative` is enabled. For drag this is the destination Y."
                        },
                        "relative": {
                            "type": "boolean",
                            "title": "Relative movement",
                            "description": "Treat X/Y as offsets from the current cursor instead of absolute screen coordinates.",
                            "default": false
                        },
                        "duration_ms": {
                            "type": "integer",
                            "title": "Mouse duration (ms)",
                            "description": "Hold time for clicks or movement time for drag. Drag defaults to 300 ms when omitted.",
                            "minimum": 0,
                            "default": 0
                        },
                        "double_click_interval_ms": {
                            "type": "integer",
                            "title": "Double-click interval (ms)",
                            "description": "Delay between the two clicks of a double-click action.",
                            "minimum": 0,
                            "default": 100
                        },
                        "start_x": {
                            "type": "integer",
                            "title": "Start X",
                            "description": "Optional drag start X. Omit to start at the current cursor position."
                        },
                        "start_y": {
                            "type": "integer",
                            "title": "Start Y",
                            "description": "Optional drag start Y. Omit to start at the current cursor position."
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
        let duration_ms = input.config_i64("duration_ms").unwrap_or(0).max(0) as u64;
        let relative = input.config_bool("relative").unwrap_or(false);
        let double_click_interval_ms = input
            .config_i64("double_click_interval_ms")
            .unwrap_or(100)
            .max(0) as u64;
        context.check_cancelled()?;
        focus_target(&input)?;

        let requested_button = input
            .config_str("button")
            .map(parse_mouse_button)
            .transpose()?;
        let (button, count) = match action.as_str() {
            "move" => (None, 0),
            "click" => (
                Some(requested_button.unwrap_or(native::MouseButton::Left)),
                1,
            ),
            "double_click" => (
                Some(requested_button.unwrap_or(native::MouseButton::Left)),
                2,
            ),
            "right_click" => (
                Some(requested_button.unwrap_or(native::MouseButton::Right)),
                1,
            ),
            "middle_click" => (
                Some(requested_button.unwrap_or(native::MouseButton::Middle)),
                1,
            ),
            "down" | "up" => (
                Some(requested_button.unwrap_or(native::MouseButton::Left)),
                0,
            ),
            "drag" => (
                Some(requested_button.unwrap_or(native::MouseButton::Left)),
                0,
            ),
            other => {
                return Err(NodeError::InvalidConfig(format!(
                    "unknown mouse action `{other}`"
                )))
            }
        };

        let origin = cursor_position()?;
        let target =
            configured_point(&input, "x", "y")?.map(|point| resolve_point(origin, point, relative));
        let mut moved = false;

        if let Some(button) = button {
            let map = |error: crate::PlatformError| NodeError::Execution(error.to_string());
            match action.as_str() {
                "move" => {
                    if let Some(target) = target {
                        move_cursor_smooth(context, origin, target, duration_ms)?;
                        moved = true;
                    }
                }
                "drag" => {
                    let start = configured_point(&input, "start_x", "start_y")?.unwrap_or(origin);
                    if start != origin {
                        native::set_cursor(start.0, start.1).map_err(map)?;
                    }
                    let delta = configured_point(&input, "x", "y")?.ok_or_else(|| {
                        NodeError::InvalidConfig(
                            "`drag` requires destination `x` and `y`".to_string(),
                        )
                    })?;
                    let destination = resolve_point(start, delta, relative);
                    let drag_duration = if duration_ms == 0 { 300 } else { duration_ms };
                    native::mouse_button(button, true).map_err(map)?;
                    let movement = move_cursor_smooth(context, start, destination, drag_duration);
                    let release = native::mouse_button(button, false).map_err(map);
                    movement?;
                    release?;
                    moved = true;
                }
                "down" => {
                    if let Some(target) = target {
                        native::set_cursor(target.0, target.1).map_err(map)?;
                        moved = true;
                    }
                    native::mouse_button(button, true).map_err(map)?;
                }
                "up" => {
                    if let Some(target) = target {
                        native::set_cursor(target.0, target.1).map_err(map)?;
                        moved = true;
                    }
                    native::mouse_button(button, false).map_err(map)?;
                }
                _ => {
                    if let Some(target) = target {
                        native::set_cursor(target.0, target.1).map_err(map)?;
                        moved = true;
                    }
                    for index in 0..count {
                        native::mouse_button(button, true).map_err(map)?;
                        let wait = wait_interruptible(context, duration_ms);
                        let release = native::mouse_button(button, false).map_err(map);
                        wait?;
                        release?;
                        if index + 1 < count && double_click_interval_ms > 0 {
                            wait_interruptible(context, double_click_interval_ms)?;
                        }
                    }
                }
            }
        }

        let final_cursor = cursor_position().unwrap_or(origin);
        let cursor = native::foreground_window();
        Ok(NodeOutput::new().with_output(
            "out",
            serde_json::json!({
                "action": action,
                "button": button.map(mouse_button_name),
                "origin_x": origin.0,
                "origin_y": origin.1,
                "x": final_cursor.0,
                "y": final_cursor.1,
                "duration_ms": duration_ms,
                "moved": moved,
                "foreground": cursor,
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(config: serde_json::Value) -> NodeInput {
        NodeInput {
            node_id: "input".to_string(),
            node_type: "windows.Input.Keyboard".to_string(),
            config: config.clone(),
            resolved_config: config,
            inputs: Default::default(),
            timeout_ms: None,
        }
    }

    #[test]
    fn focus_is_optional() {
        focus_target(&input(serde_json::json!({ "keys": "enter" }))).unwrap();
    }

    #[test]
    fn focus_requires_a_selector() {
        let error = focus_target(&input(serde_json::json!({ "focus": true })))
            .expect_err("focus without a window selector must fail");
        assert_eq!(error.code(), "E_INVALID_CONFIG");
    }

    #[test]
    fn mouse_buttons_and_relative_points_are_resolved() {
        assert_eq!(
            parse_mouse_button("LEFT").unwrap(),
            native::MouseButton::Left
        );
        assert_eq!(
            parse_mouse_button("middle").unwrap(),
            native::MouseButton::Middle
        );
        assert!(parse_mouse_button("back").is_err());
        assert_eq!(resolve_point((10, 20), (5, -3), true), (15, 17));
        assert_eq!(resolve_point((10, 20), (5, -3), false), (5, -3));
    }

    #[test]
    fn mouse_coordinates_must_be_supplied_together() {
        let missing_y = input(serde_json::json!({ "action": "click", "x": 10 }));
        let error = configured_point(&missing_y, "x", "y")
            .expect_err("one coordinate without the other must fail");
        assert_eq!(error.code(), "E_INVALID_CONFIG");
        assert_eq!(
            configured_point(&input(serde_json::json!({ "x": 10, "y": 20 })), "x", "y").unwrap(),
            Some((10, 20))
        );
    }
}
