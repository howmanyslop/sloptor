//! `luau_types.go`: the effective experimental `luauTypes` opt-in.

use crate::rbxts_options::{RbxtsOptions, read_rbxts_options};

/// `effectiveLuauTypes`: `--luauTypes` (the argv layer), then tsconfig `rbxts.luauTypes`, then
/// rotor.toml's `luauTypes`, then `fallback`. A surface that does not declare the key is skipped,
/// and unreadable configuration counts as undeclared.
pub(crate) fn effective_luau_types(
	dir: &str,
	ts_config_path: &str,
	argv: Option<&RbxtsOptions>,
	fallback: bool,
) -> bool {
	if let Some(value) = argv.and_then(|argv| argv.luau_types) {
		return value;
	}
	if let Ok(Some(declared)) = read_rbxts_options(ts_config_path)
		&& let Some(value) = declared.luau_types
	{
		return value;
	}
	if let Ok(config) = rotor_config::config::load(dir)
		&& let Some(value) = config.luau_types
	{
		return value;
	}
	fallback
}
