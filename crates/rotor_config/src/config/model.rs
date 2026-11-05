//! The `rotor.toml` model.

use super::flamework::FlameworkConfig;
use super::parallel_transformers::ParallelTransformersConfig;

/// Native compiler settings loaded from `rotor.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
	/// The experimental parallel-transformers attestation.
	pub parallel_transformers: Option<ParallelTransformersConfig>,
	/// The native Flamework transformer's settings.
	pub flamework: Option<FlameworkConfig>,

	/// The `rotor.toml` half of the `//!native` type-annotation opt-in: when
	/// true, a file whose leading comments include `//!native` emits Luau
	/// parameter and return annotations.
	///
	/// It is an `Option` so an unset value is distinguishable from an explicit
	/// false: `tsconfig.json`'s `rbxts.nativeTypes` wins when it is set, and
	/// this value is the fallback only. Both unset means off.
	pub native_types: Option<bool>,

	/// The `rotor.toml` half of the experimental `luauTypes` opt-in: when
	/// true, compiled output carries full Luau type annotations (parameters,
	/// returns, locals, class fields, exported type aliases) so Luau code can
	/// consume it with real types.
	///
	/// It is an `Option` so an unset value is distinguishable from an explicit
	/// false: `--luauTypes` on the command line and `tsconfig.json`'s
	/// `rbxts.luauTypes` win when they are set, and this value is the fallback
	/// only. Unset everywhere means off. The emitted output may change between
	/// releases.
	pub luau_types: Option<bool>,

	/// Non-fatal problems found while loading, such as unknown keys.
	pub warnings: Vec<String>,
}
