//! `flamework_config.go`: whether native Flamework is on for this project, and with which
//! profile. Ported in Stage 5a because every compile asks it; the Flamework transform itself is
//! Stage 6.

use rotor_config::config::{FlameworkConfig, LoadError};
use rotor_support::gopath;
use rotor_tsrs::tsoptions::ParsedCommandLine;

use crate::compile::{Error, Failure};
use crate::incremental_manifest::normalize_source_file_path;
use crate::sidecar::config_file_transformer_plugins;

/// `legacyFlameworkTransformer`
pub(crate) const LEGACY_FLAMEWORK_TRANSFORMER: &str = "rbxts-transformer-flamework";

/// `prepareFlameworkConfig`: the active `[flamework]` configuration, `None` when Flamework is
/// off.
pub(crate) fn prepare_flamework_config(
	dir: &str,
	parsed: &ParsedCommandLine,
) -> Result<Option<FlameworkConfig>, Failure> {
	let mut flamework = None;
	match rotor_config::config::load(dir) {
		Ok(config) => {
			let errors = config.validate_flamework();
			if !errors.is_empty() {
				let messages = errors.iter().map(|error| format!("config: {error}")).collect();
				return Err(Failure::strings(
					messages,
					Error::message("compile: invalid Flamework configuration"),
				));
			}
			flamework = select_flamework_profile(dir, Some(parsed), config.flamework)
				.map_err(|error| Failure::strings(vec![error.to_string()], error))?;
		}
		Err(LoadError::NotFound) => {}
		Err(error) => {
			let message = error.to_string();
			return Err(Failure::strings(vec![message.clone()], Error::message(message)));
		}
	}

	if project_uses_legacy_flamework_transformer(Some(parsed)) && flamework.is_some() {
		let message = "[flamework] cannot be combined with the legacy rbxts-transformer-flamework plugin; remove it from tsconfig.json";
		return Err(Failure::strings(vec![message.to_owned()], Error::message(message)));
	}
	Ok(flamework)
}

/// `selectFlameworkProfile`: with profiles, the one named by the active tsconfig's path relative
/// to the project, and `None` when no profile names it.
pub(crate) fn select_flamework_profile(
	dir: &str,
	parsed: Option<&ParsedCommandLine>,
	flamework: Option<FlameworkConfig>,
) -> Result<Option<FlameworkConfig>, Error> {
	let Some(flamework) = flamework else {
		return Ok(None);
	};
	if flamework.profiles.is_empty() {
		return Ok(Some(flamework));
	}
	let Some(config_name) = parsed
		.map(ParsedCommandLine::config_name)
		.filter(|name| !name.is_empty())
	else {
		return Err(Error::message("config: flamework profiles require an active tsconfig"));
	};
	let mut config_path = gopath::from_slash(config_name).into_owned();
	if !gopath::is_abs(&config_path) {
		config_path = gopath::join(&[dir, &config_path]);
	}
	let relative = gopath::rel(dir, &config_path)
		.map_err(|error| Error::message(format!("config: resolve active tsconfig profile: {error}")))?;
	let active = normalize_flamework_profile_path(&relative);
	for (name, profile) in &flamework.profiles {
		if normalize_flamework_profile_path(name) == active {
			return Ok(Some(FlameworkConfig {
				after: profile.after.clone(),
				no_semantic_diagnostics: profile.no_semantic_diagnostics,
				obfuscation: profile.obfuscation,
				id_generation_mode: profile.id_generation_mode.clone(),
				hash_prefix: profile.hash_prefix.clone(),
				salt: profile.salt.clone(),
				preload_ids: profile.preload_ids,
				skip_unchanged_files: profile.skip_unchanged_files,
				optimizations: profile.optimizations.clone(),
				..FlameworkConfig::default()
			}));
		}
	}
	Ok(None)
}

/// `normalizeFlameworkProfilePath`
fn normalize_flamework_profile_path(path: &str) -> String {
	gopath::to_slash(&gopath::clean(&gopath::from_slash(&path.replace('\\', "/")))).into_owned()
}

/// `projectUsesLegacyFlameworkTransformer`
pub(crate) fn project_uses_legacy_flamework_transformer(parsed: Option<&ParsedCommandLine>) -> bool {
	let Some(parsed) = parsed else {
		return false;
	};
	let config_name = parsed.config_name();
	if !config_name.is_empty()
		&& let Some(plugins) = config_file_transformer_plugins(&normalize_source_file_path(config_name))
	{
		return plugins.iter().any(|plugin| plugin == LEGACY_FLAMEWORK_TRANSFORMER);
	}
	parsed.extended_source_files().iter().any(|config_path| {
		config_file_transformer_plugins(&normalize_source_file_path(config_path))
			.is_some_and(|plugins| plugins.iter().any(|plugin| plugin == LEGACY_FLAMEWORK_TRANSFORMER))
	})
}
