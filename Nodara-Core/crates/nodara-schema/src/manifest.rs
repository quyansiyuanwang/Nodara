//! Plugin manifests.
//!
//! A manifest is the only thing the runtime reads before launching a plugin
//! process. It is therefore small, declarative and validated before any code in
//! the plugin runs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{SchemaError, SchemaResult};
use crate::version::{major_of, PROTOCOL_VERSION};

/// Conventional manifest file name inside a plugin directory.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Declarative description of an installed plugin.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct PluginManifest {
    /// Reverse-DNS style unique id, e.g. `nodara.windows.input`.
    pub id: String,
    /// Human-facing name.
    pub name: String,
    /// Semantic version of the plugin.
    pub version: String,
    /// Wire protocol version the plugin speaks.
    pub protocol_version: String,
    /// Executable path relative to the plugin directory.
    pub executable: String,
    /// Extra process arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Capabilities the plugin provides, e.g. `Input.Keyboard`.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Permissions the plugin requires to function.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Node types the plugin provides.
    #[serde(default)]
    pub node_types: Vec<String>,
    /// Short description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Author or organisation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Homepage or repository URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Extension bag for future fields.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl PluginManifest {
    /// Parse a manifest from JSON text.
    pub fn from_json(text: &str) -> SchemaResult<Self> {
        Ok(serde_json::from_str(text)?)
    }

    /// Read and validate a manifest from a plugin directory.
    pub fn from_dir(dir: &Path) -> SchemaResult<Self> {
        let path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path)?;
        let manifest: Self = serde_json::from_str(&text)?;
        manifest
            .validate()
            .map_err(|errors| SchemaError::InvalidManifest(errors.join("; ")))?;
        Ok(manifest)
    }

    /// Validate required fields and protocol compatibility.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.id.trim().is_empty() {
            errors.push("`id` must not be empty".to_string());
        }
        if self.name.trim().is_empty() {
            errors.push("`name` must not be empty".to_string());
        }
        if self.version.trim().is_empty() {
            errors.push("`version` must not be empty".to_string());
        }
        if self.executable.trim().is_empty() {
            errors.push("`executable` must not be empty".to_string());
        }
        if major_of(&self.protocol_version) != major_of(PROTOCOL_VERSION) {
            errors.push(format!(
                "`protocol_version` {} is not compatible with runtime protocol {}",
                self.protocol_version, PROTOCOL_VERSION
            ));
        }
        if self.node_types.is_empty() && self.capabilities.is_empty() {
            errors.push("plugin must declare at least one node type or capability".to_string());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Resolve the executable path relative to the plugin directory.
    pub fn executable_path(&self, dir: &Path) -> PathBuf {
        let executable = Path::new(&self.executable);
        if executable.is_absolute() {
            executable.to_path_buf()
        } else {
            dir.join(executable)
        }
    }

    /// True when the plugin declares the given permission.
    pub fn requires_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "id": "nodara.windows.input",
        "name": "Windows Input",
        "version": "1.0.0",
        "protocol_version": "1",
        "executable": "nodara-platform-plugin.exe",
        "capabilities": ["Input.Keyboard", "Input.Mouse"],
        "permissions": ["input.control"],
        "node_types": ["windows.Input.Keyboard", "windows.Input.Mouse"]
    }"#;

    #[test]
    fn parses_valid_manifest() {
        let manifest = PluginManifest::from_json(SAMPLE).expect("manifest parses");
        assert_eq!(manifest.id, "nodara.windows.input");
        assert!(manifest.validate().is_ok());
        assert!(manifest.requires_permission("input.control"));
    }

    #[test]
    fn rejects_incompatible_protocol() {
        let mut manifest = PluginManifest::from_json(SAMPLE).unwrap();
        manifest.protocol_version = "2".to_string();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn rejects_empty_node_types_and_capabilities() {
        let mut manifest = PluginManifest::from_json(SAMPLE).unwrap();
        manifest.node_types.clear();
        manifest.capabilities.clear();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn resolves_relative_executable() {
        let manifest = PluginManifest::from_json(SAMPLE).unwrap();
        let path = manifest.executable_path(Path::new("/plugins/input"));
        assert!(path.ends_with("nodara-platform-plugin.exe"));
    }
}
