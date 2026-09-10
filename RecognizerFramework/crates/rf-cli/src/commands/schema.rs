//! `rf-cli schema` — publish the JSON Schema documents.

use std::path::Path;

use crate::error::CliResult;

/// A published document: file name plus the generator for its contents.
type Document = (&'static str, fn() -> serde_json::Value);

/// Documents published by this command, in a stable order.
const DOCUMENTS: &[Document] = &[
    ("workflow.schema.json", rf_schema::workflow_schema),
    ("plugin-manifest.schema.json", rf_schema::manifest_schema),
    ("node-descriptor.schema.json", rf_schema::descriptor_schema),
    ("execution-event.schema.json", rf_schema::event_schema),
    ("agent-session.schema.json", rf_schema::session_schema),
    ("agent-tool-call.schema.json", rf_schema::tool_call_schema),
];

/// Write or print the JSON Schema documents generated from the Rust types.
pub fn execute(out: &Path, stdout: bool) -> CliResult<()> {
    if stdout {
        for (name, generate) in DOCUMENTS {
            println!("// {name}");
            println!("{}", serde_json::to_string_pretty(&generate())?);
        }
        return Ok(());
    }

    std::fs::create_dir_all(out)?;
    for (name, generate) in DOCUMENTS {
        let path = out.join(name);
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&generate())?),
        )?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
