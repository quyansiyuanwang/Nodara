//! Structured, capability-aware workflow validation.
//!
//! Validation never aborts on the first problem. It produces a
//! [`ValidationReport`] full of [`Diagnostic`]s so an editor can underline every
//! issue and an agent can repair them in one pass.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::descriptor::NodeDescriptor;
use crate::graph::WorkflowGraph;
use crate::version::is_compatible_schema_version;
use crate::workflow::{Node, Workflow};

/// Severity of a validation diagnostic.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Purely informational; does not affect validity.
    Info,
    /// Likely a mistake, but execution can proceed.
    Warning,
    /// Blocks execution.
    Error,
}

/// A single validation finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Diagnostic {
    /// Severity of the finding.
    pub severity: Severity,
    /// Stable machine-readable code, e.g. `WF103`.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
    /// JSON-pointer-ish location, e.g. `/nodes/2/config`.
    pub path: String,
    /// Related node, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// Related edge, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<String>,
    /// Suggested remediation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Diagnostic {
    fn new(
        severity: Severity,
        code: &str,
        message: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            code: code.to_string(),
            message: message.into(),
            path: path.into(),
            node_id: None,
            edge_id: None,
            hint: None,
        }
    }

    #[must_use]
    fn node(mut self, id: &str) -> Self {
        self.node_id = Some(id.to_string());
        self
    }

    #[must_use]
    fn edge(mut self, id: &str) -> Self {
        self.edge_id = Some(id.to_string());
        self
    }

    #[must_use]
    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Options controlling how strict validation is.
#[derive(Debug, Clone)]
pub struct ValidationOptions {
    /// Reject graphs that contain a cycle.
    pub reject_cycles: bool,
    /// Require at least one `core.Start` node.
    pub require_start: bool,
    /// Require at least one `core.End` node.
    pub require_end: bool,
    /// Warn about nodes unreachable from the entry point.
    pub warn_unreachable: bool,
    /// Warn about nodes with no outgoing edge that are not `core.End`.
    pub warn_dead_end: bool,
    /// Detect references to undeclared variables.
    pub check_variable_references: bool,
}

impl Default for ValidationOptions {
    fn default() -> Self {
        Self {
            reject_cycles: true,
            require_start: true,
            require_end: true,
            warn_unreachable: true,
            warn_dead_end: true,
            check_variable_references: true,
        }
    }
}

/// Result of validating a workflow.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    /// All findings, ordered by discovery.
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// Append a diagnostic.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Iterate over error diagnostics.
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    /// Iterate over warning diagnostics.
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    /// Number of blocking errors.
    pub fn error_count(&self) -> usize {
        self.errors().count()
    }

    /// Number of warnings.
    pub fn warning_count(&self) -> usize {
        self.warnings().count()
    }

    /// True when nothing blocks execution.
    pub fn is_valid(&self) -> bool {
        self.error_count() == 0
    }

    /// Merge another report into this one.
    pub fn extend(&mut self, other: Self) {
        self.diagnostics.extend(other.diagnostics);
    }
}

/// Supplies node-type knowledge so validation can check capabilities and ports.
///
/// The runtime backs this with its capability registry; the CLI may provide a
/// registry derived from installed plugins. Validation itself never depends on
/// how the registry is populated.
pub trait NodeTypeIndex {
    /// Every registered node type.
    fn node_types(&self) -> Vec<String>;

    /// Descriptor for a node type, when known.
    fn descriptor(&self, node_type: &str) -> Option<NodeDescriptor>;
}

/// Validate a workflow without capability awareness.
pub fn validate(workflow: &Workflow) -> ValidationReport {
    validate_with_options(workflow, None, &ValidationOptions::default())
}

/// Validate a workflow against a capability registry.
pub fn validate_with(
    workflow: &Workflow,
    index: &dyn NodeTypeIndex,
    options: &ValidationOptions,
) -> ValidationReport {
    validate_with_options(workflow, Some(index), options)
}

