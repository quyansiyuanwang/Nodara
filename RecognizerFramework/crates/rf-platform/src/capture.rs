//! Screen capture.

use rf_core::{
    ArtifactMeta, ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult,
};
use rf_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

use crate::native;

/// Capture a rectangle and store it as a PNG artefact.
pub fn capture_region(
    context: &ExecutionContext,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    name: &str,
) -> NodeResult<ArtifactMeta> {
    let bgra = native::capture(x, y, width, height)
        .map_err(|error| NodeError::Execution(error.to_string()))?;
    let png = encode_png(&bgra, width, height)?;
    Ok(context.artifacts().put(name, "image/png", png))
}

/// Convert BGRA pixels to a PNG byte stream.
pub fn encode_png(bgra: &[u8], width: u32, height: u32) -> NodeResult<Vec<u8>> {
    let expected = (width as usize) * (height as usize) * 4;
    if bgra.len() < expected {
        return Err(NodeError::Execution(format!(
            "capture buffer is {} bytes but {expected} were expected",
            bgra.len()
        )));
    }
    let mut rgba = Vec::with_capacity(expected);
    for pixel in bgra[..expected].chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 0xFF]);
    }
    let buffer = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| NodeError::Execution("capture dimensions are inconsistent".to_string()))?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buffer)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| NodeError::Execution(error.to_string()))?;
    Ok(bytes.into_inner())
}

/// `windows.Desktop.Capture`
#[derive(Debug, Default)]
pub struct DesktopCaptureExecutor;

impl NodeExecutor for DesktopCaptureExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![PortDescriptor::new(
                "in",
                "In",
                PortKind::Input,
                ValueType::Any,
            )],
            outputs: vec![PortDescriptor::new(
                "artifact",
                "Artifact",
                PortKind::Output,
                ValueType::Image,
            )],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "x": {
                        "type": "integer",
                        "title": "X",
                        "description": "Left edge of the region, in screen pixels.",
                        "default": 0
                    },
                    "y": {
                        "type": "integer",
                        "title": "Y",
                        "description": "Top edge of the region, in screen pixels.",
                        "default": 0
                    },
                    "width": {
                        "type": "integer",
                        "title": "Width",
                        "description": "Width of the region in pixels. Defaults to the whole \
                                        primary display.",
                        "minimum": 1
                    },
                    "height": {
                        "type": "integer",
                        "title": "Height",
                        "description": "Height of the region in pixels. Defaults to the whole \
                                        primary display.",
                        "minimum": 1
                    },
                    "output_var": {
                        "type": "string",
                        "title": "Output variable",
                        "description": "Variable receiving the captured artefact metadata."
                    }
                },
                "required": ["output_var"],
                "additionalProperties": false
            }),
            permissions: vec!["screen.capture".to_string()],
            allows_additional_config: false,
            ..NodeDescriptor::new("windows.Desktop.Capture", "Capture Desktop", "Desktop")
                .with_description("Captures the whole primary display or a region of it")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let output_var = input.require_str("output_var")?;
        let (screen_width, screen_height) = native::screen_size();
        let x = input.config_i64("x").unwrap_or(0) as i32;
        let y = input.config_i64("y").unwrap_or(0) as i32;
        let width = input
            .config_i64("width")
            .map_or(screen_width.max(0) as u32, |value| value as u32);
        let height = input
            .config_i64("height")
            .map_or(screen_height.max(0) as u32, |value| value as u32);

        let meta = capture_region(context, x, y, width, height, "desktop")?;
        context.set_variable(output_var, serde_json::to_value(&meta).unwrap_or_default());
        Ok(NodeOutput::new()
            .with_output("artifact", serde_json::to_value(meta).unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_bgra_as_png() {
        let width = 2;
        let height = 2;
        let mut bgra = Vec::new();
        for _ in 0..(width * height) {
            bgra.extend_from_slice(&[0x10, 0x20, 0x30, 0xFF]);
        }
        let png = encode_png(&bgra, width, height).expect("png encodes");
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn rejects_short_buffers() {
        assert!(encode_png(&[0, 0, 0], 2, 2).is_err());
    }
}
