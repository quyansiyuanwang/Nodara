//! The node reference has to describe what the build actually ships.
//!
//! `docs/nodes.md` and its Chinese twin are the human-readable counterpart of
//! `GET /api/v1/node-types`. Nothing generates them, so this test fails when a
//! node type, a permission or a configuration key is added without documenting
//! it — the documentation equivalent of the schema drift check.

use std::path::{Path, PathBuf};

use nodara_core::CapabilityRegistry;

/// The node types the default build can run.
fn shipped_registry() -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::new();
    nodara_core::register_builtins(&mut registry);
    nodara_platform::register_platform(&mut registry);
    nodara_vision::register_vision(&mut registry);
    registry
}

/// The repository's `docs/` directory, from this crate's manifest.
fn docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs")
}

fn read(name: &str) -> String {
    let path = docs_dir().join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The Markdown section that documents `node_type`.
///
/// A section starts at its `### \`type\`` heading and ends at the next heading
/// of level two or three.
fn section<'a>(document: &'a str, node_type: &str) -> Option<&'a str> {
    let heading = format!("### `{node_type}`");
    let start = document.find(&heading)?;
    let rest = &document[start..];
    let end = rest[1..]
        .find("\n## ")
        .map_or(rest.len(), |offset| offset + 1);
    Some(&rest[..end])
}

#[test]
fn node_reference_documents_every_shipped_node_type() {
    let registry = shipped_registry();
    let english = read("nodes.md");
    let chinese = read("nodes.zh.md");

    for descriptor in registry.descriptors() {
        let node_type = &descriptor.node_type;
        let english_section = section(&english, node_type)
            .unwrap_or_else(|| panic!("docs/nodes.md has no section for `{node_type}`"));
        assert!(
            english_section.contains(&format!("`{node_type}`")),
            "docs/nodes.md section for `{node_type}` does not name it"
        );
        assert!(
            section(&chinese, node_type).is_some(),
            "docs/nodes.zh.md has no section for `{node_type}`"
        );
        for permission in &descriptor.permissions {
            assert!(
                english_section.contains(permission.as_str()),
                "docs/nodes.md does not mention `{permission}` for `{node_type}`"
            );
        }
    }
}

#[test]
fn node_reference_documents_every_configuration_key() {
    let registry = shipped_registry();
    let english = read("nodes.md");
    let chinese = read("nodes.zh.md");

    for descriptor in registry.descriptors() {
        let node_type = &descriptor.node_type;
        let Some(properties) = descriptor.config_schema["properties"].as_object() else {
            continue;
        };
        for key in properties.keys() {
            for (name, document) in [("nodes.md", &english), ("nodes.zh.md", &chinese)] {
                let section = section(document, node_type)
                    .unwrap_or_else(|| panic!("{name} has no section for `{node_type}`"));
                assert!(
                    section.contains(&format!("`{key}`")),
                    "{name} does not document `{key}` for `{node_type}`"
                );
            }
        }
    }
}

#[test]
fn the_two_node_references_document_the_same_types() {
    let registry = shipped_registry();
    let english = documented_types(&read("nodes.md"), &registry);
    let chinese = documented_types(&read("nodes.zh.md"), &registry);

    assert_eq!(
        english, chinese,
        "the two node references document different types"
    );
    assert_eq!(
        english.len(),
        registry.descriptors().len(),
        "a shipped node type is missing from the reference"
    );
}

fn documented_types(document: &str, registry: &CapabilityRegistry) -> Vec<String> {
    let mut types: Vec<String> = registry
        .descriptors()
        .into_iter()
        .map(|descriptor| descriptor.node_type)
        .filter(|node_type| section(document, node_type).is_some())
        .collect();
    types.sort();
    types
}