/// Validate a workflow, optionally consulting a capability registry.
pub fn validate_with_options(
    workflow: &Workflow,
    index: Option<&dyn NodeTypeIndex>,
    options: &ValidationOptions,
) -> ValidationReport {
    let mut report = ValidationReport::default();

    if !is_compatible_schema_version(&workflow.schema_version) {
        report.push(
            Diagnostic::new(
                Severity::Error,
                "WF100",
                format!(
                    "unsupported schema_version `{}` (this build supports {})",
                    workflow.schema_version,
                    crate::version::SCHEMA_VERSION
                ),
                "/schema_version",
            )
            .hint("run `nodara-cli migrate` to upgrade a legacy document"),
        );
    }

    if workflow.id.trim().is_empty() {
        report.push(Diagnostic::new(
            Severity::Error,
            "WF101",
            "workflow id must not be empty",
            "/id",
        ));
    }

    let mut seen_node_ids: HashSet<&str> = HashSet::with_capacity(workflow.nodes.len());
    for (i, node) in workflow.nodes.iter().enumerate() {
        let path = format!("/nodes/{i}");
        if node.id.trim().is_empty() {
            report.push(Diagnostic::new(
                Severity::Error,
                "WF102",
                "node id must not be empty",
                &path,
            ));
        } else if !seen_node_ids.insert(node.id.as_str()) {
            report.push(
                Diagnostic::new(
                    Severity::Error,
                    "WF103",
                    format!("duplicate node id `{}`", node.id),
                    &path,
                )
                .node(&node.id),
            );
        }
        if node.node_type.trim().is_empty() {
            report.push(
                Diagnostic::new(
                    Severity::Error,
                    "WF104",
                    "node `type` must not be empty",
                    &path,
                )
                .node(&node.id),
            );
        }
    }

    let mut seen_edge_ids: HashSet<&str> = HashSet::with_capacity(workflow.edges.len());
    for (i, edge) in workflow.edges.iter().enumerate() {
        let path = format!("/edges/{i}");
        if edge.id.trim().is_empty() {
            report.push(Diagnostic::new(
                Severity::Error,
                "WF110",
                "edge id must not be empty",
                &path,
            ));
        } else if !seen_edge_ids.insert(edge.id.as_str()) {
            report.push(
                Diagnostic::new(
                    Severity::Error,
                    "WF111",
                    format!("duplicate edge id `{}`", edge.id),
                    &path,
                )
                .edge(&edge.id),
            );
        }
        if !workflow.contains_node(&edge.source) {
            report.push(
                Diagnostic::new(
                    Severity::Error,
                    "WF112",
                    format!("edge source `{}` does not exist", edge.source),
                    &path,
                )
                .edge(&edge.id)
                .hint("create the missing node or remove the edge"),
            );
        }
        if !workflow.contains_node(&edge.target) {
            report.push(
                Diagnostic::new(
                    Severity::Error,
                    "WF113",
                    format!("edge target `{}` does not exist", edge.target),
                    &path,
                )
                .edge(&edge.id)
                .hint("create the missing node or remove the edge"),
            );
        }
        if edge.source == edge.target {
            report.push(
                Diagnostic::new(
                    Severity::Warning,
                    "WF114",
                    format!("edge `{}` is a self-loop", edge.id),
                    &path,
                )
                .edge(&edge.id),
            );
        }
    }

    let graph = WorkflowGraph::new(workflow);

    let start_nodes: Vec<&Node> = workflow
        .nodes
        .iter()
        .filter(|n| n.node_type == "core.Start")
        .collect();
    if options.require_start && start_nodes.is_empty() {
        report.push(Diagnostic::new(
            Severity::Error,
            "WF120",
            "workflow has no `core.Start` node",
            "/nodes",
        ));
    }
    if start_nodes.len() > 1 {
        report.push(Diagnostic::new(
            Severity::Error,
            "WF121",
            format!("workflow declares {} `core.Start` nodes", start_nodes.len()),
            "/nodes",
        ));
    }
    if options.require_end && !workflow.nodes.iter().any(|n| n.node_type == "core.End") {
        report.push(
            Diagnostic::new(
                Severity::Error,
                "WF122",
                "workflow has no `core.End` node",
                "/nodes",
            )
            .hint("terminate the graph with a `core.End` node"),
        );
    }

    if options.reject_cycles {
        if let Some(cycle) = graph.find_cycle() {
            report.push(
                Diagnostic::new(
                    Severity::Error,
                    "WF130",
                    format!("workflow contains a cycle: {}", cycle.join(" -> ")),
                    "/edges",
                )
                .hint("break the cycle or disable cycle rejection for looping workflows"),
            );
        }
    }

    if options.warn_unreachable {
        if let Some(start) = start_nodes.first() {
            let reachable = graph.reachable_from(&start.id);
            for node in &workflow.nodes {
                if !reachable.contains(node.id.as_str()) {
                    report.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "WF131",
                            format!("node `{}` is not reachable from `{}`", node.id, start.id),
                            "/nodes",
                        )
                        .node(&node.id),
                    );
                }
            }
        }
    }

    if options.warn_dead_end {
        for node in &workflow.nodes {
            if node.node_type == "core.End" || node.node_type == "core.Start" {
                continue;
            }
            if graph.out_degree(&node.id) == 0 {
                report.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "WF132",
                        format!("node `{}` has no outgoing edge", node.id),
                        "/nodes",
                    )
                    .node(&node.id),
                );
            }
        }
    }

    if let Some(index) = index {
        let known: HashSet<String> = index.node_types().into_iter().collect();
        for node in &workflow.nodes {
            if !known.contains(&node.node_type) {
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "WF140",
                        format!("unknown node type `{}`", node.node_type),
                        "/nodes",
                    )
                    .node(&node.id)
                    .hint("install a plugin that provides this node type"),
                );
                continue;
            }
            if let Some(descriptor) = index.descriptor(&node.node_type) {
                validate_config(node, &descriptor, &mut report);
            }
        }
    }

    if options.check_variable_references {
        let declared: HashSet<&str> = workflow.variables.keys().map(String::as_str).collect();
        let mut reported: HashSet<(String, String)> = HashSet::new();
        for node in &workflow.nodes {
            for reference in collect_variable_references(&node.config) {
                if declared.contains(reference.as_str()) {
                    continue;
                }
                if reported.insert((format!("node:{}", node.id), reference.clone())) {
                    report.push(
                        Diagnostic::new(
                            Severity::Warning,
                            "WF150",
                            format!(
                                "node `{}` references undeclared variable `{reference}`",
                                node.id
                            ),
                            "/nodes",
                        )
                        .node(&node.id)
                        .hint("declare it under `variables` or bind it at runtime"),
                    );
                }
            }
            if let Some(condition) = &node.condition {
                for reference in collect_expression_identifiers(condition) {
                    if declared.contains(reference.as_str()) {
                        continue;
                    }
                    if reported.insert((format!("condition:{}", node.id), reference.clone())) {
                        report.push(
                            Diagnostic::new(
                                Severity::Warning,
                                "WF151",
                                format!(
                                    "node `{}` condition references undeclared variable `{reference}`",
                                    node.id
                                ),
                                "/nodes",
                            )
                            .node(&node.id)
                            .hint("declare it under `variables` or bind it at runtime"),
                        );
                    }
                }
            }
        }
        for edge in &workflow.edges {
            if let Some(condition) = &edge.condition {
                for reference in collect_expression_identifiers(condition) {
                    if declared.contains(reference.as_str()) {
                        continue;
                    }
                    if reported.insert((format!("edge:{}", edge.id), reference.clone())) {
                        report.push(
                            Diagnostic::new(
                                Severity::Warning,
                                "WF152",
                                format!(
                                    "edge `{}` condition references undeclared variable `{reference}`",
                                    edge.id
                                ),
                                "/edges",
                            )
                            .edge(&edge.id)
                            .hint("declare it under `variables` or bind it at runtime"),
                        );
                    }
                }
            }
        }
    }

    report
}

