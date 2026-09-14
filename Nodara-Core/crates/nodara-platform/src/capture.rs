//! Screen capture.

use nodara_core::{
    ArtifactMeta, ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult,
};
use nodara_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

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
        let width = dimension(input.config_i64("width"), screen_width, "width")?;
        let height = dimension(input.config_i64("height"), screen_height, "height")?;

        let meta = capture_region(context, x, y, width, height, "desktop")?;
        context.set_variable(output_var, serde_json::to_value(&meta).unwrap_or_default());
        Ok(NodeOutput::new()
            .with_output("artifact", serde_json::to_value(meta).unwrap_or_default()))
    }
}

/// Resolve a capture dimension, rejecting values that would wrap around or
/// attempt an absurd allocation.
///
/// A negative width cast straight to `u32` becomes ~4 billion pixels; the
/// buffer for that aborts the process before any error can be reported.
fn dimension(value: Option<i64>, screen: i32, name: &str) -> NodeResult<u32> {
    const MAX_DIMENSION: i64 = 100_000;

    let raw = value.unwrap_or(screen.max(0) as i64);
    if raw < 1 {
        return Err(NodeError::InvalidConfig(format!(
            "`{name}` must be a positive number of pixels, got {raw}"
        )));
    }
    if raw > MAX_DIMENSION {
        return Err(NodeError::InvalidConfig(format!(
            "`{name}` may be at most {MAX_DIMENSION} pixels, got {raw}"
        )));
    }
    Ok(raw as u32)
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

#[cfg(test)]
mod dimension_tests {
    use super::*;

    #[test]
    fn defaults_to_the_screen_size() {
        assert_eq!(dimension(None, 1920, "width").unwrap(), 1920);
    }

    #[test]
    fn rejects_non_positive_and_oversized_values() {
        assert_eq!(
            dimension(Some(-1), 1920, "width").unwrap_err().code(),
            "E_INVALID_CONFIG"
        );
        assert_eq!(
            dimension(Some(0), 1920, "height").unwrap_err().code(),
            "E_INVALID_CONFIG"
        );
        assert_eq!(
            dimension(Some(10_000_000), 1920, "width")
                .unwrap_err()
                .code(),
            "E_INVALID_CONFIG"
        );
    }
}
