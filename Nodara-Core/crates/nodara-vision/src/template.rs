//! Template matching.
//!
//! Finds the best placement of a template inside a frame using zero-mean
//! normalised cross-correlation, which is insensitive to uniform brightness
//! changes. A coarse-to-fine search keeps a full-HD frame cheap to scan.

use nodara_core::{ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use nodara_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

use crate::error::VisionError;

/// Where the best match was found.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Match {
    /// Whether the score cleared the threshold.
    pub found: bool,
    /// Correlation score in `-1.0..=1.0`.
    pub score: f32,
    /// Left edge of the match.
    pub x: i32,
    /// Top edge of the match.
    pub y: i32,
    /// Template width.
    pub width: u32,
    /// Template height.
    pub height: u32,
}

fn statistics(pixels: &[u8]) -> (f32, f32) {
    if pixels.is_empty() {
        return (0.0, 0.0);
    }
    let n = pixels.len() as f32;
    let mean = pixels.iter().map(|p| f32::from(*p)).sum::<f32>() / n;
    let variance = pixels
        .iter()
        .map(|p| {
            let delta = f32::from(*p) - mean;
            delta * delta
        })
        .sum::<f32>()
        / n;
    (mean, variance.sqrt())
}

/// Zero-mean normalised cross-correlation of `needle` at `(ox, oy)`.
/// One image being searched, paired with its width.
#[derive(Clone, Copy)]
struct Frame<'a> {
    pixels: &'a [u8],
    width: u32,
}

/// One template being searched for, with its statistics.
#[derive(Clone, Copy)]
struct Needle<'a> {
    pixels: &'a [u8],
    width: u32,
    height: u32,
    mean: f32,
    std_dev: f32,
}

fn correlation(frame: Frame<'_>, needle: Needle<'_>, ox: u32, oy: u32) -> f32 {
    let Frame {
        pixels: haystack,
        width: haystack_width,
    } = frame;
    let Needle {
        pixels: needle,
        width: needle_width,
        height: needle_height,
        mean: needle_mean,
        std_dev: needle_std,
    } = needle;
    if needle_std == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0f32;
    let mut sum_squares = 0.0f32;
    let count = (needle_width * needle_height) as f32;
    for ty in 0..needle_height {
        let haystack_row = ((oy + ty) * haystack_width + ox) as usize;
        for tx in 0..needle_width {
            let value = f32::from(haystack[haystack_row + tx as usize]);
            sum += value;
            sum_squares += value * value;
        }
    }
    let mean = sum / count;
    let variance = (sum_squares / count) - mean * mean;
    let std = variance.max(0.0).sqrt();
    if std == 0.0 {
        return 0.0;
    }
    let mut covariance = 0.0f32;
    for ty in 0..needle_height {
        let haystack_row = ((oy + ty) * haystack_width + ox) as usize;
        let needle_row = (ty * needle_width) as usize;
        for tx in 0..needle_width {
            let h = f32::from(haystack[haystack_row + tx as usize]) - mean;
            let n = f32::from(needle[needle_row + tx as usize]) - needle_mean;
            covariance += h * n;
        }
    }
    covariance / (count * std * needle_std)
}

/// Search `haystack` for `needle`, returning the best match if any position was
/// evaluated.
pub fn match_template(
    haystack: &[u8],
    haystack_width: u32,
    haystack_height: u32,
    needle: &[u8],
    needle_width: u32,
    needle_height: u32,
    threshold: f32,
) -> Result<Match, VisionError> {
    if needle_width > haystack_width || needle_height > haystack_height {
        return Err(VisionError::TemplateTooLarge {
            tw: needle_width,
            th: needle_height,
            fw: haystack_width,
            fh: haystack_height,
        });
    }
    let (needle_mean, needle_std) = statistics(needle);
    let max_x = haystack_width - needle_width;
    let max_y = haystack_height - needle_height;
    let frame = Frame {
        pixels: haystack,
        width: haystack_width,
    };
    let template = Needle {
        pixels: needle,
        width: needle_width,
        height: needle_height,
        mean: needle_mean,
        std_dev: needle_std,
    };

    let coarse_stride = (needle_width.min(needle_height) / 4).max(1);
    let mut best = (f32::MIN, 0u32, 0u32);
    let evaluate = |ox: u32, oy: u32, best: &mut (f32, u32, u32)| {
        let score = correlation(frame, template, ox, oy);
        if score > best.0 {
            *best = (score, ox, oy);
        }
    };

    let mut oy = 0;
    while oy <= max_y {
        let mut ox = 0;
        while ox <= max_x {
            evaluate(ox, oy, &mut best);
            ox += coarse_stride;
        }
        oy += coarse_stride;
    }

    // Refine at full resolution around the coarse winner.
    let (_, best_x, best_y) = best;
    let low_x = best_x.saturating_sub(coarse_stride);
    let low_y = best_y.saturating_sub(coarse_stride);
    let high_x = (best_x + coarse_stride).min(max_x);
    let high_y = (best_y + coarse_stride).min(max_y);
    for oy in low_y..=high_y {
        for ox in low_x..=high_x {
            evaluate(ox, oy, &mut best);
        }
    }

    let (score, x, y) = best;
    Ok(Match {
        found: score >= threshold,
        score,
        x: x as i32,
        y: y as i32,
        width: needle_width,
        height: needle_height,
    })
}

