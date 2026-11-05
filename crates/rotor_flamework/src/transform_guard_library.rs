//! `transform_guard_library.go`: the module guards import `t` from.

use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::diagnostics::Category;

use crate::project::FLAMEWORK_DIAGNOSTIC_CODE;
use crate::transform_decorators_imports::FLAMEWORK_PRELUDE_MODULE;
use crate::transform_diagnostics::{Diagnostic, text_pos};
use crate::transform_state::TransformState;
use crate::transform_symbols::path_within;

/// `flameworkGuardLibrary`: `@flamework/core`'s prelude when its `@rbxts/t` is the one this
/// package resolves, else `@rbxts/t` (with a warning when core or `t` is missing).
pub(crate) fn flamework_guard_library(state: &mut TransformState<'_>, file: P<SourceFile>) -> String {
	if !state.guard_library.is_empty() {
		return state.guard_library.clone();
	}
	let directory = gopath::dir(file.file_name());
	let Some(core_root) = resolve_node_module_root(&directory, "@flamework/core") else {
		state.add_diagnostic(flamework_warning_at_eof(
			file,
			"Flamework core was not found, guard generation may not work.",
		));
		return "@rbxts/t".to_owned();
	};
	let file_guard = resolve_node_module_root(&directory, "@rbxts/t");
	let core_guard = resolve_node_module_root(&core_root, "@rbxts/t");
	// A hoisted dependency can resolve without being importable through this package's
	// runtime node_modules scope.
	if let (Some(file_guard), Some(core_guard)) = (&file_guard, &core_guard)
		&& gopath::clean(file_guard) == gopath::clean(core_guard)
	{
		let local = rotor_support::fsutil::eval_symlinks(&gopath::join(&[
			&state.project.root_directory,
			"node_modules",
			"@rbxts",
			"t",
		]));
		state.guard_library = if local.is_ok_and(|local| gopath::clean(&local) == gopath::clean(file_guard)) {
			"@rbxts/t".to_owned()
		} else {
			FLAMEWORK_PRELUDE_MODULE.to_owned()
		};
		return state.guard_library.clone();
	}
	let expected_core_guard_root = gopath::join(&[&core_root, "node_modules", "@rbxts", "t"]);
	if !core_guard.is_some_and(|core_guard| path_within(&expected_core_guard_root, &core_guard)) {
		state.add_diagnostic(flamework_warning_at_eof(
			file,
			"Valid `@rbxts/t` was not found, guard generation may not work.",
		));
		return "@rbxts/t".to_owned();
	}
	FLAMEWORK_PRELUDE_MODULE.clone_into(&mut state.guard_library);
	state.guard_library.clone()
}

fn flamework_warning_at_eof(file: P<SourceFile>, message: &str) -> Diagnostic {
	let end = text_pos(file.as_node().end());
	Diagnostic::with_string_code(
		Some(file),
		end,
		end,
		FLAMEWORK_DIAGNOSTIC_CODE,
		Category::Warning,
		message,
	)
}

/// `resolveNodeModuleRoot`: the nearest `node_modules/<module>` with a `package.json`, with
/// symlinks resolved.
fn resolve_node_module_root(start: &str, module: &str) -> Option<String> {
	let mut current = gopath::clean(start);
	loop {
		let candidate = gopath::join(&[&current, "node_modules", &gopath::from_slash(module)]);
		if std::fs::metadata(gopath::join(&[&candidate, "package.json"])).is_ok_and(|info| info.is_file()) {
			return Some(rotor_support::fsutil::eval_symlinks(&candidate).unwrap_or(candidate));
		}
		let parent = gopath::dir(&current);
		if parent == current {
			return None;
		}
		current = parent;
	}
}
