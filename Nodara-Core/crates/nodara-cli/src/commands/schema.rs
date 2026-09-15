//! `nodara-cli schema` — publish the JSON Schema documents.
//!
//! The workflow schema is *composed*: the node types this build can actually run
//! contribute their configuration schemas, so a workflow file that declares
//! `"$schema"` gets node-type completion, config-key completion and inline
//! documentation in an editor — the same experience the original single-process
//! implementation got from generating one schema from every model.
//!
//! ```text
//! nodara-cli schema --out schema                     # built-ins + official capabilities
//! nodara-cli schema --plugin-dir target/release/plugins
//! nodara-cli schema --no-capabilities                # bare schema, no node catalog
//! ```

use std::path::PathBuf;

use crate::error::CliResult;

/// A document whose contents do not depend on the installed capabilities.
type StaticDocument = (&'static str, fn() -> serde_json::Value);

/// Documents published from the Rust types alone, in a stable order.
const STATIC_DOCUMENTS: &[StaticDocument] = &[
    (
        "plugin-manifest.schema.json",
        nodara_schema::manifest_schema,
    ),
    (
        "node-descriptor.schema.json",
        nodara_schema::descriptor_schema,
    ),
    ("extension.schema.json", nodara_schema::extension_schema),
    ("execution-event.schema.json", nodara_schema::event_schema),
    ("agent-session.schema.json", nodara_schema::session_schema),
    (
        "agent-tool-call.schema.json",
        nodara_schema::tool_call_schema,
    ),
];

/// Arguments accepted by `schema`.
pub struct SchemaArgs {
    /// Directory to write the documents into.
    pub out: PathBuf,
    /// Print the documents instead of writing files.
    pub stdout: bool,
    /// Plugin directories whose descriptors join the workflow schema.
    pub plugin_dirs: Vec<PathBuf>,
    /// Compose the workflow schema from the installed node catalog.
    pub capabilities: bool,
}

/// Write or print the JSON Schema documents generated from the Rust types.
pub fn execute(args: SchemaArgs) -> CliResult<()> {
    let workflow = workflow_schema(&args)?;
    let documents = std::iter::once(("workflow.schema.json".to_string(), workflow))
        .chain(
            STATIC_DOCUMENTS
                .iter()
                .map(|(name, generate)| ((*name).to_string(), generate())),
        )
        .collect::<Vec<_>>();

    if args.stdout {
        for (name, document) in &documents {
            println!("// {name}");
            println!("{}", serde_json::to_string_pretty(document)?);
        }
        return Ok(());
    }

    std::fs::create_dir_all(&args.out)?;
    for (name, document) in &documents {
        let path = args.out.join(name);
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(document)?),
        )?;
        println!("wrote {}", path.display());
    }
    if let Some(count) = node_type_count(&documents[0].1) {
        println!(
            "workflow schema covers {count} node type(s); editors pick them up from the \
             document's `$schema`"
        );
    }
    Ok(())
}

/// Compose the workflow schema for the node catalog this invocation can see.
fn workflow_schema(args: &SchemaArgs) -> CliResult<serde_json::Value> {
    if !args.capabilities {
        return Ok(nodara_schema::workflow_schema());
    }

    // Explicit plugin directories are launched so their descriptors arrive over
    // the protocol; without them the schema is built from the in-process
    // official capabilities, which keeps the published file deterministic.
    let capabilities =
        super::build_capabilities(&args.plugin_dirs, true, !args.plugin_dirs.is_empty())?;
    for (id, message) in &capabilities.failures {
        eprintln!("warning: plugin `{id}` could not be described: {message}");
    }
    let schema = nodara_schema::workflow_schema_for(&capabilities.registry.descriptors());
    capabilities.host.shutdown();
    Ok(schema)
}

/// Number of node types a composed workflow schema advertises, if any.
fn node_type_count(schema: &serde_json::Value) -> Option<usize> {
    schema["definitions"][nodara_schema::NODE_TYPE_DEFINITION]["enum"]
        .as_array()
        .map(Vec::len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_schema_advertises_no_node_types() {
        assert!(node_type_count(&nodara_schema::workflow_schema()).is_none());
    }

    #[test]
    fn composed_schema_counts_node_types() {
        let descriptor = nodara_schema::NodeDescriptor::new("core.Log", "Log", "Core");
        let schema = nodara_schema::workflow_schema_for(&[descriptor]);
        assert_eq!(node_type_count(&schema), Some(1));
    }
}
