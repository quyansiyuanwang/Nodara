//! The built-in `core.*` and `system.*` node types.
//!
//! These ship with the engine because every workflow needs them and none of them
//! touch the operating system. Anything host-specific (input, windows, OCR) lives
//! in plugins, per the architecture document.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use nodara_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

use crate::context::ExecutionContext;
use crate::error::{NodeError, NodeResult};
use crate::executor::{NodeExecutor, NodeInput, NodeOutput};
use crate::expr::evaluate_expression;
use crate::registry::CapabilityRegistry;

/// Build the config JSON Schema shared by these nodes.
fn config_schema(properties: serde_json::Value, required: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn port(
    name: &str,
    display: &str,
    kind: PortKind,
    value_type: ValueType,
    required: bool,
) -> PortDescriptor {
    PortDescriptor::new(name, display, kind, value_type).required(required)
}

/// `core.Start`: the workflow entry point.
#[derive(Debug, Default)]
pub struct StartExecutor;

impl NodeExecutor for StartExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            outputs: vec![port("out", "Out", PortKind::Output, ValueType::Any, false)],
            config_schema: config_schema(serde_json::json!({}), &[]),
            allows_additional_config: false,
            ..NodeDescriptor::new("core.Start", "Start", "Core")
                .with_description("Entry point of the workflow")
        }
    }

    fn execute(
        &self,
        _input: NodeInput,
        _context: &mut ExecutionContext,
    ) -> NodeResult<NodeOutput> {
        Ok(NodeOutput::new().with_output("out", serde_json::Value::Null))
    }
}

/// `core.End`: terminates the workflow.
#[derive(Debug, Default)]
pub struct EndExecutor;

impl NodeExecutor for EndExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any, false)],
            config_schema: config_schema(
                serde_json::json!({
                    "code": {
                        "type": "integer",
                        "title": "Exit code",
                        "description": "Process exit code reported by the runtime when the \
                                        workflow finishes.",
                        "default": 0
                    }
                }),
                &[],
            ),
            allows_additional_config: false,
            ..NodeDescriptor::new("core.End", "End", "Core")
                .with_description("Terminates the workflow")
        }
    }

    fn execute(
        &self,
        _input: NodeInput,
        _context: &mut ExecutionContext,
    ) -> NodeResult<NodeOutput> {
        Ok(NodeOutput::new())
    }
}

/// `core.Log`: writes a message to the event stream and audit log.
#[derive(Debug, Default)]
pub struct LogExecutor;

impl NodeExecutor for LogExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any, false)],
            outputs: vec![port(
                "out",
                "Out",
                PortKind::Output,
                ValueType::String,
                false,
            )],
            config_schema: config_schema(
                serde_json::json!({
                    "message": {
                        "type": "string",
                        "title": "Message",
                        "description": "Message template; supports `{{variable}}` interpolation.",
                        "examples": ["Hello, {{name}}!"]
                    },
                    "level": {
                        "type": "string",
                        "title": "Level",
                        "description": "Log severity. Defaults to `info`.",
                        "enum": ["debug", "info", "warn", "error"],
                        "default": "info"
                    }
                }),
                &["message"],
            ),
            allows_additional_config: false,
            ..NodeDescriptor::new("core.Log", "Log", "Core")
                .with_description("Writes a message to the run log")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let message = input.require_str("message")?;
        let level = match input.config_str("level").unwrap_or("info") {
            "debug" => nodara_schema::LogLevel::Debug,
            "warn" => nodara_schema::LogLevel::Warn,
            "error" => nodara_schema::LogLevel::Error,
            _ => nodara_schema::LogLevel::Info,
        };
        context.log(level, message.clone());
        Ok(NodeOutput::new().with_output("out", serde_json::Value::String(message)))
    }
}

/// `core.Calculate`: evaluates an expression and publishes the result.
#[derive(Debug, Default)]
pub struct CalculateExecutor;

impl NodeExecutor for CalculateExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any, false)],
            outputs: vec![port(
                "result",
                "Result",
                PortKind::Output,
                ValueType::Number,
                false,
            )],
            config_schema: config_schema(
                serde_json::json!({
                    "expression": {
                        "type": "string",
                        "title": "Expression",
                        "description": "Arithmetic and comparison expression evaluated against \
                                        the run scope. Supports `+`, `-`, `*`, `/`, `%`, `^`, \
                                        comparisons and `&&`/`||`.",
                        "examples": ["2 + 2 * 3", "{{price}} * {{quantity}}"]
                    },
                    "output_var": {
                        "type": "string",
                        "title": "Output variable",
                        "description": "Variable the result is published under.",
                        "examples": ["answer"]
                    }
                }),
                &["expression", "output_var"],
            ),
            allows_additional_config: false,
            ..NodeDescriptor::new("core.Calculate", "Calculate", "Core")
                .with_description("Evaluates a numeric expression")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let expression = input.require_str("expression")?;
        let output_var = input.require_str("output_var")?;
        let value = evaluate_expression(&expression, context.variables())
            .map_err(|error| NodeError::InvalidConfig(format!("`{expression}`: {error}")))?;
        let number = number_value(value);
        context.set_variable(output_var, number.clone());
        Ok(NodeOutput::new().with_output("result", number))
    }
}

/// One named expression in `core.CalculateMany`.
#[derive(Debug, Clone, serde::Deserialize)]
struct NamedExpression {
    name: String,
    expression: String,
}

/// Configuration for `core.CalculateMany`.
#[derive(Debug, Clone, serde::Deserialize)]
struct CalculateManyConfig {
    expressions: Vec<NamedExpression>,
    #[serde(default)]
    variables: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    output_var: Option<String>,
}

