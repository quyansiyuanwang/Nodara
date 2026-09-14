//! Bridges a remote plugin node into the in-process executor SDK.

use std::sync::Arc;

use nodara_core::{NodeError, NodeExecutor, NodeInput, NodeOutput};
use nodara_schema::NodeDescriptor;

use crate::client::PluginClient;
use crate::error::PluginError;
use crate::jsonrpc::codes;
use crate::protocol::ExecuteParams;

/// A [`NodeExecutor`] whose implementation lives in another process.
pub struct PluginExecutor {
    client: Arc<PluginClient>,
    descriptor: NodeDescriptor,
}

impl std::fmt::Debug for PluginExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginExecutor")
            .field("node_type", &self.descriptor.node_type)
            .field("plugin_id", &self.client.info().id)
            .finish()
    }
}

impl PluginExecutor {
    /// Wrap a descriptor exposed by `client`.
    pub fn new(client: Arc<PluginClient>, descriptor: NodeDescriptor) -> Self {
        Self { client, descriptor }
    }

    /// The plugin that provides this node.
    pub fn client(&self) -> &Arc<PluginClient> {
        &self.client
    }

    fn map_error(&self, error: PluginError) -> NodeError {
        map_error(&error, &self.descriptor.node_type, &self.client.info().id)
    }
}

/// Translate a transport or protocol failure into the engine's error model.
///
/// This mapping is the contract between the plugin protocol's error table and
/// the engine: a plugin reporting `E_INVALID_CONFIG` must look like any other
/// invalid configuration to the caller, or the plugin boundary would leak.
pub fn map_error(error: &PluginError, node_type: &str, plugin_id: &str) -> NodeError {
    match error {
        PluginError::Remote { code, message, .. } => match *code {
            codes::APP_INVALID_CONFIG => NodeError::InvalidConfig(message.clone()),
            codes::APP_PERMISSION_DENIED => NodeError::denied(node_type, message.clone()),
            codes::APP_CANCELLED => NodeError::Cancelled,
            codes::APP_TIMEOUT => NodeError::Timeout,
            codes::APP_UNSUPPORTED => NodeError::Unsupported(message.clone()),
            codes::APP_IO => NodeError::Io(message.clone()),
            _ => NodeError::Execution(message.clone()),
        },
        PluginError::Timeout { .. } => NodeError::Timeout,
        PluginError::Disconnected => {
            NodeError::Execution(format!("plugin `{plugin_id}` disconnected"))
        }
        PluginError::Launch { id, source } => {
            NodeError::Execution(format!("plugin `{id}` could not be launched: {source}"))
        }
        other => NodeError::Execution(other.to_string()),
    }
}

impl NodeExecutor for PluginExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        self.descriptor.clone()
    }

    fn execute(
        &self,
        input: NodeInput,
        context: &mut nodara_core::ExecutionContext,
    ) -> Result<NodeOutput, NodeError> {
        context.check_cancelled()?;
        let params = ExecuteParams {
            run_id: context.run_id().to_string(),
            node_id: input.node_id.clone(),
            node_type: input.node_type.clone(),
            config: input.resolved_config.clone(),
            inputs: input.inputs.clone(),
            // Crosses a process boundary, so secrets stay masked; a plugin
            // receives resolved config values through `config`, not the scope.
            variables: context.redacted_variables(),
            timeout_ms: None,
        };
        match self.client.execute(params) {
            Ok(result) => Ok(NodeOutput {
                outputs: result.outputs,
                variables: result.variables,
            }),
            Err(error) => Err(self.map_error(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_codes_map_onto_engine_errors() {
        let cases = [
            (codes::APP_INVALID_CONFIG, "E_INVALID_CONFIG"),
            (codes::APP_PERMISSION_DENIED, "E_PERMISSION_DENIED"),
            (codes::APP_CANCELLED, "E_CANCELLED"),
            (codes::APP_TIMEOUT, "E_TIMEOUT"),
            (codes::APP_UNSUPPORTED, "E_UNSUPPORTED"),
            (codes::APP_IO, "E_IO"),
            (codes::APP_EXECUTION, "E_EXECUTION"),
        ];
        for (code, expected) in cases {
            let error = PluginError::Remote {
                code,
                message: "boom".to_string(),
                data: None,
            };
            assert_eq!(
                map_error(&error, "test.Node", "nodara.test").code(),
                expected,
                "code {code} mapped incorrectly"
            );
        }
    }

    #[test]
    fn an_unknown_code_falls_back_to_execution() {
        let error = PluginError::Remote {
            code: -99999,
            message: "mystery".to_string(),
            data: None,
        };
        assert_eq!(
            map_error(&error, "test.Node", "nodara.test").code(),
            "E_EXECUTION"
        );
    }

    #[test]
    fn transport_failures_are_distinguished() {
        let timeout = PluginError::Timeout {
            method: "execute".to_string(),
            timeout_ms: 10,
        };
        assert_eq!(map_error(&timeout, "n", "p").code(), "E_TIMEOUT");

        let mapped = map_error(&PluginError::Disconnected, "n", "nodara.demo");
        assert_eq!(mapped.code(), "E_EXECUTION");
        assert!(mapped.to_string().contains("nodara.demo"));
    }
}
