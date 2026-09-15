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

fn input_window_properties(allow_background: bool) -> serde_json::Value {
    let mut properties = serde_json::json!({
        "focus": {
            "type": "boolean",
            "title": "Focus target window",
            "description": "Find and focus a target window before sending input. When false, input goes to the current foreground window.",
            "default": false
        }
    });
    if allow_background {
        if let Some(base) = properties.as_object_mut() {
            base.insert(
                "background".to_string(),
                serde_json::json!({
                    "type": "boolean",
                    "title": "Background input",
                    "description": "Send input messages to a matching window without changing focus. Requires a title, class or process selector and cannot be combined with focus.",
                    "default": false
                }),
            );
        }
    }
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

fn config_schema(
    mut properties: serde_json::Value,
    required: &[&str],
    allow_background: bool,
) -> serde_json::Value {
    if let (Some(base), Some(extra)) = (
        properties.as_object_mut(),
        input_window_properties(allow_background).as_object(),
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

#[derive(Debug)]
struct InputTarget {
    window: native::WindowId,
    background: bool,
}

fn input_target(input: &NodeInput) -> NodeResult<Option<InputTarget>> {
    let background = input.config_bool("background").unwrap_or(false);
    let focus = input.config_bool("focus").unwrap_or(false);
    if background && focus {
        return Err(NodeError::InvalidConfig(
            "`background` and `focus` cannot both be enabled".to_string(),
        ));
    }
    if !background && !focus {
        return Ok(None);
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
            "targeted input requires a window `title`, `class` or `process`".to_string(),
        ));
    }
    let record =
        window::find(&selector).map_err(|error| NodeError::Execution(error.to_string()))?;
    if focus {
        native::focus(record.id).map_err(|error| NodeError::Execution(error.to_string()))?;
    }
    Ok(Some(InputTarget {
        window: record.id,
        background,
    }))
}

fn send_mouse_move(
    window: native::WindowId,
    point: (i32, i32),
    held: Option<native::MouseButton>,
) -> NodeResult<()> {
    native::send_mouse_move(window, point.0, point.1, held)
        .map_err(|error| NodeError::Execution(error.to_string()))
}

fn send_mouse_button(
    window: native::WindowId,
    point: (i32, i32),
    button: native::MouseButton,
    down: bool,
) -> NodeResult<()> {
    native::send_mouse_button(window, point.0, point.1, button, down)
        .map_err(|error| NodeError::Execution(error.to_string()))
}

fn move_mouse_smooth_background(
    context: &ExecutionContext,
    window: native::WindowId,
    from: (i32, i32),
    to: (i32, i32),
    duration_ms: u64,
    held: Option<native::MouseButton>,
) -> NodeResult<()> {
    if from == to {
        send_mouse_move(window, to, held)?;
        return Ok(());
    }
    let steps = (duration_ms / 10).max(1);
    let step_delay = duration_ms / steps;
    for step in 1..=steps {
        context.check_cancelled()?;
        let ratio = step as f64 / steps as f64;
        let point = (
            (from.0 as f64 + (to.0 - from.0) as f64 * ratio).round() as i32,
            (from.1 as f64 + (to.1 - from.1) as f64 * ratio).round() as i32,
        );
        send_mouse_move(window, point, held)?;
        if step < steps && step_delay > 0 {
            std::thread::sleep(Duration::from_millis(step_delay));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn execute_background_mouse(
    context: &ExecutionContext,
    input: &NodeInput,
    target: &InputTarget,
    action: &str,
    button: Option<native::MouseButton>,
    count: usize,
    duration_ms: u64,
    relative: bool,
    click_interval_ms: u64,
) -> NodeResult<NodeOutput> {
    let screen_cursor = cursor_position()?;
    let origin = native::screen_to_client(target.window, screen_cursor.0, screen_cursor.1)
        .map_err(|error| NodeError::Execution(error.to_string()))?;
    let target_point =
        configured_point(input, "x", "y")?.map(|point| resolve_point(origin, point, relative));
    let mut moved = false;

    if let Some(button) = button {
        match action {
            "move" => {
                if let Some(point) = target_point {
                    move_mouse_smooth_background(
                        context,
                        target.window,
                        origin,
                        point,
                        duration_ms,
                        None,
                    )?;
                    moved = true;
                }
            }
            "drag" => {
                let start = configured_point(input, "start_x", "start_y")?.unwrap_or(origin);
                let delta = configured_point(input, "x", "y")?.ok_or_else(|| {
                    NodeError::InvalidConfig("`drag` requires destination `x` and `y`".to_string())
                })?;
                let destination = resolve_point(start, delta, relative);
                let drag_duration = if duration_ms == 0 { 300 } else { duration_ms };
                send_mouse_move(target.window, start, None)?;
                send_mouse_button(target.window, start, button, true)?;
                let movement = move_mouse_smooth_background(
                    context,
                    target.window,
                    start,
                    destination,
                    drag_duration,
                    Some(button),
                );
                let release = send_mouse_button(target.window, destination, button, false);
                movement?;
                release?;
                moved = true;
            }
            "down" => {
                let point = target_point.unwrap_or(origin);
                send_mouse_move(target.window, point, None)?;
                send_mouse_button(target.window, point, button, true)?;
                moved = point != origin;
            }
            "up" => {
                let point = target_point.unwrap_or(origin);
                send_mouse_move(target.window, point, None)?;
                send_mouse_button(target.window, point, button, false)?;
                moved = point != origin;
            }
            _ => {
                let point = target_point.unwrap_or(origin);
                send_mouse_move(target.window, point, None)?;
                moved = point != origin;
                for index in 0..count {
                    send_mouse_button(target.window, point, button, true)?;
                    let wait = wait_interruptible(context, duration_ms);
                    let release = send_mouse_button(target.window, point, button, false);
                    wait?;
                    release?;
                    if index + 1 < count && click_interval_ms > 0 {
                        wait_interruptible(context, click_interval_ms)?;
                    }
                }
            }
        }
    }

    Ok(NodeOutput::new().with_output(
        "out",
        serde_json::json!({
            "action": action,
            "button": button.map(mouse_button_name),
            "origin_x": origin.0,
            "origin_y": origin.1,
            "x": target_point.map_or(origin.0, |point| point.0),
            "y": target_point.map_or(origin.1, |point| point.1),
            "duration_ms": duration_ms,
            "click_count": count,
            "click_interval_ms": click_interval_ms,
            "moved": moved,
            "background": true,
            "coordinate_space": "client",
        }),
    ))
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

fn post_key(window: native::WindowId, virtual_key: u8, down: bool) -> NodeResult<()> {
    native::send_key(window, virtual_key, down)
        .map_err(|error| NodeError::Execution(error.to_string()))
}

fn background_keyboard_action(
    context: &ExecutionContext,
    target: &InputTarget,
    modifiers: &[u8],
    stroke: KeyStroke,
    action: &str,
    hold_ms: u64,
) -> NodeResult<()> {
    let mut failure = None;
    match action {
        "type" => {
            for modifier in modifiers {
                if let Err(error) = post_key(target.window, *modifier, false) {
                    failure = Some(error);
                    break;
                }
            }
            if failure.is_none() && stroke.shift {
                failure = post_key(target.window, keys::vk::SHIFT, false).err();
            }
            if failure.is_none() {
                failure = post_key(target.window, stroke.virtual_key, false).err();
            }
            if failure.is_none() {
                failure = wait_interruptible(context, hold_ms).err();
            }
            let _ = post_key(target.window, stroke.virtual_key, true);
            if stroke.shift {
                let _ = post_key(target.window, keys::vk::SHIFT, true);
            }
            for modifier in modifiers.iter().rev() {
                let _ = post_key(target.window, *modifier, true);
            }
        }
        "press" => {
            for modifier in modifiers {
                if let Err(error) = post_key(target.window, *modifier, false) {
                    failure = Some(error);
                    break;
                }
            }
            if failure.is_none() && stroke.shift {
                failure = post_key(target.window, keys::vk::SHIFT, false).err();
            }
            if failure.is_none() {
                failure = post_key(target.window, stroke.virtual_key, false).err();
            }
            if failure.is_some() {
                let _ = post_key(target.window, stroke.virtual_key, true);
                if stroke.shift {
                    let _ = post_key(target.window, keys::vk::SHIFT, true);
                }
                for modifier in modifiers.iter().rev() {
                    let _ = post_key(target.window, *modifier, true);
                }
            }
        }
        "release" => {
            let _ = post_key(target.window, stroke.virtual_key, true);
            if stroke.shift {
                let _ = post_key(target.window, keys::vk::SHIFT, true);
            }
            for modifier in modifiers.iter().rev() {
                let _ = post_key(target.window, *modifier, true);
            }
        }
        other => {
            return Err(NodeError::InvalidConfig(format!(
                "unknown keyboard action `{other}`"
            )))
        }
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
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
                        },
                        "repeat": {
                            "type": "integer",
                            "title": "Repeat count",
                            "description": "Number of times to send this key or chord. Only supported for the Type action.",
                            "minimum": 1,
                            "maximum": 100000,
                            "default": 1
                        },
                        "repeat_interval_ms": {
                            "type": "integer",
                            "title": "Repeat interval (ms)",
                            "description": "Delay between repeated Type actions. Cancellation remains responsive.",
                            "minimum": 0,
                            "default": 0
                        }
                    }),
                    &["keys"],
                    true,
                ),
                allows_additional_config: false,
                ..NodeDescriptor::new("windows.Input.Keyboard", "Keyboard", "Input")
                    .with_description("Sends a key or key chord to the focused window")
            },
            "input.control",
        )
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let input = input.with_input_object_fallback("in");
        let chord = input.require_str("keys")?;
        let action = input.config_str("action").unwrap_or("type");
        let hold_ms = input.config_i64("hold_ms").unwrap_or(0).max(0) as u64;
        let repeat = input.config_i64("repeat").unwrap_or(1).max(1) as u64;
        let repeat_interval_ms = input.config_i64("repeat_interval_ms").unwrap_or(0).max(0) as u64;
        if action != "type" && repeat != 1 {
            return Err(NodeError::InvalidConfig(
                "`repeat` is only supported with the Type action".to_string(),
            ));
        }
        context.check_cancelled()?;
        let target = input_target(&input)?;
        let (modifiers, stroke) = keys::parse_chord(&chord)
            .ok_or_else(|| NodeError::InvalidConfig(format!("unknown key chord `{chord}`")))?;
        for iteration in 0..repeat {
            context.check_cancelled()?;
            if let Some(target) = target.as_ref().filter(|target| target.background) {
                background_keyboard_action(context, target, &modifiers, stroke, action, hold_ms)?;
            } else {
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
            }
            if iteration + 1 < repeat {
                wait_interruptible(context, repeat_interval_ms)?;
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
                        },
                        "background_method": {
                            "type": "string",
                            "title": "Background method",
                            "description": "How background text is delivered. `wm_char` sends one character message at a time; `set_text` replaces the window text directly; `clipboard` pastes through the clipboard and restores its previous value.",
                            "enum": ["wm_char", "set_text", "clipboard"],
                            "default": "wm_char"
                        },
                        "paste_delay_ms": {
                            "type": "integer",
                            "title": "Clipboard paste delay (ms)",
                            "description": "Time allowed for the target to process Ctrl+V before the previous clipboard value is restored.",
                            "minimum": 0,
                            "default": 100
                        }
                    }),
                    &["text"],
                    true,
                ),
                allows_additional_config: false,
                ..NodeDescriptor::new("windows.Input.Text", "Text", "Input")
                    .with_description("Types literal text into the focused window")
            },
            "input.control",
        )
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let input = input
            .with_input_object_fallback("in")
            .with_input_fallback("in", "text");
        let text = input.require_str("text")?;
        let interval = input.config_i64("interval_ms").unwrap_or(10).max(0) as u64;
        let target = input_target(&input)?;
        if let Some(target) = target.filter(|target| target.background) {
            match input.config_str("background_method").unwrap_or("wm_char") {
                "wm_char" => {
                    for character in text.chars() {
                        context.check_cancelled()?;
                        let mut units = [0u16; 2];
                        for unit in character.encode_utf16(&mut units) {
                            native::send_text(target.window, *unit)
                                .map_err(|error| NodeError::Execution(error.to_string()))?;
                        }
                        wait_interruptible(context, interval)?;
                    }
                }
                "set_text" => native::set_window_text(target.window, &text)
                    .map_err(|error| NodeError::Execution(error.to_string()))?,
                "clipboard" => {
                    let previous = native::clipboard_read()
                        .map_err(|error| NodeError::Execution(error.to_string()))?;
                    native::clipboard_write(&text)
                        .map_err(|error| NodeError::Execution(error.to_string()))?;
                    let paste_target = InputTarget {
                        window: target.window,
                        background: true,
                    };
                    let (modifiers, stroke) = keys::parse_chord("ctrl+v")
                        .ok_or_else(|| NodeError::Execution("internal ctrl+v chord".to_string()))?;
                    let send = background_keyboard_action(
                        context,
                        &paste_target,
                        &modifiers,
                        stroke,
                        "type",
                        0,
                    );
                    let delay = input.config_i64("paste_delay_ms").unwrap_or(100).max(0) as u64;
                    let wait = wait_interruptible(context, delay);
                    let restore = previous
                        .map_or_else(
                            || native::clipboard_write(""),
                            |value| native::clipboard_write(&value),
                        )
                        .map_err(|error| NodeError::Execution(error.to_string()));
                    send?;
                    wait?;
                    restore?;
                }
                other => {
                    return Err(NodeError::InvalidConfig(format!(
                        "unknown background text method `{other}`"
                    )))
                }
            }
        } else {
            for character in text.chars() {
                context.check_cancelled()?;
                let stroke = keys::resolve_char(character).ok_or_else(|| {
                    NodeError::Unsupported(format!("character `{character}` cannot be typed"))
                })?;
                tap(context, stroke, 0)?;
                wait_interruptible(context, interval)?;
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
                        "click_count": {
                            "type": "integer",
                            "title": "Click count",
                            "description": "Number of times click, right_click or middle_click repeats. Double-click always sends two clicks.",
                            "minimum": 1,
                            "maximum": 100000,
                            "default": 1
                        },
                        "click_interval_ms": {
                            "type": "integer",
                            "title": "Click interval (ms)",
                            "description": "Delay between repeated clicks. Falls back to double_click_interval_ms when omitted.",
                            "minimum": 0,
                            "default": 100
                        },
                        "double_click_interval_ms": {
                            "type": "integer",
                            "title": "Double-click interval (ms)",
                            "description": "Legacy fallback delay between clicks when click_interval_ms is omitted.",
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
                    true,
                ),
                allows_additional_config: false,
                ..NodeDescriptor::new("windows.Input.Mouse", "Mouse", "Input")
                    .with_description("Moves the cursor and synthesises mouse buttons")
            },
            "input.control",
        )
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let input = input.with_input_object_fallback("in");
        let action = input.require_str("action")?;
        let duration_ms = input.config_i64("duration_ms").unwrap_or(0).max(0) as u64;
        let relative = input.config_bool("relative").unwrap_or(false);
        let click_interval_ms = input
            .config_i64("click_interval_ms")
            .or_else(|| input.config_i64("double_click_interval_ms"))
            .unwrap_or(100)
            .max(0) as u64;
        let click_count = input.config_i64("click_count").unwrap_or(1).max(1) as usize;
        context.check_cancelled()?;
        let target = input_target(&input)?;
        let requested_button = input
            .config_str("button")
            .map(parse_mouse_button)
            .transpose()?;
        let (button, count) = match action.as_str() {
            "move" => (None, 0),
            "click" => (
                Some(requested_button.unwrap_or(native::MouseButton::Left)),
                click_count,
            ),
            "double_click" => (
                Some(requested_button.unwrap_or(native::MouseButton::Left)),
                2,
            ),
            "right_click" => (
                Some(requested_button.unwrap_or(native::MouseButton::Right)),
                click_count,
            ),
            "middle_click" => (
                Some(requested_button.unwrap_or(native::MouseButton::Middle)),
                click_count,
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

        if let Some(target) = target.filter(|target| target.background) {
            let screen_cursor = cursor_position()?;
            let result = execute_background_mouse(
                context,
                &input,
                &target,
                &action,
                button,
                count,
                duration_ms,
                relative,
                click_interval_ms,
            );
            let restore = native::set_cursor(screen_cursor.0, screen_cursor.1)
                .map_err(|error| NodeError::Execution(error.to_string()));
            return match result {
                Ok(output) => {
                    restore?;
                    Ok(output)
                }
                Err(error) => {
                    let _ = restore;
                    Err(error)
                }
            };
        }

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
                        if index + 1 < count && click_interval_ms > 0 {
                            wait_interruptible(context, click_interval_ms)?;
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
                "click_count": count,
                "click_interval_ms": click_interval_ms,
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
        input_target(&input(serde_json::json!({ "keys": "enter" }))).unwrap();
    }

    #[test]
    fn focus_requires_a_selector() {
        let error = input_target(&input(serde_json::json!({ "focus": true })))
            .expect_err("focus without a window selector must fail");
        assert_eq!(error.code(), "E_INVALID_CONFIG");
    }

    #[test]
    fn background_and_focus_are_mutually_exclusive() {
        let error = input_target(&input(serde_json::json!({
            "background": true,
            "focus": true,
            "process": "notepad.exe"
        })))
        .expect_err("conflicting input targeting must fail");
        assert_eq!(error.code(), "E_INVALID_CONFIG");

        let error = input_target(&input(serde_json::json!({ "background": true })))
            .expect_err("background input requires a selector");
        assert_eq!(error.code(), "E_INVALID_CONFIG");
    }

    #[test]
    fn mouse_descriptor_exposes_background_client_input() {
        let schema = MouseExecutor.descriptor().config_schema;
        assert!(schema["properties"]["background"].is_object());
        assert!(schema["properties"]["relative"].is_object());
        assert!(schema["properties"]["duration_ms"].is_object());
        assert!(schema["properties"]["click_count"].is_object());
        assert!(schema["properties"]["click_interval_ms"].is_object());
    }

    #[test]
    fn keyboard_descriptor_exposes_repetition_controls() {
        let schema = KeyboardExecutor.descriptor().config_schema;
        assert!(schema["properties"]["repeat"].is_object());
        assert!(schema["properties"]["repeat_interval_ms"].is_object());
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
