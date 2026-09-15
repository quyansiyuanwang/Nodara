//! Window discovery, focus and capture nodes.

use nodara_core::{ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use nodara_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};
use std::time::{Duration, Instant};

use crate::error::PlatformError;
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

/// Matching properties shared by window nodes and input targeting.
pub(crate) fn window_match_properties() -> serde_json::Value {
    serde_json::json!({
        "title": {
            "type": "string",
            "title": "Window title",
            "description": "Window title to match. Substring match unless `exact` is set.",
            "examples": ["Notepad", "Settings"]
        },
        "class": {
            "type": "string",
            "title": "Window class",
            "description": "Win32 window class name to match, e.g. `Notepad`.",
            "examples": ["Notepad"]
        },
        "process": {
            "type": "string",
            "title": "Process name",
            "description": "Executable file name that owns the window, for example `notepad.exe`. Matching is case-insensitive; substring unless `exact` is set.",
            "examples": ["notepad.exe", "chrome.exe"]
        },
        "exact": {
            "type": "boolean",
            "title": "Exact match",
            "description": "Require title, class and process name to match exactly instead of as substrings.",
            "default": false
        },
        "visible_only": {
            "type": "boolean",
            "title": "Visible windows only",
            "description": "Ignore hidden and minimised-away windows while searching.",
            "default": true
        }
    })
}

/// Configuration properties shared by every node that selects a window.
fn selector_properties() -> serde_json::Value {
    let mut properties = window_match_properties();
    if let Some(base) = properties.as_object_mut() {
        base.insert(
            "foreground".to_string(),
            serde_json::json!({
                "type": "boolean",
                "title": "Foreground window",
                "description": "Use the foreground window instead of searching. Overrides title, class and process filters.",
                "default": false
            }),
        );
    }
    properties
}

fn default_visible_only() -> bool {
    true
}

/// A window selector schema plus any node-specific properties.
fn selector_schema(extra: serde_json::Value, required: &[&str]) -> serde_json::Value {
    let mut properties = selector_properties();
    if let (Some(base), Some(extra)) = (properties.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    config_schema(properties, required)
}

/// The `output_var` property shared by nodes that publish their result.
fn output_var_property(description: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "string",
        "title": "Output variable",
        "description": description
    })
}

/// How a window selector resolves to a single window.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WindowSelector {
    /// Window title; substring match unless `exact` is set.
    #[serde(default)]
    pub title: Option<String>,
    /// Window class name.
    #[serde(default)]
    pub class: Option<String>,
    /// Process executable name.
    #[serde(default)]
    pub process: Option<String>,
    /// Require an exact title match.
    #[serde(default)]
    pub exact: bool,
    /// Ignore hidden windows while searching.
    #[serde(default = "default_visible_only")]
    pub visible_only: bool,
    /// Use the foreground window instead of searching.
    #[serde(default)]
    pub foreground: bool,
}

impl WindowSelector {
    /// Parse a selector out of a node configuration.
    ///
    /// A malformed selector is a hard error: falling back to the foreground
    /// window would silently aim focus and capture actions at whatever window
    /// happens to be active. `output_var` is a node-level key that travels in
    /// the same configuration object, so it is tolerated here.
    pub fn from_config(config: &serde_json::Value) -> Result<Self, NodeError> {
        const SELECTOR_KEYS: &[&str] = &[
            "title",
            "class",
            "process",
            "exact",
            "visible_only",
            "foreground",
            "output_var",
            "wait_timeout_ms",
            "poll_interval_ms",
        ];
        let object = config.as_object().ok_or_else(|| {
            NodeError::InvalidConfig("window selector must be an object".to_string())
        })?;
        for key in object.keys() {
            if !SELECTOR_KEYS.contains(&key.as_str()) {
                return Err(NodeError::InvalidConfig(format!(
                    "unknown window selector field `{key}`"
                )));
            }
        }
        serde_json::from_value(config.clone())
            .map_err(|error| NodeError::InvalidConfig(format!("invalid window selector: {error}")))
    }

    /// Human-readable description used in error messages.
    pub fn describe(&self) -> String {
        if self.foreground {
            return "the foreground window".to_string();
        }
        let mut filters = Vec::new();
        if let Some(title) = &self.title {
            filters.push(format!("title `{title}`"));
        }
        if let Some(class) = &self.class {
            filters.push(format!("class `{class}`"));
        }
        if let Some(process) = &self.process {
            filters.push(format!("process `{process}`"));
        }
        if self.visible_only {
            filters.push("visible".to_string());
        }
        if filters.is_empty() {
            "any window".to_string()
        } else {
            filters.join(" and ")
        }
    }
}

