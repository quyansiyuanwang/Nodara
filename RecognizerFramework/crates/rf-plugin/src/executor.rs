//! Bridges a remote plugin node into the in-process executor SDK.

use std::sync::Arc;

use rf_core::{NodeError, NodeExecutor, NodeInput, NodeOutput};
use rf_schema::NodeDescriptor;

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
        let node_type = self.descriptor.node_type.clone();
        match error {
            PluginError::Remote { code, message, .. } => match code {
                codes::APP_INVALID_CONFIG => NodeError::InvalidConfig(message),
                codes::APP_PERMISSION_DENIED => NodeError::denied(node_type, message),
                codes::APP_CANCELLED => NodeError::Cancelled,
                codes::APP_TIMEOUT => NodeError::Timeout,
                codes::APP_UNSUPPORTED => NodeError::Unsupported(message),
                codes::APP_IO => NodeError::Io(message),
                _ => NodeError::Execution(message),
            },
            PluginError::Timeout { .. } => NodeError::Timeout,
            PluginError::Disconnected => {
                NodeError::Execution(format!("plugin `{}` disconnected", self.client.info().id))
            }
            other => NodeError::Execution(other.to_string()),
        }
    }
}

impl NodeExecutor for PluginExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        self.descriptor.clone()
    }

    fn execute(
        &self,
        input: NodeInput,
        context: &mut rf_core::ExecutionContext,
    ) -> Result<NodeOutput, NodeError> {
        context.check_cancelled()?;
        let params = ExecuteParams {
            run_id: context.run_id().to_string(),
            node_id: input.node_id.clone(),
            node_type: input.node_type.clone(),
            config: input.resolved_config.clone(),
            inputs: input.inputs.clone(),
            variables: context.variables().clone(),
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
