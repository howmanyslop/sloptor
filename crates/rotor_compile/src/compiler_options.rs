//! `compiler_options.go`.
//!
//! The compiler option overrides rotor applies after parsing a config. The program loader
//! (`rotor_tsrs::project`) applies them; these are the same rules for callers that hold parsed
//! options.

use rotor_tsrs::{CompilerOptions, Tristate};

/// `ApplyAutomaticTypes`: restores TypeScript 5's automatic typeRoots inclusion with the `"*"`
/// marker when the resolved config has no `types` list.
pub fn apply_automatic_types(options: &mut CompilerOptions) {
	if options.types.is_none() {
		options.types = Some(vec!["*".to_owned()]);
	}
}

/// `ApplyCheckerOverride`
pub const fn apply_checker_override(options: &mut CompilerOptions, checkers: Option<i64>) {
	if let Some(checkers) = checkers {
		options.checkers = Some(checkers);
	}
}

/// `ApplySingleThreadedOverride`
pub const fn apply_single_threaded_override(options: &mut CompilerOptions, single_threaded: Option<bool>) {
	if let Some(single_threaded) = single_threaded {
		options.single_threaded = if single_threaded {
			Tristate::True
		} else {
			Tristate::False
		};
	}
}
