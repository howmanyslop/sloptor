//! The native Flamework transformer's settings and their validation.

use std::collections::BTreeMap;
use std::fmt;

use rotor_support::gopath::Flavor;
use rotor_support::gostr::quote;

use super::model::Config;

/// The `flamework` section of `rotor.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
	clippy::struct_excessive_bools,
	reason = "one field per boolean key of the rotor.toml section"
)]
pub struct FlameworkConfig {
	/// Run native Flamework after this remaining TypeScript transformer.
	pub after: String,
	/// Disable TypeScript semantic diagnostics during transformation.
	pub no_semantic_diagnostics: bool,
	/// Enable identifier and event-name obfuscation.
	pub obfuscation: bool,
	/// The identifier generation mode: `full`, `obfuscated`, `short` or `tiny`.
	pub id_generation_mode: String,
	/// An optional identifier hash prefix; values beginning with `$` are
	/// reserved.
	pub hash_prefix: String,
	/// The salt used for hashes.
	pub salt: String,
	/// Generate identifiers for exports.
	pub preload_ids: bool,
	/// Reuse the original parse tree for sources Flamework leaves unchanged
	/// instead of reprinting them (opts out of upstream source-print parity).
	pub skip_unchanged_files: bool,
	/// Tuning knobs.
	pub optimizations: FlameworkOptimizations,
	/// Per-tsconfig settings, keyed by the path as written.
	pub profiles: BTreeMap<String, FlameworkProfile>,
}

/// Per-tsconfig Flamework settings: [`FlameworkConfig`] without `profiles`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
	clippy::struct_excessive_bools,
	reason = "one field per boolean key of the rotor.toml section"
)]
pub struct FlameworkProfile {
	/// Run native Flamework after this remaining TypeScript transformer.
	pub after: String,
	/// Disable TypeScript semantic diagnostics during transformation.
	pub no_semantic_diagnostics: bool,
	/// Enable identifier and event-name obfuscation.
	pub obfuscation: bool,
	/// The identifier generation mode: `full`, `obfuscated`, `short` or `tiny`.
	pub id_generation_mode: String,
	/// An optional identifier hash prefix; values beginning with `$` are
	/// reserved.
	pub hash_prefix: String,
	/// The salt used for hashes.
	pub salt: String,
	/// Generate identifiers for exports.
	pub preload_ids: bool,
	/// Reuse the original parse tree for sources Flamework leaves unchanged.
	pub skip_unchanged_files: bool,
	/// Tuning knobs.
	pub optimizations: FlameworkOptimizations,
}

/// Flamework tuning knobs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlameworkOptimizations {
	/// The maximum guard-generation deduplication work; `None` when unset.
	pub guard_generation_dedup_limit: Option<i64>,
}

/// A structural problem [`Config::validate`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError(String);

impl fmt::Display for ConfigError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.0)
	}
}

impl std::error::Error for ConfigError {}

impl Config {
	/// Fills in the `idGenerationMode` default for Flamework settings that
	/// leave it unset.
	pub(super) fn apply_flamework_defaults(&mut self) {
		let Some(flamework) = &mut self.flamework else {
			return;
		};
		for profile in flamework.profiles.values_mut() {
			profile.id_generation_mode =
				default_flamework_id_generation_mode(&profile.id_generation_mode, profile.obfuscation);
		}
		if flamework.profiles.is_empty() {
			flamework.id_generation_mode =
				default_flamework_id_generation_mode(&flamework.id_generation_mode, flamework.obfuscation);
		}
	}

	/// Structural validation errors for the Flamework settings.
	#[must_use]
	pub fn validate_flamework(&self) -> Vec<ConfigError> {
		let Some(flamework) = &self.flamework else {
			return Vec::new();
		};
		let mut errs = Vec::new();
		if !flamework.profiles.is_empty() && has_global_flamework_options(flamework) {
			errs.push(ConfigError(
				"flamework: global options cannot be combined with flamework.profiles".to_owned(),
			));
		}
		match flamework.id_generation_mode.as_str() {
			"" | "full" | "obfuscated" | "short" | "tiny" => {}
			mode => errs.push(ConfigError(format!(
				"flamework.idGenerationMode must be one of full, obfuscated, short, tiny; got {}",
				quote(mode)
			))),
		}
		if flamework.hash_prefix.starts_with('$') {
			errs.push(ConfigError(format!(
				"flamework.hashPrefix must not start with reserved prefix \"$\": {}",
				quote(&flamework.hash_prefix)
			)));
		}
		if let Some(limit) = flamework.optimizations.guard_generation_dedup_limit
			&& limit < 0
		{
			errs.push(ConfigError(format!(
				"flamework.optimizations.guardGenerationDedupLimit must be >= 0, got {limit}"
			)));
		}
		let mut normalized_names: BTreeMap<String, &str> = BTreeMap::new();
		for (name, profile) in &flamework.profiles {
			let normalized = Flavor::Unix.clean(&name.replace('\\', "/"));
			if normalized == "." || normalized.starts_with('/') || normalized == ".." || normalized.starts_with("../") {
				errs.push(ConfigError(format!(
					"flamework.profiles.{name} must be a project-relative tsconfig path"
				)));
			} else if let Some(previous) = normalized_names.get(&normalized) {
				errs.push(ConfigError(format!(
					"flamework profiles {} and {} resolve to the same tsconfig",
					quote(previous),
					quote(name)
				)));
			} else {
				normalized_names.insert(normalized, name);
			}
			errs.extend(validate_flamework_profile(name, profile));
		}
		errs
	}
}

fn default_flamework_id_generation_mode(mode: &str, obfuscation: bool) -> String {
	if !mode.is_empty() {
		return mode.to_owned();
	}
	if obfuscation { "obfuscated" } else { "full" }.to_owned()
}

const fn has_global_flamework_options(flamework: &FlameworkConfig) -> bool {
	!flamework.after.is_empty()
		|| flamework.no_semantic_diagnostics
		|| flamework.obfuscation
		|| !flamework.id_generation_mode.is_empty()
		|| !flamework.hash_prefix.is_empty()
		|| !flamework.salt.is_empty()
		|| flamework.preload_ids
		|| flamework.skip_unchanged_files
		|| flamework.optimizations.guard_generation_dedup_limit.is_some()
}

fn validate_flamework_profile(name: &str, profile: &FlameworkProfile) -> Vec<ConfigError> {
	let cfg = Config {
		flamework: Some(FlameworkConfig {
			after: profile.after.clone(),
			no_semantic_diagnostics: profile.no_semantic_diagnostics,
			obfuscation: profile.obfuscation,
			id_generation_mode: profile.id_generation_mode.clone(),
			hash_prefix: profile.hash_prefix.clone(),
			salt: profile.salt.clone(),
			preload_ids: profile.preload_ids,
			skip_unchanged_files: profile.skip_unchanged_files,
			optimizations: profile.optimizations.clone(),
			profiles: BTreeMap::new(),
		}),
		..Config::default()
	};
	cfg.validate_flamework()
		.into_iter()
		.map(|err| ConfigError(format!("flamework.profiles.{name}: {err}")))
		.collect()
}

#[cfg(test)]
mod tests;
