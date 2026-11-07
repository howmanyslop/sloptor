//! The single source of truth for rotor's release version.
//!
//! It is defined in code rather than injected by the build, so every build
//! path (`cargo build`, `cargo install`, release builds) reports the same
//! version.

/// rotor's release version, without the leading `v`.
///
/// Bumping this constant is part of cutting a release: the release workflow
/// refuses to publish a tag that doesn't match it (tag `vX.Y.Z` must equal `v`
/// plus this constant).
pub const VERSION: &str = "3.0.0-beta.4";
