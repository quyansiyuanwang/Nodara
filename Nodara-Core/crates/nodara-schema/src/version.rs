//! Version constants for the three independent version axes.
//!
//! The project never lets one axis silently imply another. A workflow document,
//! a plugin wire handshake and an HTTP API client each negotiate their own
//! version so that, for example, a new node type can ship without bumping the
//! transport protocol.

/// Workflow document format version communicated through `schema_version`.
pub const SCHEMA_VERSION: &str = "2.1";

/// Version of the runtime <-> plugin wire protocol (`protocol_version`).
pub const PROTOCOL_VERSION: &str = "1";

/// Public runtime HTTP/WebSocket API version.
pub const API_VERSION: &str = "v1";

/// Returns `true` when `version` has the same major component as [`SCHEMA_VERSION`].
///
/// Minor versions are forward compatible by construction: unknown fields are
/// preserved and unknown node types are rejected only by capability-aware
/// validation, never by the parser.
pub fn is_compatible_schema_version(version: &str) -> bool {
    major_of(version) == major_of(SCHEMA_VERSION)
}

/// Extract the leading numeric component of a dotted version string.
///
/// Non-numeric or malformed input yields `0`, which fails every compatibility
/// check and therefore surfaces the problem as a validation error instead of a
/// panic.
pub fn major_of(version: &str) -> u32 {
    version
        .trim()
        .trim_start_matches('v')
        .split(['.', '-'])
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_major_components() {
        assert_eq!(major_of("2.0"), 2);
        assert_eq!(major_of("v1.4.3"), 1);
        assert_eq!(major_of("1"), 1);
        assert_eq!(major_of("garbage"), 0);
    }

    #[test]
    fn compatibility_is_major_only() {
        assert!(is_compatible_schema_version("2.1"));
        assert!(is_compatible_schema_version("2.5"));
        assert!(!is_compatible_schema_version("1.0"));
        assert!(!is_compatible_schema_version("3.0"));
    }
}