fn validate_config(node: &Node, descriptor: &NodeDescriptor, report: &mut ValidationReport) {
    let Some(config) = node.config.as_object() else {
        report.push(
            Diagnostic::new(
                Severity::Error,
                "WF141",
                format!("node `{}` config must be a JSON object", node.id),
                "/nodes",
            )
            .node(&node.id),
        );
        return;
    };

    let required = descriptor
        .config_schema
        .get("required")
        .and_then(|v| v.as_array());
    if let Some(required) = required {
        for key in required.iter().filter_map(|v| v.as_str()) {
            if !config.contains_key(key) {
                report.push(
                    Diagnostic::new(
                        Severity::Error,
                        "WF142",
                        format!("node `{}` is missing required config key `{key}`", node.id),
                        "/nodes",
                    )
                    .node(&node.id),
                );
            }
        }
    }

    if let Some(props) = descriptor
        .config_schema
        .get("properties")
        .and_then(|v| v.as_object())
    {
        for key in config.keys() {
            if !props.contains_key(key) && !descriptor.allows_additional_config {
                report.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "WF143",
                        format!("node `{}` has unknown config key `{key}`", node.id),
                        "/nodes",
                    )
                    .node(&node.id),
                );
            }
        }
    }
}

/// Extract `{{variable}}` references from a JSON config tree.
fn collect_variable_references(value: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_refs_into(value, &mut out);
    out
}