/// `core.CalculateMany`: evaluates several named expressions in order.
#[derive(Debug, Default)]
pub struct CalculateManyExecutor;

impl NodeExecutor for CalculateManyExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any, false)],
            outputs: vec![port(
                "out",
                "Results",
                PortKind::Output,
                ValueType::Object,
                false,
            )],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "expressions": {
                        "type": "array",
                        "title": "Expressions",
                        "description": "Named expressions evaluated in order. Each result is published before the next expression is evaluated.",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": {
                                    "type": "string",
                                    "title": "Name",
                                    "description": "Variable name for this result."
                                },
                                "expression": {
                                    "type": "string",
                                    "title": "Expression",
                                    "description": "Arithmetic expression; may read run-scope variables and earlier results."
                                }
                            },
                            "required": ["name", "expression"],
                            "additionalProperties": false
                        }
                    },
                    "variables": {
                        "type": "object",
                        "title": "Seed variables",
                        "description": "Optional numeric values available only while evaluating this node.",
                        "additionalProperties": true
                    },
                    "output_var": {
                        "type": "string",
                        "title": "Output variable",
                        "description": "Optional variable receiving the complete result object."
                    }
                },
                "required": ["expressions"],
                "additionalProperties": false
            }),
            allows_additional_config: false,
            ..NodeDescriptor::new("core.CalculateMany", "Calculate Many", "Core")
                .with_description("Evaluates several named expressions in order")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let config: CalculateManyConfig = serde_json::from_value(input.resolved_config.clone())
            .map_err(|error| {
                NodeError::InvalidConfig(format!("invalid calculate config: {error}"))
            })?;
        if config.expressions.is_empty() {
            return Err(NodeError::InvalidConfig(
                "`expressions` must contain at least one entry".to_string(),
            ));
        }

        let mut scope = context.variables().clone();
        scope.extend(config.variables);
        let mut results = serde_json::Map::new();
        let mut seen = BTreeSet::new();
        for expression in config.expressions {
            let name = expression.name.trim();
            if name.is_empty() {
                return Err(NodeError::InvalidConfig(
                    "expression names must not be empty".to_string(),
                ));
            }
            if !seen.insert(name.to_string()) {
                return Err(NodeError::InvalidConfig(format!(
                    "duplicate expression name `{name}`"
                )));
            }
            let value =
                evaluate_expression(expression.expression.trim(), &scope).map_err(|error| {
                    NodeError::InvalidConfig(format!("`{}`: {error}", expression.expression))
                })?;
            let number = number_value(value);
            scope.insert(name.to_string(), number.clone());
            context.set_variable(name, number.clone());
            results.insert(name.to_string(), number);
        }

        let result = serde_json::Value::Object(results);
        if let Some(name) = config.output_var.filter(|name| !name.trim().is_empty()) {
            context.set_variable(name, result.clone());
        }
        Ok(NodeOutput::new().with_output("out", result))
    }
}

/// Render a computed `f64` as JSON, keeping integral results as integers so that
/// `{{result}}` interpolates to `8` rather than `8.0`.
fn number_value(value: f64) -> serde_json::Value {
    if !value.is_finite() {
        return serde_json::Value::Null;
    }
    if value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_992.0 {
        serde_json::Value::Number(serde_json::Number::from(value as i64))
    } else {
        serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null)
    }
}

/// `system.Delay`: sleeps without blocking cancellation.
#[derive(Debug, Default)]
pub struct DelayExecutor;

impl NodeExecutor for DelayExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any, false)],
            outputs: vec![port("out", "Out", PortKind::Output, ValueType::Any, false)],
            config_schema: config_schema(
                serde_json::json!({
                    "duration_ms": {
                        "type": "integer",
                        "title": "Duration (ms)",
                        "minimum": 0,
                        "default": 0,
                        "description": "How long to wait, in milliseconds. Cancellation is \
                                        observed while waiting."
                    }
                }),
                &["duration_ms"],
            ),
            allows_additional_config: false,
            ..NodeDescriptor::new("system.Delay", "Delay", "System")
                .with_description("Waits for a fixed duration")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let total_ms = input.require_i64("duration_ms")?.max(0) as u64;
        let mut remaining = total_ms;
        while remaining > 0 {
            context.check_cancelled()?;
            let slice = remaining.min(25);
            std::thread::sleep(Duration::from_millis(slice));
            remaining -= slice;
        }
        Ok(NodeOutput::new().with_output("out", serde_json::Value::from(total_ms)))
    }
}

/// `core.SetVariable`: writes an arbitrary value into the run scope.
#[derive(Debug, Default)]
pub struct SetVariableExecutor;

impl NodeExecutor for SetVariableExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any, false)],
            outputs: vec![port("out", "Out", PortKind::Output, ValueType::Any, false)],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "title": "Variable name",
                        "description": "Name to publish the value under in the run scope."
                    },
                    "value": {
                        "title": "Value",
                        "description": "Value to store. Strings support `{{variable}}` \
                                        interpolation; any JSON value is accepted."
                    }
                },
                "required": ["name"],
                "additionalProperties": false
            }),
            allows_additional_config: false,
            ..NodeDescriptor::new("core.SetVariable", "Set Variable", "Core")
                .with_description("Publishes a value into the run scope")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let name = input.require_str("name")?;
        let value = input
            .resolved_config
            .get("value")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        context.set_variable(name, value.clone());
        Ok(NodeOutput::new().with_output("out", value))
    }
}

/// Register every built-in executor.
pub fn register_builtins(registry: &mut CapabilityRegistry) {
    registry
        .register(StartExecutor)
        .register(EndExecutor)
        .register(LogExecutor)
        .register(CalculateExecutor)
        .register(CalculateManyExecutor)
        .register(DelayExecutor)
        .register(SetVariableExecutor);
}
