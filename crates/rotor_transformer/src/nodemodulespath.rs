//! `nodemodulespath.go`

use std::collections::HashMap;

use rotor_rojo::canonical_file_name;
use rotor_support::gopath;

/// `NodeModulesPathMapping`: each type root package's types entry to its shipped runtime path.
///
/// Exact keys are canonicalized with `rojo.CanonicalFileName`; patterns cover wildcard export
/// subpaths (`./x/*`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeModulesPathMapping {
	/// Canonical types path to runtime path.
	pub exact: HashMap<String, String>,
	/// Wildcard export subpaths.
	pub patterns: Vec<NodeModulesPathPattern>,
}

/// `NodeModulesPathPattern`: a types file matches when its canonical path starts with
/// `types_prefix` and ends with `types_suffix`; the text between replaces every `*` in `runtime`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeModulesPathPattern {
	/// The canonical prefix.
	pub types_prefix: String,
	/// The canonical suffix.
	pub types_suffix: String,
	/// The runtime path with `*` placeholders.
	pub runtime: String,
}

impl NodeModulesPathMapping {
	/// `Lookup`: an exact key first, then the matching pattern with the longest prefix.
	#[must_use]
	pub fn lookup(&self, file_path: &str, use_case_sensitive_file_names: bool) -> Option<String> {
		let key = canonical_file_name(file_path, use_case_sensitive_file_names);
		if let Some(mapped) = self.exact.get(&key) {
			return Some(mapped.clone());
		}

		let mut best: Option<usize> = None;
		for (index, pattern) in self.patterns.iter().enumerate() {
			if key.len() < pattern.types_prefix.len() + pattern.types_suffix.len()
				|| !key.starts_with(&pattern.types_prefix)
				|| !key.ends_with(&pattern.types_suffix)
			{
				continue;
			}
			if best.is_none_or(|best| pattern.types_prefix.len() > self.patterns[best].types_prefix.len()) {
				best = Some(index);
			}
		}
		let pattern = &self.patterns[best?];

		// Take the star text from the original path so case-insensitive file systems keep the
		// import's casing; fall back to the key if lowercasing changed the byte length.
		let mut source = gopath::clean(&gopath::from_slash(file_path));
		if source.len() != key.len() {
			source.clone_from(&key);
		}
		// Go slices by bytes, so a cut inside a multi-byte character still substitutes. A Rust
		// string cannot hold the split bytes; they come out as U+FFFD.
		let star = &source.as_bytes()[pattern.types_prefix.len()..source.len() - pattern.types_suffix.len()];
		let mut runtime = Vec::with_capacity(pattern.runtime.len() + star.len());
		for &byte in pattern.runtime.as_bytes() {
			if byte == b'*' {
				runtime.extend_from_slice(star);
			} else {
				runtime.push(byte);
			}
		}
		Some(String::from_utf8_lossy(&runtime).into_owned())
	}
}