fn matches(record: &native::WindowRecord, selector: &WindowSelector) -> bool {
    let title_ok = match &selector.title {
        Some(wanted) if selector.exact => record.title == *wanted,
        Some(wanted) => record.title.contains(wanted.as_str()),
        None => true,
    };
    let class_ok = match &selector.class {
        Some(wanted) if selector.exact => record.class_name == *wanted,
        Some(wanted) => record.class_name.contains(wanted.as_str()),
        None => true,
    };
    let process_ok = match &selector.process {
        Some(wanted) if selector.exact => record.process_name.eq_ignore_ascii_case(wanted),
        Some(wanted) => record
            .process_name
            .to_ascii_lowercase()
            .contains(&wanted.to_ascii_lowercase()),
        None => true,
    };
    title_ok && class_ok && process_ok
}

/// Find exactly one window matching a selector.
pub fn find(selector: &WindowSelector) -> Result<native::WindowRecord, PlatformError> {
    if selector.foreground {
        let id = native::foreground_window();
        return native::windows()
            .into_iter()
            .find(|record| record.id == id)
            .ok_or_else(|| PlatformError::WindowNotFound {
                query: "the foreground window".to_string(),
            });
    }
    native::windows()
        .into_iter()
        .filter(|record| (!selector.visible_only || record.visible) && matches(record, selector))
        .max_by_key(|record| record.rect.width * record.rect.height)
        .ok_or_else(|| PlatformError::WindowNotFound {
            query: selector.describe(),
        })
}

fn window_value(record: &native::WindowRecord) -> serde_json::Value {
    serde_json::json!({
        "handle": record.id,
        "title": record.title,
        "class": record.class_name,
        "process": record.process_name,
        "visible": record.visible,
        "rect": {
            "x": record.rect.x,
            "y": record.rect.y,
            "width": record.rect.width,
            "height": record.rect.height,
        }
    })
}

fn wait_interruptible(context: &ExecutionContext, milliseconds: u64) -> NodeResult<()> {
    let mut remaining = milliseconds;
    while remaining > 0 {
        context.check_cancelled()?;
        let slice = remaining.min(25);
        std::thread::sleep(Duration::from_millis(slice));
        remaining -= slice;
    }
    Ok(())
}

/// `windows.Window.Find`
#[derive(Debug, Default)]
pub struct FindExecutor;

impl NodeExecutor for FindExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any)],
            outputs: vec![port(
                "window",
                "Window",
                PortKind::Output,
                ValueType::Window,
            )],
            config_schema: selector_schema(
                serde_json::json!({
                    "output_var": output_var_property(
                        "Variable receiving the window record (`handle`, `title`, `class`, \
                         `rect`)."
                    )
                }),
                &["output_var"],
            ),
            allows_additional_config: false,
            ..NodeDescriptor::new("windows.Window.Find", "Find Window", "Window")
                .with_description("Locates a window by title, class or process")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let output_var = input.require_str("output_var")?;
        let selector = WindowSelector::from_config(&input.resolved_config)?;
        let record = find(&selector).map_err(|error| NodeError::Execution(error.to_string()))?;
        let value = window_value(&record);
        context.set_variable(output_var, value.clone());
        context.log(
            nodara_schema::LogLevel::Info,
            format!("found window `{}`", record.title),
        );
        Ok(NodeOutput::new().with_output("window", value))
    }
}

/// `windows.Window.Wait`
#[derive(Debug, Default)]
pub struct WaitExecutor;

impl NodeExecutor for WaitExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any)],
            outputs: vec![port(
                "window",
                "Window",
                PortKind::Output,
                ValueType::Window,
            )],
            config_schema: selector_schema(
                serde_json::json!({
                    "output_var": output_var_property(
                        "Variable receiving the first matching window record."
                    ),
                    "wait_timeout_ms": {
                        "type": "integer",
                        "title": "Wait timeout (ms)",
                        "description": "Maximum time to wait for a matching window.",
                        "minimum": 0,
                        "default": 10000
                    },
                    "poll_interval_ms": {
                        "type": "integer",
                        "title": "Poll interval (ms)",
                        "description": "Delay between window enumeration attempts.",
                        "minimum": 1,
                        "default": 100
                    }
                }),
                &["output_var"],
            ),
            allows_additional_config: false,
            ..NodeDescriptor::new("windows.Window.Wait", "Wait for Window", "Window")
                .with_description("Waits until a matching window appears")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let output_var = input.require_str("output_var")?;
        let selector = WindowSelector::from_config(&input.resolved_config)?;
        let timeout_ms = input.config_i64("wait_timeout_ms").unwrap_or(10_000).max(0) as u64;
        let poll_interval_ms = input.config_i64("poll_interval_ms").unwrap_or(100).max(1) as u64;
        let started = Instant::now();
        loop {
            context.check_cancelled()?;
            match find(&selector) {
                Ok(record) => {
                    let value = window_value(&record);
                    context.set_variable(output_var, value.clone());
                    context.log(
                        nodara_schema::LogLevel::Info,
                        format!("window appeared: `{}`", record.title),
                    );
                    return Ok(NodeOutput::new().with_output("window", value));
                }
                Err(PlatformError::WindowNotFound { .. })
                    if started.elapsed() < Duration::from_millis(timeout_ms) =>
                {
                    let remaining = timeout_ms.saturating_sub(started.elapsed().as_millis() as u64);
                    wait_interruptible(context, poll_interval_ms.min(remaining))?;
                }
                Err(PlatformError::WindowNotFound { .. }) => return Err(NodeError::Timeout),
                Err(error) => return Err(NodeError::Execution(error.to_string())),
            }
        }
    }
}

