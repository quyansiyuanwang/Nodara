//! Window discovery, focus and capture nodes.

use rf_core::{ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use rf_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

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

/// How a window selector resolves to a single window.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WindowSelector {
    /// Window title; substring match unless `exact` is set.
    #[serde(default)]
    pub title: Option<String>,
    /// Window class name.
    #[serde(default)]
    pub class: Option<String>,
    /// Require an exact title match.
    #[serde(default)]
    pub exact: bool,
    /// Use the foreground window instead of searching.
    #[serde(default)]
    pub foreground: bool,
}

impl WindowSelector {
    /// Parse a selector out of a node configuration.
    pub fn from_config(config: &serde_json::Value) -> Self {
        serde_json::from_value(config.clone()).unwrap_or(Self {
            title: None,
            class: None,
            exact: false,
            foreground: true,
        })
    }

    /// Human-readable description used in error messages.
    pub fn describe(&self) -> String {
        if self.foreground {
            return "the foreground window".to_string();
        }
        match (&self.title, &self.class) {
            (Some(title), Some(class)) => format!("title `{title}` and class `{class}`"),
            (Some(title), None) => format!("title `{title}`"),
            (None, Some(class)) => format!("class `{class}`"),
            (None, None) => "any visible window".to_string(),
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
    title_ok && class_ok
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
        .filter(|record| record.visible && matches(record, selector))
        .max_by_key(|record| record.rect.width * record.rect.height)
        .ok_or_else(|| PlatformError::WindowNotFound {
            query: selector.describe(),
        })
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
            config_schema: config_schema(
                serde_json::json!({
                    "title": { "type": "string" },
                    "class": { "type": "string" },
                    "exact": { "type": "boolean", "default": false },
                    "foreground": { "type": "boolean", "default": false },
                    "output_var": { "type": "string" }
                }),
                &["output_var"],
            ),
            allows_additional_config: false,
            ..NodeDescriptor::new("windows.Window.Find", "Find Window", "Window")
                .with_description("Locates a window by title or class")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let output_var = input.require_str("output_var")?;
        let selector = WindowSelector::from_config(&input.resolved_config);
        let record = find(&selector).map_err(|error| NodeError::Execution(error.to_string()))?;
        let value = serde_json::json!({
            "handle": record.id,
            "title": record.title,
            "class": record.class_name,
            "rect": {
                "x": record.rect.x,
                "y": record.rect.y,
                "width": record.rect.width,
                "height": record.rect.height,
            }
        });
        context.set_variable(output_var, value.clone());
        context.log(
            rf_schema::LogLevel::Info,
            format!("found window `{}`", record.title),
        );
        Ok(NodeOutput::new().with_output("window", value))
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
            config_schema: config_schema(
                serde_json::json!({
                    "title": { "type": "string" },
                    "class": { "type": "string" },
                    "exact": { "type": "boolean", "default": false },
                    "foreground": { "type": "boolean", "default": false }
                }),
                &[],
            ),
            dangerous: true,
            permissions: vec!["window.control".to_string()],
            allows_additional_config: false,
            ..NodeDescriptor::new("windows.Window.Focus", "Focus Window", "Window")
                .with_description("Brings a window to the foreground")
        }
    }

    fn execute(&self, input: NodeInput, _context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let selector = WindowSelector::from_config(&input.resolved_config);
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
            config_schema: config_schema(
                serde_json::json!({
                    "title": { "type": "string" },
                    "class": { "type": "string" },
                    "exact": { "type": "boolean", "default": false },
                    "foreground": { "type": "boolean", "default": false },
                    "output_var": { "type": "string" }
                }),
                &["output_var"],
            ),
            dangerous: false,
            permissions: vec!["screen.capture".to_string()],
            allows_additional_config: false,
            ..NodeDescriptor::new("windows.Window.Capture", "Capture Window", "Window")
                .with_description("Captures a window's client area")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let output_var = input.require_str("output_var")?;
        let selector = WindowSelector::from_config(&input.resolved_config);
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