fn collect_refs_into(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.extend(parse_placeholders(s)),
        serde_json::Value::Array(items) => {
            for item in items {
                collect_refs_into(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                collect_refs_into(item, out);
            }
        }
        _ => {}
    }
}

/// Extract identifier roots used by an expression (`a.b + 1` yields `a`).
fn collect_expression_identifiers(expression: &str) -> Vec<String> {
    let chars: Vec<char> = expression.chars().collect();
    let mut names = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let current = chars[index];
        if current.is_alphabetic() || current == '_' {
            let start = index;
            while index < chars.len()
                && (chars[index].is_alphanumeric() || chars[index] == '_' || chars[index] == '.')
            {
                index += 1;
            }
            let token: String = chars[start..index].iter().collect();
            let root = token.split('.').next().unwrap_or(&token);
            if root != "true" && root != "false" {
                names.push(root.to_string());
            }
            continue;
        }
        index += 1;
    }
    names
}

/// Parse `{{ name }}` placeholders out of a template string.
pub fn parse_placeholders(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if let Some(end) = text[i + 2..].find("}}") {
                let raw = &text[i + 2..i + 2 + end];
                let name = raw.trim();
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '-')
                {
                    names.push(name.to_string());
                }
                i += 2 + end + 2;
                continue;
            }
        }
        i += 1;
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{Edge, Node, Workflow};

    fn linear() -> Workflow {
        let mut wf = Workflow::new("wf.test");
        wf.add_node(Node::new("start", "core.Start"));
        wf.add_node(Node::new("log", "core.Log"));
        wf.add_node(Node::new("end", "core.End"));
        wf.add_edge(Edge::new("e1", "start", "log"));
        wf.add_edge(Edge::new("e2", "log", "end"));
        wf
    }

    #[test]
    fn valid_linear_workflow_has_no_errors() {
        let report = validate(&linear());
        assert!(report.is_valid(), "{:?}", report.diagnostics);
    }

    #[test]
    fn detects_cycle() {
        let mut wf = linear();
        wf.add_edge(Edge::new("e3", "end", "log"));
        let report = validate(&wf);
        assert!(report.diagnostics.iter().any(|d| d.code == "WF130"));
    }

    #[test]
    fn detects_missing_edge_target() {
        let mut wf = Workflow::new("wf.test");
        wf.add_node(Node::new("start", "core.Start"));
        wf.add_node(Node::new("end", "core.End"));
        wf.add_edge(Edge::new("e1", "start", "ghost"));
        let report = validate(&wf);
        assert!(report.diagnostics.iter().any(|d| d.code == "WF113"));
    }

    #[test]
    fn detects_duplicate_node_ids() {
        let mut wf = Workflow::new("wf.test");
        wf.add_node(Node::new("start", "core.Start"));
        wf.add_node(Node::new("start", "core.End"));
        let report = validate(&wf);
        assert!(report.diagnostics.iter().any(|d| d.code == "WF103"));
    }

    #[test]
    fn checks_node_and_edge_condition_references() {
        let mut wf = linear();
        wf.node_mut("log").unwrap().condition = Some("missing > 0".to_string());
        wf.edges[0].condition = Some("also_missing == 1".to_string());
        let report = validate(&wf);
        assert!(report.diagnostics.iter().any(|d| d.code == "WF151"));
        assert!(report.diagnostics.iter().any(|d| d.code == "WF152"));
    }

    #[test]
    fn parses_placeholders() {
        assert_eq!(
            parse_placeholders("hello {{name}} {{ count }}"),
            vec!["name".to_string(), "count".to_string()]
        );
        assert!(parse_placeholders("no placeholders").is_empty());
    }
}