/// `windows.Window.Focus`
#[derive(Debug, Default)]
pub struct FocusExecutor;

impl NodeExecutor for FocusExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any)],
            outputs: vec![port("out", "Out", PortKind::Output, ValueType::Window)],
            config_schema: selector_schema(serde_json::json!({}), &[]),
            dangerous: true,
            permissions: vec!["window.control".to_string()],
            allows_additional_config: false,
            ..NodeDescriptor::new("windows.Window.Focus", "Focus Window", "Window")
                .with_description("Brings a window to the foreground")
        }
    }

    fn execute(&self, input: NodeInput, _context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let selector = WindowSelector::from_config(&input.resolved_config)?;
        let record = find(&selector).map_err(|error| NodeError::Execution(error.to_string()))?;
        native::focus(record.id).map_err(|error| NodeError::Execution(error.to_string()))?;
        Ok(NodeOutput::new().with_output(
            "out",
            serde_json::json!({ "handle": record.id, "title": record.title }),
        ))
    }
}

/// `windows.Window.Capture`
#[derive(Debug, Default)]
pub struct CaptureExecutor;

impl NodeExecutor for CaptureExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any)],
            outputs: vec![port(
                "artifact",
                "Artifact",
                PortKind::Output,
                ValueType::Image,
            )],
            config_schema: selector_schema(
                serde_json::json!({
                    "output_var": output_var_property(
                        "Variable receiving the captured artefact metadata."
                    )
                }),
                &["output_var"],
            ),
            dangerous: false,
            permissions: vec!["screen.capture".to_string()],
            allows_additional_config: false,
            ..NodeDescriptor::new("windows.Window.Capture", "Capture Window", "Window")
                .with_description("Captures a window, including its frame")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let output_var = input.require_str("output_var")?;
        let selector = WindowSelector::from_config(&input.resolved_config)?;
        let record = find(&selector).map_err(|error| NodeError::Execution(error.to_string()))?;
        let meta = crate::capture::capture_region(
            context,
            record.rect.x,
            record.rect.y,
            record.rect.width,
            record.rect.height,
            &record.title,
        )?;
        context.set_variable(output_var, serde_json::to_value(&meta).unwrap_or_default());
        Ok(NodeOutput::new()
            .with_output("artifact", serde_json::to_value(meta).unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_selector_parses() {
        let selector = WindowSelector::from_config(&serde_json::json!({
            "title": "Notepad", "process": "notepad.exe", "exact": true
        }))
        .unwrap();
        assert_eq!(selector.title.as_deref(), Some("Notepad"));
        assert_eq!(selector.process.as_deref(), Some("notepad.exe"));
        assert!(selector.exact);
        assert!(selector.visible_only);
        assert!(!selector.foreground);
    }

    #[test]
    fn a_malformed_selector_is_an_error_not_a_fallback() {
        // A typo must not silently redirect focus/capture at the foreground
        // window.
        let error = WindowSelector::from_config(&serde_json::json!({ "tittle": "Notepad" }))
            .expect_err("unknown fields must be rejected");
        assert_eq!(error.code(), "E_INVALID_CONFIG");

        let error = WindowSelector::from_config(&serde_json::json!({ "exact": "yes" }))
            .expect_err("wrong types must be rejected");
        assert_eq!(error.code(), "E_INVALID_CONFIG");
    }

    #[test]
    fn a_selector_matches_the_window_process_name() {
        let record = native::WindowRecord {
            id: 1,
            title: "Untitled - Notepad".to_string(),
            class_name: "Notepad".to_string(),
            process_name: "notepad.exe".to_string(),
            rect: native::Rect {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            visible: true,
        };
        let selector = WindowSelector::from_config(&serde_json::json!({
            "process": "notepad",
            "visible_only": true
        }))
        .unwrap();
        assert!(matches(&record, &selector));

        let excluded = WindowSelector::from_config(&serde_json::json!({
            "process": "chrome.exe",
            "exact": true
        }))
        .unwrap();
        assert!(!matches(&record, &excluded));
    }

    #[test]
    fn wait_descriptor_exposes_timeout_and_poll_controls() {
        let descriptor = WaitExecutor.descriptor();
        assert_eq!(descriptor.node_type, "windows.Window.Wait");
        assert!(descriptor.config_schema["properties"]["wait_timeout_ms"].is_object());
        assert!(descriptor.config_schema["properties"]["poll_interval_ms"].is_object());
        assert!(descriptor.permissions.is_empty());
        assert!(WindowSelector::from_config(&serde_json::json!({
            "process": "notepad.exe",
            "wait_timeout_ms": 500,
            "poll_interval_ms": 25,
            "output_var": "window"
        }))
        .is_ok());
    }
}
