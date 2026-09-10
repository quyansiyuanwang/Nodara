//! Filesystem plugin discovery.
//!
//! Discovery is deliberately boring: look for `manifest.json` either directly in
//! a configured root or in its immediate children. There is no online registry in
//! the core, by design — that belongs to a future product surface, not here.

use std::path::{Path, PathBuf};

use rf_schema::PluginManifest;

/// A plugin directory that parsed successfully.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// Parsed, validated manifest.
    pub manifest: PluginManifest,
    /// Directory containing the manifest and executable.
    pub directory: PathBuf,
}

/// A directory that looked like a plugin but could not be loaded.
#[derive(Debug, Clone)]
pub struct DiscoveryError {
    /// Offending directory.
    pub directory: PathBuf,
    /// Why it was rejected.
    pub message: String,
}

/// Result of scanning one or more roots.
#[derive(Debug, Default, Clone)]
pub struct DiscoveryOutcome {
    /// Successfully parsed plugins.
    pub plugins: Vec<DiscoveredPlugin>,
    /// Directories that failed to parse.
    pub errors: Vec<DiscoveryError>,
}

impl DiscoveryOutcome {
    /// Total number of directories inspected.
    pub fn inspected(&self) -> usize {
        self.plugins.len() + self.errors.len()
    }
}

/// Scan `roots` for plugin directories.
pub fn discover_plugins(roots: &[PathBuf]) -> DiscoveryOutcome {
    let mut outcome = DiscoveryOutcome::default();
    for root in roots {
        scan_root(root, &mut outcome);
    }
    outcome
}

fn scan_root(root: &Path, outcome: &mut DiscoveryOutcome) {
    if !root.is_dir() {
        return;
    }
    if root.join(rf_schema::MANIFEST_FILE).is_file() {
        record(root, outcome);
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join(rf_schema::MANIFEST_FILE).is_file())
        .collect();
    children.sort();
    for child in children {
        record(&child, outcome);
    }
}

fn record(directory: &Path, outcome: &mut DiscoveryOutcome) {
    match PluginManifest::from_dir(directory) {
        Ok(manifest) => outcome.plugins.push(DiscoveredPlugin {
            manifest,
            directory: directory.to_path_buf(),
        }),
        Err(error) => outcome.errors.push(DiscoveryError {
            directory: directory.to_path_buf(),
            message: error.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )
    }

    fn write_plugin(dir: &Path, id: &str, executable: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let manifest = serde_json::json!({
            "id": id,
            "name": id,
            "version": "1.0.0",
            "protocol_version": "1",
            "executable": executable,
            "capabilities": ["Test.Capability"],
            "node_types": ["test.Node"]
        });
        std::fs::write(
            dir.join(rf_schema::MANIFEST_FILE),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn discovers_child_directories() {
        let root = std::env::temp_dir().join(format!("rf-discover-{}", unique()));
        write_plugin(&root.join("alpha"), "rf.alpha", "alpha.exe");
        write_plugin(&root.join("beta"), "rf.beta", "beta.exe");

        let outcome = discover_plugins(std::slice::from_ref(&root));
        assert_eq!(outcome.plugins.len(), 2);
        assert!(outcome.errors.is_empty());
        let ids: Vec<&str> = outcome
            .plugins
            .iter()
            .map(|plugin| plugin.manifest.id.as_str())
            .collect();
        assert_eq!(ids, vec!["rf.alpha", "rf.beta"]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reports_malformed_manifests_without_aborting() {
        let root = std::env::temp_dir().join(format!("rf-discover-bad-{}", unique()));
        write_plugin(&root.join("good"), "rf.good", "good.exe");
        let bad = root.join("bad");
        std::fs::create_dir_all(&bad).unwrap();
        std::fs::write(bad.join(rf_schema::MANIFEST_FILE), "{ not json").unwrap();

        let outcome = discover_plugins(std::slice::from_ref(&root));
        assert_eq!(outcome.plugins.len(), 1);
        assert_eq!(outcome.errors.len(), 1);
        assert_eq!(outcome.inspected(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_roots_are_ignored() {
        let outcome = discover_plugins(&[PathBuf::from("definitely-not-here")]);
        assert_eq!(outcome.inspected(), 0);
    }
}
