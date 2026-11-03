//! `transform_addpaths_index_test.go`
//!
//! `BenchmarkAddPathsModules` is a benchmark without assertions beyond its fixture check, which
//! `add_paths_modules_matches_reference_for_every_caller_and_pattern` covers; it is not ported.

use std::collections::HashSet;
use std::sync::Arc;

use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::tspath::{ComparePathsOptions, get_directory_path};

use super::{join, new_transform_program, temp_dir, write_transform_fixture};
use crate::path::project_relative_path;
use crate::project::{ProjectOptions, open_project};
use crate::transform_addpaths::{
	add_paths_modules, is_script_rbx_type, match_add_paths_glob_segments_or_ancestor, match_glob_or_ancestor,
	module_specifier,
};
use crate::transform_state::{TransformState, new_transform_state};

/// `referenceAddPathsModules`: the c5428f9 addPathsModules loop. It computes every source path
/// again on each call and is the oracle for the index.
fn reference_add_paths_modules(
	state: &TransformState<'_>,
	file: P<SourceFile>,
	pattern: &str,
) -> Result<(Vec<String>, HashSet<String>), &'static str> {
	let root = &state.project.root_directory;
	let compare = ComparePathsOptions {
		use_case_sensitive_file_names: state.program.use_case_sensitive_file_names(),
		..ComparePathsOptions::default()
	};
	let from_directory = get_directory_path(file.file_name());
	let mut matches: Vec<(String, String)> = Vec::new();
	let mut sources = HashSet::new();
	for &source in state.program.get_source_files() {
		if source.is_declaration_file() || state.program.is_source_file_from_external_library(source) {
			continue;
		}
		let Ok(relative) = project_relative_path(root, source.file_name()) else {
			continue;
		};
		sources.insert(relative.clone());
		if source == file {
			continue;
		}
		if !match_glob_or_ancestor(pattern, &relative)? {
			continue;
		}
		let output = state.project.translator.get_output_path(source.file_name());
		if state.project.resolver.get_rbx_path_from_file_path(&output).is_none() {
			continue;
		}
		if is_script_rbx_type(state.project.resolver.get_rbx_type_from_file_path(&output)) {
			continue;
		}
		matches.push((
			relative,
			module_specifier(&from_directory, source.file_name(), &compare),
		));
	}
	matches.sort_by(|left, right| left.0.cmp(&right.0));
	Ok((matches.into_iter().map(|(_, module)| module).collect(), sources))
}

