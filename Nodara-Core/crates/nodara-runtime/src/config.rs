//! Runtime configuration.

use std::path::PathBuf;
use std::time::Duration;

/// How capability decisions are made.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PolicyMode {
    /// Allow every capability. Useful for trusted, fully embedded hosts.
    AllowAll,
    /// Allow only the listed capabilities or permissions, deny the rest.
    Allowlist(Vec<String>),
    /// Allow safe nodes and require approval for privileged ones.
    #[default]
    Default,
}

/// Everything the runtime needs to start.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Interface to bind, e.g. `127.0.0.1`.
    pub host: String,
    /// Port to bind.
    pub port: u16,
    /// Directories scanned for plugins.
    pub plugin_dirs: Vec<PathBuf>,
    /// Launch discovered plugins at start-up.
    pub autoload_plugins: bool,
    /// Capability policy.
    pub policy: PolicyMode,
    /// Automatically approve policy requests that require approval.
    pub auto_approve: bool,
    /// How long a run waits for an operator decision when approval is required.
    pub approval_timeout: Duration,
    /// Append audit records to this file instead of keeping them in memory.
    pub audit_path: Option<PathBuf>,
    /// Send permissive CORS headers (the Studio is a browser client).
    pub permissive_cors: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let mut plugin_dirs = Vec::new();
        // Side-by-side layout produced by a release build.
        if let Ok(current) = std::env::current_exe() {
            if let Some(directory) = current.parent() {
                plugin_dirs.push(directory.join("plugins"));
            }
        }
        plugin_dirs.push(PathBuf::from("plugins"));

        Self {
            host: "127.0.0.1".to_string(),
            port: 8710,
            plugin_dirs,
            autoload_plugins: true,
            policy: PolicyMode::default(),
            auto_approve: true,
            approval_timeout: crate::sessions::DEFAULT_APPROVAL_TIMEOUT,
            audit_path: None,
            permissive_cors: true,
        }
    }
}

impl RuntimeConfig {
    /// `host:port`.
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Builder-style plugin directory.
    #[must_use]
    pub fn with_plugin_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.plugin_dirs.push(dir.into());
        self
    }

    /// Builder-style port.
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
}
