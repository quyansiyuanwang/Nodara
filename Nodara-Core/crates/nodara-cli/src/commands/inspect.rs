//! `nodara-cli inspect`

use std::path::Path;

use nodara_schema::WorkflowGraph;

use crate::error::CliResult;
use crate::workflow_io;

/// Print a structural summary of a workflow.
pub fn execute(file: &Path, json: bool) -> CliResult<()> {
    let loaded = workflow_io::load(file)?;
    let workflow = &loaded.workflow;
    let graph = WorkflowGraph::new(workflow);
    let order = graph
        .topological_order()
        .map(|order| order.into_iter().map(str::to_string).collect::<Vec<_>>())
        .map_err(|error| error.to_string());
    let cycle = graph.find_cycle();

    if json {
        let payload = serde_json::json!({
            "id": workflow.id,
            "schema_version": workflow.schema_version,
            "name": workflow.metadata.name,
            "nodes": workflow.nodes,
            "edges": workflow.edges,
            "variables": workflow.variables,
            "node_count": graph.node_count(),
            "edge_count": graph.edge_count(),
            "topological_order": order,
            "cycle": cycle,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!("id:             {}", workflow.id);
    println!("schema_version: {}", workflow.schema_version);
    println!("name:           {}", workflow.metadata.name);
    println!("nodes:          {}", graph.node_count());
    println!("edges:          {}", graph.edge_count());
    println!("variables:      {}", workflow.variables.len());
    println!("nodes:");
    for node in &workflow.nodes {
        let incoming = graph.in_degree(&node.id);
        let outgoing = graph.out_degree(&node.id);
        println!(
            "  {:<16} {:<28} in={incoming} out={outgoing}",
            node.id, node.node_type
        );
    }
    println!("edges:");
    for edge in &workflow.edges {
        let guard = edge
            .condition
            .as_deref()
            .map(|condition| format!(" when {condition}"))
            .unwrap_or_default();
        println!("  {} -> {}{guard}", edge.source, edge.target);
    }
    match order {
        Ok(order) => println!("topological order: {}", order.join(" -> ")),
        Err(error) => println!("topological order: unavailable ({error})"),
    }
    if let Some(cycle) = cycle {
        println!("cycle: {}", cycle.join(" -> "));
    }
    Ok(())
}