/// `newAddPathsIndexState`
fn with_index_state<R: Send>(directory: &str, run: impl FnOnce(&mut TransformState<'_>) -> R + Send) -> R {
	rotor_tsrs::with_large_stack(|| {
		let program = new_transform_program(directory);
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		let mut project = open_project(&ProjectOptions {
			project_dir: directory.to_owned(),
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			..ProjectOptions::default()
		})
		.expect("OpenProject");
		let mut state = new_transform_state(program, &mut checker, &mut project, None, &[]);
		run(&mut state)
	})
}

/// `addPathsIndexSource`
fn add_paths_index_source(state: &TransformState<'_>, directory: &str, name: &str) -> P<SourceFile> {
	let path = rotor_support::gopath::to_slash(&join(&[directory, name])).into_owned();
	state
		.program
		.get_source_file(&path)
		.unwrap_or_else(|| panic!("source {name} not found"))
}

/// `writeAddPathsIndexFixture`: mixed case, a dot directory, an external library source and a
/// source outside the project root added to the addPaths fixture. The project directory.
fn write_add_paths_index_fixture(base: &str) -> String {
	let directory = join(&[base, "game"]);
	super::transform_addpaths::write_add_paths_fixture(&directory, "import \"ext-lib\";\nexport {};\n");
	write_transform_fixture(
		&directory,
		"tsconfig.json",
		r#"{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"include":["src/**/*.ts","../outside/**/*.ts"]}"#,
	);
	write_transform_fixture(&directory, "src/server/services/Upper/Mixed.ts", "export {};\n");
	write_transform_fixture(&directory, "src/server/services/Ärea/Ünicode.ts", "export {};\n");
	write_transform_fixture(&directory, "src/server/services/.hidden/skip.ts", "export {};\n");
	write_transform_fixture(&directory, "node_modules/ext-lib/package.json", r#"{"name":"ext-lib"}"#);
	write_transform_fixture(&directory, "node_modules/ext-lib/index.ts", "export const value = 1;\n");
	write_transform_fixture(base, "outside/escape.ts", "export {};\n");
	directory
}

/// `assertAddPathsModulesMatchReference`
fn assert_matches_reference(state: &mut TransformState<'_>, caller: &str, file: P<SourceFile>, pattern: &str) {
	let got = add_paths_modules(state, file, pattern).map_err(|error| error.message().to_owned());
	let want = reference_add_paths_modules(state, file, pattern).map_err(str::to_owned);
	match (got, want) {
		(Err(got), Err(want)) => assert_eq!(got, want, "addPathsModules({caller}, {pattern:?}) error"),
		(Ok((got, got_sources)), Ok((want, want_sources))) => {
			assert_eq!(got, want, "addPathsModules({caller}, {pattern:?})");
			assert_eq!(
				got_sources.sources, want_sources,
				"addPathsModules({caller}, {pattern:?}) sources"
			);
		}
		(got, want) => panic!(
			"addPathsModules({caller}, {pattern:?}) = {:?}, want {:?}",
			got.map(|(modules, _)| modules),
			want.map(|(modules, _)| modules)
		),
	}
}

#[test]
fn add_paths_modules_matches_reference_for_every_caller_and_pattern() {
	// One state, and callers inside and outside the matched directories.
	let (_dir, base) = temp_dir();
	let directory = write_add_paths_index_fixture(&base);
	with_index_state(&directory, |state| {
		// Package resolution realpaths node_modules imports; look up the same canonical filename
		// in the program.
		let external_path =
			std::fs::canonicalize(join(&[&directory, "node_modules/ext-lib/index.ts"])).expect("realpath");
		let external = state
			.program
			.get_source_file(&rotor_support::gopath::to_slash(&external_path.to_string_lossy()));
		let escape_path = rotor_support::gopath::to_slash(&join(&[&base, "outside/escape.ts"])).into_owned();
		let escape = state.program.get_source_file(&escape_path);
		assert!(
			external.is_some_and(|external| state.program.is_source_file_from_external_library(external))
				&& escape.is_some(),
			"fixture lacks an external source ({}) or an escaped source ({})",
			external.is_some(),
			escape.is_some()
		);
		let callers = [
			"src/server/main.server.ts",
			"src/server/services/a.ts",
			"src/shared/components/x.ts",
			"src/server/services/Upper/Mixed.ts",
		];
		let patterns = [
			"src/server/services/**",
			"src/shared/components/*.ts",
			"src/server/services/upper/*",
			"src/server/services/Upper/*",
			"src/**",
			"src/missing/**",
			"src/[",
			"src/server/services",
			"src/server/services/.hidden/*",
			"src/server/services/*/*.ts",
			"src/missing/[",
			"src/server/services/ärea/ünicode.ts",
		];
		// Every call gives the result of the old loop, in two rounds so that calls after an error
		// also use the built index.
		for _ in 0..2 {
			for caller in callers {
				let file = add_paths_index_source(state, &directory, caller);
				for pattern in patterns {
					assert_matches_reference(state, caller, file, pattern);
				}
			}
		}
		let index = state.add_paths_sources.as_ref().expect("source index");
		let sources = &index.sources;
		assert!(
			sources.contains("src/server/main.server.ts")
				&& !sources.contains("src/shared/components/types.d.ts")
				&& !sources.contains("node_modules/ext-lib/index.ts")
				&& sources.len() == index.entries.len(),
			"source index = {sources:?}"
		);
	});
}

#[test]
fn add_paths_glob_segments_or_ancestor_stops_before_root() {
	for relative in [".", ""] {
		let segments: Vec<&str> = relative.split('/').collect();
		assert_eq!(
			match_add_paths_glob_segments_or_ancestor(&["**"], &segments),
			Ok(false),
			"root {relative:?}"
		);
	}
	assert_eq!(
		match_add_paths_glob_segments_or_ancestor(&["**"], &[]),
		Ok(false),
		"empty segments"
	);
}

#[test]
fn add_paths_modules_includes_caller_when_source_file_identity_changed() {
	// An updated caller SourceFile, as the nested visitor gets after a top-level expansion.
	let (_dir, base) = temp_dir();
	let directory = write_add_paths_index_fixture(&base);
	with_index_state(&directory, |state| {
		let file = add_paths_index_source(state, &directory, "src/server/services/a.ts");
		let updated = state
			.factory
			.update_source_file(
				file.as_node(),
				state.factory.new_node_list(file.statements().nodes().to_vec()),
				file.end_of_file_token,
			)
			.as_source_file_p();
		assert_ne!(updated, file, "UpdateSourceFile kept the SourceFile identity");
		let pattern = "src/server/services/*";

		let (original, _) = add_paths_modules(state, file, pattern).expect("original");
		let (changed, _) = add_paths_modules(state, updated, pattern).expect("updated");

		// The program SourceFile excludes itself; the updated one does not.
		assert!(
			!original.iter().any(|module| module == "./a") && changed.iter().any(|module| module == "./a"),
			"original = {original:?}, updated = {changed:?}"
		);
		assert_matches_reference(state, "updated a.ts", updated, pattern);
	});
}

#[test]
fn add_paths_modules_builds_source_index_once_per_state() {
	let (_dir, base) = temp_dir();
	let directory = write_add_paths_index_fixture(&base);
	with_index_state(&directory, |state| {
		assert!(
			state.add_paths_sources.is_none(),
			"source index exists before any addPaths call"
		);

		// Two callers and two patterns use one state.
		let main = add_paths_index_source(state, &directory, "src/server/main.server.ts");
		let (_, first) = add_paths_modules(state, main, "src/server/services/**").expect("first");
		let built = state.add_paths_sources.clone().expect("built index");
		let component = add_paths_index_source(state, &directory, "src/shared/components/x.ts");
		let (_, second) = add_paths_modules(state, component, "src/shared/**").expect("second");

		// The second call computes no source path and shares the set.
		assert!(
			state
				.add_paths_sources
				.as_ref()
				.is_some_and(|index| Arc::ptr_eq(index, &built))
				&& Arc::ptr_eq(&first, &second),
			"second addPaths call built the program source index again"
		);
	});
}

#[test]
fn add_paths_modules_uses_fresh_index_for_each_program() {
	// A state that built its index before a file is added and one is removed.
	let (_dir, base) = temp_dir();
	let directory = write_add_paths_index_fixture(&base);
	let pattern = "src/server/services/**";
	let modules_of = |directory: &str| {
		with_index_state(directory, |state| {
			let main = add_paths_index_source(state, directory, "src/server/main.server.ts");
			let (modules, sources) = add_paths_modules(state, main, pattern).expect("addPathsModules");
			(modules, sources)
		})
	};
	let (before_modules, before_sources) = modules_of(&directory);
	write_transform_fixture(&directory, "src/server/services/added.ts", "export {};\n");
	std::fs::remove_file(join(&[&directory, "src/server/services/b.ts"])).expect("remove b.ts");

	// A new program and state answer the same call.
	let (after_modules, after_sources) = modules_of(&directory);

	// Each state reflects only its own program.
	let has = |modules: &[String], module: &str| modules.iter().any(|candidate| candidate == module);
	assert!(
		has(&before_modules, "./services/b") && !has(&before_modules, "./services/added"),
		"before = {before_modules:?}"
	);
	assert!(
		!has(&after_modules, "./services/b")
			&& has(&after_modules, "./services/added")
			&& !after_sources.sources.contains("src/server/services/b.ts"),
		"after = {after_modules:?}, sources = {:?}",
		after_sources.sources
	);
	assert!(
		!Arc::ptr_eq(&before_sources, &after_sources)
			&& !before_sources.sources.contains("src/server/services/added.ts"),
		"a later Program changed or reused the earlier source index"
	);
}
