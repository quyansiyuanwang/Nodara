//! The `vision.Ocr` node.

use nodara_core::{ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use nodara_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

use crate::backend::ocr_backend;
use crate::error::VisionError;

/// `vision.Ocr`
#[derive(Debug, Default)]
pub struct OcrExecutor;

impl NodeExecutor for OcrExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![PortDescriptor::new(
                "in",
                "In",
                PortKind::Input,
                ValueType::Any,
            )],
            outputs: vec![PortDescriptor::new(
                "text",
                "Text",
                PortKind::Output,
                ValueType::String,
            )],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "image": {
                        "type": "string",
                        "title": "Image",
                        "description": "Artefact id produced by a capture node, or a path to an \
                                        image file to read."
                    },
                    "language": {
                        "type": "string",
                        "title": "Language",
                        "description": "BCP-47 language tag passed to the OCR backend, when it \
                                        supports one.",
                        "examples": ["en-US", "zh-CN"]
                    },
                    "output_var": {
                        "type": "string",
                        "title": "Output variable",
                        "description": "Variable receiving the recognized text."
                    }
                },
                "required": ["image", "output_var"],
                "additionalProperties": false
            }),
            permissions: vec!["vision.analyze".to_string()],
            allows_additional_config: false,
            ..NodeDescriptor::new("vision.Ocr", "OCR", "Vision")
                .with_description("Extracts text from an image using the configured backend")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let output_var = input.require_str("output_var")?;
        let source = input.require_str("image")?;
        let language = input.config_str("language");

        let Some(backend) = ocr_backend() else {
            return Err(NodeError::Unsupported(
                VisionError::NoOcrBackend.to_string(),
            ));
        };

        let bytes = if let Some(bytes) = context.artifacts().get(&source) {
            bytes
        } else {
            std::fs::read(&source).map_err(|error| {
                NodeError::Execution(
                    VisionError::NotFound(format!("{source}: {error}")).to_string(),
                )
            })?
        };

        let result = backend
            .recognize(&bytes, language)
            .map_err(|error| NodeError::Execution(error.to_string()))?;
        context.set_variable(output_var, serde_json::json!(result.text));
        context.log(
            nodara_schema::LogLevel::Info,
            format!(
                "ocr backend `{}` recognised {} characters",
                backend.name(),
                result.text.chars().count()
            ),
        );
        Ok(NodeOutput::new().with_output("text", serde_json::json!(result.text)))
    }
}
