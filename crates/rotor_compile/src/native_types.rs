//! `native_types.go`: the effective `nativeTypes` opt-in.

use crate::rbxts_options::read_rbxts_options;

/// `effectiveNativeTypes`: tsconfig `rbxts.nativeTypes` (through `extends`), then rotor.toml's
/// `nativeTypes`, then `fallback`. A surface that does not declare the key is skipped, and
/// unreadable configuration counts as undeclared.
pub(crate) fn effective_native_types(dir: &str, ts_config_path: &str, fallback: bool) -> bool {
	if let Ok(Some(declared)) = read_rbxts_options(ts_config_path)
		&& let Some(value) = declared.native_types
	{
		return value;
	}
	if let Ok(config) = rotor_config::config::load(dir)
		&& let Some(value) = config.native_types
	{
		return value;
	}
	fallback
}
