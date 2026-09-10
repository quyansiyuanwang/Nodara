//! `rf-cli simulate` — a dry run that reports the plan without executing.

use std::path::{Path, PathBuf};

use rf_schema::{validate_with, WorkflowGraph};

use crate::error::{CliError, CliResult};
use crate::workflow_io;

/// Report the execution plan for a workflow.
pub fn execute(
    file: &Path,
    json: bool,
    plugin_dirs: &[PathBuf],
    in_process: bool,
) -> CliResult<()> {
    let loaded = workflow_io::load(file)?;
    let capabilities = super::build_capabilities(plugin_dirs, in_process, true)?;
    let report = validate_with(
        &loaded.workflow,
        &capabilities.registry,
        &Default::default(),
    );

    let graph = WorkflowGraph::new(&loaded.workflow);
    let order: Result<Vec<String>, _> = graph
        .topological_order()
        .map(|order| order.into_iter().map(str::to_string).collect());

    let mut steps = Vec::new();
    if let Ok(order) = &order {
        let reachable: std::collections::HashSet<String> = graph
            .entry_node()
            .map(|entry| {
                graph
                    .reachable_from(&entry.id)
                    .into_iter()
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        for node_id in order {
            if !reachable.contains(node_id) {
                continue;
            }
            let Some(node) = loaded.workflow.node(node_id) else {
                continue;
            };
            let descriptor = capabilities.registry.descriptor(&node.node_type);
            steps.push(serde_json::json!({
                "node_id": node.id,
                "node_type": node.node_type,
                "permissions": descriptor.as_ref().map(|d| d.permissions.clone()).unwrap_or_default(),
                "dangerous": descriptor.as_ref().map(|d| d.dangerous).unwrap_or(false),
            }));
        }
    }

    if json {
        let payload = serde_json::json!({
            "workflow_id": loaded.workflow.id,
            "valid": report.is_valid(),
            "errors": report.error_count(),
            "warnings": report.warning_count(),
            "steps": steps,
            "diagnostics": report.diagnostics,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("workflow: {}", loaded.workflow.id);
        println!(
            "valid: {} ({} error(s), {} warning(s))",
            report.is_valid(),
            report.error_count(),
            report.warning_count()
        );
        match order {
            Ok(_) => {
                println!("plan:");
                for step in &steps {
                    let marker = if step["dangerous"].as_bool().unwrap_or(false) {
                        " [requires approval]"
                    } else {
                        ""
                    };
                    println!(
                        "  {}. {} ({}){marker}",
                        steps
                            .iter()
                            .position(|candidate| candidate == step)
                            .map_or(0, |index| index + 1),
                        step["node_id"].as_str().unwrap_or_default(),
                        step["node_type"].as_str().unwrap_or_default()
                    );
                }
            }
            Err(error) => println!("plan: cannot schedule ({error})"),
        }
    }

    if report.is_valid() {
        Ok(())
    } else {
        Err(CliError::Invalid(report.error_count()))
    }
}
