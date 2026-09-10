//! Tool selection.
//!
//! Before the agent asks a model anything, it narrows the runtime's node
//! catalogue to the tools that could plausibly serve the goal. Two things follow:
//!
//! * the prompt stays small and cheap even when dozens of plugins are installed;
//! * the model is not tempted into an irrelevant capability, which is a common
//!   source of workflows that validate but do not make sense.
//!
//! Selection never widens authority. It only removes candidates; the guardrails
//! and the runtime's policy layer still see every node that survives.

use std::collections::BTreeSet;

use rf_schema::NodeDescriptor;

/// Node types that every plan needs, regardless of the goal.
pub const ALWAYS_INCLUDED: &[&str] = &["core.Start", "core.End", "core.Log"];

/// Chooses a subset of the catalogue for one goal.
#[derive(Debug, Clone)]
pub struct ToolSelector {
    /// Always include the workflow scaffolding nodes.
    pub always_include_core: bool,
    /// Hard cap on how many node types reach the prompt.
    pub max_tools: usize,
}

impl Default for ToolSelector {
    fn default() -> Self {
        Self {
            always_include_core: true,
            max_tools: 40,
        }
    }
}

impl ToolSelector {
    /// Select the tools relevant to `goal`.
    ///
    /// Scoring is deliberately simple and explainable rather than clever: an
    /// exact token match on the node type or its category counts for more than a
    /// match in the prose description. A ranking that can be read out loud is
    /// easier to debug than an embedding, and the catalogue is small.
    pub fn select(&self, goal: &str, descriptors: &[NodeDescriptor]) -> Vec<NodeDescriptor> {
        let tokens = tokenise(goal);
        if tokens.is_empty() {
            let mut all = descriptors.to_vec();
            all.truncate(self.max_tools);
            return all;
        }

        let mut scored: Vec<(i32, &NodeDescriptor)> = descriptors
            .iter()
            .map(|descriptor| (score(&tokens, descriptor), descriptor))
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.node_type.cmp(&b.1.node_type))
        });

        let mut selected: Vec<NodeDescriptor> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();

        if self.always_include_core {
            for wanted in ALWAYS_INCLUDED {
                if let Some(descriptor) = descriptors
                    .iter()
                    .find(|descriptor| descriptor.node_type == *wanted)
                {
                    if seen.insert(descriptor.node_type.clone()) {
                        selected.push(descriptor.clone());
                    }
                }
            }
        }

        for (points, descriptor) in scored {
            if !seen.insert(descriptor.node_type.clone()) {
                continue;
            }
            // A zero score means the goal said nothing about this tool. Keep it
            // only while there is room, so an unrecognised goal still gets a
            // usable catalogue rather than an empty one.
            if points == 0 && selected.len() >= self.max_tools / 2 {
                continue;
            }
            selected.push(descriptor.clone());
            if selected.len() >= self.max_tools {
                break;
            }
        }

        selected
    }
}

fn score(tokens: &BTreeSet<String>, descriptor: &NodeDescriptor) -> i32 {
    let node_type = descriptor.node_type.to_lowercase();
    let category = descriptor.category.to_lowercase();
    let display = descriptor.display_name.to_lowercase();
    let description = descriptor.description.to_lowercase();

    let mut points = 0;
    for token in tokens {
        if node_type.split('.').any(|segment| segment == token) {
            points += 6;
        }
        if category == *token {
            points += 4;
        }
        if display.split_whitespace().any(|word| word == token) {
            points += 3;
        }
        if description.split_whitespace().any(|word| word == token) {
            points += 1;
        }
    }
    points
}

/// Lower-case the goal and drop noise words, so scoring is on intent.
fn tokenise(goal: &str) -> BTreeSet<String> {
    const STOP_WORDS: &[&str] = &[
        "a", "an", "and", "the", "to", "of", "for", "with", "then", "into", "on", "in", "it",
        "that", "this", "from", "at", "by", "is", "are", "be", "my", "me", "i", "please",
    ];
    goal.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 1 && !STOP_WORDS.contains(word))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalogue() -> Vec<NodeDescriptor> {
        vec![
            NodeDescriptor::new("core.Start", "Start", "Core"),
            NodeDescriptor::new("core.End", "End", "Core"),
            NodeDescriptor::new("core.Log", "Log", "Core"),
            NodeDescriptor {
                description: "Sends a key or key chord to the focused window".to_string(),
                ..NodeDescriptor::new("windows.Input.Keyboard", "Keyboard", "Input")
            },
            NodeDescriptor {
                description: "Captures the whole primary display".to_string(),
                ..NodeDescriptor::new("windows.Desktop.Capture", "Capture Desktop", "Desktop")
            },
            NodeDescriptor {
                description: "Extracts text from an image".to_string(),
                ..NodeDescriptor::new("vision.Ocr", "OCR", "Vision")
            },
        ]
    }

    #[test]
    fn a_keyboard_goal_ranks_the_keyboard_first() {
        let selector = ToolSelector::default();
        let selected = selector.select("press ctrl and s on the keyboard", &catalogue());
        let order: Vec<&str> = selected
            .iter()
            .map(|descriptor| descriptor.node_type.as_str())
            .collect();
        let keyboard = order
            .iter()
            .position(|node_type| *node_type == "windows.Input.Keyboard")
            .expect("the keyboard must be selected");
        // Core scaffolding is always first by design; within the scored tools
        // the relevant one must outrank the unrelated ones.
        let irrelevant = order
            .iter()
            .position(|node_type| *node_type == "vision.Ocr")
            .expect("ocr is still offered as a candidate");
        assert!(
            keyboard < irrelevant,
            "expected the keyboard to outrank an unrelated tool: {order:?}"
        );
    }

    #[test]
    fn core_scaffolding_is_always_present() {
        let selector = ToolSelector::default();
        let selected = selector.select("something entirely unrelated", &catalogue());
        let types: Vec<&str> = selected
            .iter()
            .map(|descriptor| descriptor.node_type.as_str())
            .collect();
        assert!(types.contains(&"core.Start"));
        assert!(types.contains(&"core.End"));
        assert!(types.contains(&"core.Log"));
    }

    #[test]
    fn selection_never_invents_a_node_type() {
        let selector = ToolSelector::default();
        let catalogue = catalogue();
        let selected = selector.select("read the screen with ocr", &catalogue);
        for descriptor in &selected {
            assert!(
                catalogue
                    .iter()
                    .any(|candidate| candidate.node_type == descriptor.node_type),
                "selector produced a node type that is not installed"
            );
        }
    }

    #[test]
    fn the_cap_is_respected() {
        let selector = ToolSelector {
            max_tools: 3,
            ..ToolSelector::default()
        };
        let selected = selector.select("do everything everywhere", &catalogue());
        assert!(selected.len() <= 3, "got {} tools", selected.len());
    }

    #[test]
    fn a_blank_goal_still_yields_a_catalogue() {
        let selector = ToolSelector::default();
        let selected = selector.select("   ", &catalogue());
        assert_eq!(selected.len(), 6);
    }
}
