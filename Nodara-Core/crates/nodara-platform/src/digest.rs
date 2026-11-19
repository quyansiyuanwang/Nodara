//! Read-only screenshot inventories and mail delivery to a host-configured recipient.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use lettre::{transport::smtp::authentication::Credentials, Message, SmtpTransport, Transport};
use nodara_core::{ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult};
use nodara_schema::{LogLevel, NodeDescriptor, PortDescriptor, PortKind, ValueType};
use serde_json::json;

#[derive(Debug)]
pub(crate) struct ScreenshotIndex;

fn image_group(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if ![
        "screenshot",
        "screen shot",
        "screen capture",
        "截屏",
        "截图",
        "屏幕快照",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
    {
        return None;
    }
    match lower.rsplit_once('.')?.1 {
        "png" => Some("PNG"),
        "jpeg" | "jpg" => Some("JPEG"),
        "webp" => Some("WebP"),
        _ => None,
    }
}

fn desktop_names(path: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if image_group(&name).is_some() && entry.file_type()?.is_file() {
            if names.len() >= 500 {
                return Err(std::io::Error::other(
                    "More than 500 screenshots; narrow the inventory before sending",
                ));
            }
            names.push(name);
        }
    }
    Ok(names)
}

fn digest(names: Vec<String>) -> String {
    let mut groups = BTreeMap::<_, Vec<_>>::new();
    for name in names {
        if let Some(group) = image_group(&name) {
            groups
                .entry(group)
                .or_default()
                .push(name.replace(char::is_control, " "));
        }
    }
    let mut body = format!(
        "Desktop screenshot inventory: {} files\n",
        groups.values().map(Vec::len).sum::<usize>()
    );
    for (group, mut files) in groups {
        files.sort();
        body.push_str(&format!(
            "\n{group} ({})\n{}\n",
            files.len(),
            files.join("\n")
        ));
    }
    body
}

impl NodeExecutor for ScreenshotIndex {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            permissions: vec!["desktop.list".into()],
            dangerous: true,
            outputs: vec![PortDescriptor::new(
                "digest",
                "Inventory",
                PortKind::Output,
                ValueType::String,
            )],
            config_schema: json!({"type":"object","properties":{"source":{"type":"string","enum":["sample","desktop"],"default":"sample"}},"additionalProperties":false}),
            ..NodeDescriptor::new("desktop.ScreenshotIndex", "Index screenshots", "Desktop")
                .with_description(
                "Groups screenshot filenames by format. Sample mode never accesses the filesystem.",
            )
        }
    }

    fn execute(&self, input: NodeInput, _: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let names = match input.config_str("source").unwrap_or("sample") {
            "sample" => [
                "Screenshot - Canvas.png",
                "Screenshot - Inbox.jpg",
                "Screenshot - Settings.png",
            ]
            .map(String::from)
            .to_vec(),
            "desktop" => desktop_names(
                &dirs::desktop_dir()
                    .ok_or_else(|| NodeError::Execution("Desktop folder unavailable".into()))?,
            )
            .map_err(|e| NodeError::Execution(format!("Cannot list Desktop: {e}")))?,
            _ => return Err(NodeError::InvalidConfig("Unknown inventory source".into())),
        };
        Ok(NodeOutput::new().with_output("digest", json!(digest(names))))
    }
}

#[derive(Debug)]
pub(crate) struct DigestMail;

fn host_setting(key: &str) -> NodeResult<String> {
    std::env::var(key)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            NodeError::InvalidConfig(format!(
                "Set {key} on the runtime host before SMTP delivery"
            ))
        })
}

impl NodeExecutor for DigestMail {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            dangerous: true, permissions: vec!["mail.send".into()],
            inputs: vec![PortDescriptor::new("digest", "Inventory", PortKind::Input, ValueType::String).required(true)],
            outputs: vec![
                PortDescriptor::new("message", "Email", PortKind::Output, ValueType::Object),
                PortDescriptor::new("delivery", "Delivery result", PortKind::Output, ValueType::String),
            ],
            config_schema: json!({"type":"object","properties":{"delivery":{"type":"string","enum":["preview","smtp"],"default":"preview"}},"additionalProperties":false}),
            ..NodeDescriptor::new("mail.Digest", "Deliver inventory", "Mail").with_description("Saves a local .eml preview or submits the inventory to the host's configured SMTP recipient over TLS.")
        }
    }

    fn execute(&self, input: NodeInput, ctx: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        let body = input
            .input("digest")
            .and_then(|v| v.as_str())
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| {
                NodeError::InvalidConfig("Connect an inventory to the digest input".into())
            })?;
        let (from, to, send) = match input.config_str("delivery").unwrap_or("preview") {
            "preview" => (
                "studio@example.invalid".into(),
                "preview@example.invalid".into(),
                false,
            ),
            "smtp" => (
                host_setting("NODARA_MAIL_FROM")?,
                host_setting("NODARA_MAIL_TO")?,
                true,
            ),
            _ => {
                return Err(NodeError::InvalidConfig(
                    "Unknown mail delivery mode".into(),
                ))
            }
        };
        let parse = |s: &str| {
            s.parse()
                .map_err(|_| NodeError::InvalidConfig("Invalid host mail address".into()))
        };
        let message = Message::builder()
            .from(parse(&from)?)
            .to(parse(&to)?)
            .subject("Desktop screenshot inventory")
            .body(body.to_string())
            .map_err(|e| NodeError::InvalidConfig(e.to_string()))?;
        if send {
            let host = host_setting("NODARA_SMTP_HOST")?;
            let transport = SmtpTransport::relay(&host)
                .map_err(|_| NodeError::InvalidConfig("Invalid SMTP relay".into()))?
                .timeout(Some(Duration::from_secs(15)))
                .credentials(Credentials::new(
                    host_setting("NODARA_SMTP_USER")?,
                    host_setting("NODARA_SMTP_PASSWORD")?,
                ))
                .build();
            transport
                .send(&message)
                .map_err(|e| NodeError::Execution(format!("SMTP submission failed: {e}")))?;
        }
        let artifact = ctx.artifacts().put(
            "screenshot-inventory.eml",
            "message/rfc822",
            message.formatted(),
        );
        ctx.log(
            LogLevel::Info,
            if send {
                "Message accepted by SMTP relay"
            } else {
                "Local mail preview saved; nothing was sent"
            },
        );
        Ok(NodeOutput::new()
            .with_output("message", json!(artifact))
            .with_output(
                "delivery",
                json!(if send { "smtp_accepted" } else { "preview" }),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_groups_only_screenshots_in_a_stable_order() {
        let body = digest(
            [
                "Screenshot B.PNG",
                "notes.png",
                "截图.jpg",
                "Screenshot A.png",
                "Screenshot.txt",
            ]
            .map(String::from)
            .to_vec(),
        );
        assert_eq!(body, "Desktop screenshot inventory: 3 files\n\nJPEG (1)\n截图.jpg\n\nPNG (2)\nScreenshot A.png\nScreenshot B.PNG\n");
    }
}
