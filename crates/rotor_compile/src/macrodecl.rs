//! `macrodecl.go`: the synthetic `$nameof` / `$keys` / `$file` ambient declaration. As with
//! `$env`, `rotor_tsrs::project` serves it and applies the coexistence guard.

use rotor_tsrs::tspath;

pub use rotor_tsrs::project::{MACRO_DECL_BODY, MACRO_DECL_FILE_NAME};

/// `MacroDeclFileName`: the legacy per-macro editor companion, still recognised.
pub const MACRO_DECL_COMPANION_FILE_NAME: &str = rotor_tsrs::project::MACRO_DECL_COMPANION;

/// `macroDeclSubstrings`: the substrings that mark a source as using one of the macros.
const MACRO_DECL_SUBSTRINGS: [&str; 3] = ["$nameof", "$keys", "$file"];

/// `SourceUsesMacros`
#[must_use]
pub fn source_uses_macros(text: &str) -> bool {
	MACRO_DECL_SUBSTRINGS.iter().any(|substring| text.contains(substring))
}

/// `macroDeclPath`: the synthetic declaration next to the tsconfig (slash separated).
#[must_use]
pub fn macro_decl_path(config_path: &str) -> String {
	format!("{}/{MACRO_DECL_FILE_NAME}", tspath::get_directory_path(config_path))
}