/// `vision.TemplateMatch`
#[derive(Debug, Default)]
pub struct TemplateMatchExecutor;

impl NodeExecutor for TemplateMatchExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![PortDescriptor::new(
                "in",
                "In",
                PortKind::Input,
                ValueType::Any,
            )],
            outputs: vec![PortDescriptor::new(
                "match",
                "Match",
                PortKind::Output,
                ValueType::Object,
            )],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "frame": {
                        "type": "string",
                        "title": "Frame",
                        "description": "Artefact id produced by a capture node, or a path to an \
                                        image file to search."
                    },
                    "template": {
                        "type": "string",
                        "title": "Template",
                        "description": "Artefact id or image path of the template to find."
                    },
                    "threshold": {
                        "type": "number",
                        "title": "Threshold",
                        "description": "Minimum normalized cross-correlation score (ZNCC) for a \
                                        match.",
                        "minimum": -1,
                        "maximum": 1,
                        "default": 0.8
                    },
                    "output_var": {
                        "type": "string",
                        "title": "Output variable",
                        "description": "Variable receiving the match (`found`, `score`, `x`, \
                                        `y`, `width`, `height`)."
                    }
                },
                "required": ["frame", "template", "output_var"],
                "additionalProperties": false
            }),
            permissions: vec!["vision.analyze".to_string()],
            allows_additional_config: false,
            ..NodeDescriptor::new("vision.TemplateMatch", "Template Match", "Vision")
                .with_description("Locates a template image inside a captured frame")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let output_var = input.require_str("output_var")?;
        let frame_source = input.require_str("frame")?;
        let template_source = input.require_str("template")?;
        let threshold = input.config_f64("threshold").unwrap_or(0.8) as f32;

        let frame = load_gray(context, &frame_source)?;
        let template = load_gray(context, &template_source)?;
        let result = match_template(
            &frame.0,
            frame.1,
            frame.2,
            &template.0,
            template.1,
            template.2,
            threshold,
        )
        .map_err(|error| NodeError::Execution(error.to_string()))?;

        let value = serde_json::to_value(&result)
            .map_err(|error| NodeError::Execution(error.to_string()))?;
        context.set_variable(output_var, value.clone());
        context.log(
            nodara_schema::LogLevel::Info,
            format!(
                "template match score {:.3} at ({}, {})",
                result.score, result.x, result.y
            ),
        );
        Ok(NodeOutput::new().with_output("match", value))
    }
}

/// Load an image either from the artefact store or from disk, as grayscale.
fn load_gray(context: &ExecutionContext, source: &str) -> NodeResult<(Vec<u8>, u32, u32)> {
    let bytes = if let Some(bytes) = context.artifacts().get(source) {
        bytes
    } else {
        std::fs::read(source).map_err(|error| {
            NodeError::Execution(VisionError::NotFound(format!("{source}: {error}")).to_string())
        })?
    };
    let image = image::load_from_memory(&bytes)
        .map_err(|error| NodeError::Execution(VisionError::Image(error.to_string()).to_string()))?
        .to_luma8();
    let (width, height) = image.dimensions();
    Ok((image.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> (Vec<u8>, u32, u32) {
        let mut pixels = vec![10u8; 40 * 30];
        // Stamp a 4x3 block with a distinctive pattern at (12, 7).
        let pattern = [200u8, 240, 30, 90, 180, 60, 20, 250, 90, 120, 210, 40];
        for y in 0..3u32 {
            for x in 0..4u32 {
                pixels[((7 + y) * 40 + 12 + x) as usize] = pattern[(y * 4 + x) as usize];
            }
        }
        (pixels, 40, 30)
    }

    fn needle() -> (Vec<u8>, u32, u32) {
        (
            vec![200, 240, 30, 90, 180, 60, 20, 250, 90, 120, 210, 40],
            4,
            3,
        )
    }

    #[test]
    fn finds_an_exact_template() {
        let (frame, fw, fh) = frame();
        let (needle, nw, nh) = needle();
        let result = match_template(&frame, fw, fh, &needle, nw, nh, 0.8).unwrap();
        assert!(result.found);
        assert_eq!((result.x, result.y), (12, 7));
        assert!(
            (result.score - 1.0).abs() < 1e-3,
            "score was {}",
            result.score
        );
    }

    #[test]
    fn reports_a_low_score_when_absent() {
        let (frame, fw, fh) = frame();
        let absent = vec![7u8; 4 * 3];
        let result = match_template(&frame, fw, fh, &absent, 4, 3, 0.9).unwrap();
        assert!(!result.found);
    }

    #[test]
    fn rejects_templates_larger_than_the_frame() {
        let result = match_template(&[0; 16], 4, 4, &[0; 25], 5, 5, 0.8);
        assert!(matches!(result, Err(VisionError::TemplateTooLarge { .. })));
    }

    #[test]
    fn correlation_is_brightness_invariant() {
        let (frame, fw, fh) = frame();
        let needle = vec![200, 240, 30, 90, 180, 60, 20, 250, 90, 120, 210, 40];
        let brightened_frame: Vec<u8> = frame.iter().map(|p| p.saturating_add(20)).collect();
        let result = match_template(&brightened_frame, fw, fh, &needle, 4, 3, 0.8).unwrap();
        assert!(result.found, "score was {}", result.score);
        assert_eq!((result.x, result.y), (12, 7));
    }
}
