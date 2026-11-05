//! `transform_macro_adversarial_test.go`

use std::rc::Rc;

use rotor_config::config::FlameworkConfig;

use super::{StateSpec, collect_call_expressions, emit_node, temp_dir, with_macro_state, with_state};
use crate::ErrorKind;
use crate::project::{ProjectOptions, open_project};
use crate::transform_intrinsics::build_intrinsic_macro;
use crate::transform_macro_types::{MacroKind, MacroRuntime, UserMacro};
use crate::transform_macros::{build_generic_macro, transform_flamework_call};
use crate::transform_network::shuffled_indexes;
use crate::transform_paths::{build_path_glob_intrinsic, build_path_intrinsic};

/// The type of the fixture's first type alias.
fn first_alias_type(
	state: &mut crate::transform_state::TransformState<'_>,
	file: rotor_tsrs::P<rotor_tsrs::ast::SourceFile>,
	index: usize,
) -> rotor_tsrs::P<rotor_tsrs::checker::Type> {
	let alias = file.statements().nodes()[index];
	state
		.ck()
		.get_type_from_type_node(alias.type_node().expect("alias type"))
}

#[test]
fn build_path_intrinsics_registers_glob_and_emits_rojo_path_when_literal_paths_are_valid() {
	let spec = StateSpec::new(
		vec![
			("package.json", r#"{"name":"fixture-game","version":"1.0.0"}"#.to_owned()),
			(
				"tsconfig.json",
				r#"{"compilerOptions":{"rootDir":"src","outDir":"out","strict":true},"files":["src/main.ts","src/assets/entry.ts"]}"#
					.to_owned(),
			),
			(
				"default.project.json",
				r#"{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"}}}}"#.to_owned(),
			),
			("src/assets/entry.ts", "export {};\n".to_owned()),
			(
				"src/main.ts",
				"type Path = \"src/assets/entry.ts\";\ntype Glob = \"./assets/*.ts\";\n".to_owned(),
			),
		],
		"src/main.ts",
	);
	let (path_text, glob_text, globs_json, origin_globs) = with_state(&spec, |state, file| {
		let path_type = first_alias_type(state, file, 0);
		let glob_type = first_alias_type(state, file, 1);
		let path = build_path_intrinsic(state, file.as_node(), path_type).expect("buildPathIntrinsic");
		let glob = build_path_glob_intrinsic(state, file.as_node(), glob_type).expect("buildPathGlobIntrinsic");
		let (_, globs_json) = state.project.runtime_artifacts().expect("RuntimeArtifacts");
		let snapshot = state.project.build_info_snapshot();
		let origin_globs = snapshot
			.metadata
			.and_then(|metadata| metadata.globs)
			.and_then(|globs| globs.origins)
			.map(|origins| origins.get("src/main.ts").cloned().flatten());
		(
			emit_node(path, file),
			emit_node(glob, file),
			String::from_utf8(globs_json.unwrap_or_default()).expect("utf-8"),
			origin_globs,
		)
	});

	assert!(
		path_text.contains(r#""ReplicatedStorage""#) && path_text.contains(r#""entry""#),
		"path intrinsic Rojo output = {path_text:?}"
	);
	assert_eq!(
		glob_text, r#""src/assets/*.ts""#,
		"pathglob intrinsic output, want project-relative assets glob"
	);
	assert_eq!(
		globs_json,
		r#"{"game":{"src/assets/*.ts":[["ReplicatedStorage","TS","assets","entry"]]},"packages":{}}"#
	);
	let origin_globs = origin_globs.expect("glob registration");
	assert_eq!(origin_globs, Some(vec!["src/assets/*.ts".to_owned()]), "origin globs");
}

#[test]
fn build_path_intrinsics_return_macro_errors_when_path_is_not_literal() {
	let (path_error, glob_error) =
		with_macro_state(FlameworkConfig::default(), "type Dynamic = string;", |state, file| {
			let target = first_alias_type(state, file, 0);
			(
				build_path_intrinsic(state, file.as_node(), target).err(),
				build_path_glob_intrinsic(state, file.as_node(), target).err(),
			)
		});

	assert!(
		path_error
			.as_ref()
			.is_some_and(|error| error.is(ErrorKind::InvalidMacro))
			&& glob_error
				.as_ref()
				.is_some_and(|error| error.is(ErrorKind::InvalidMacro)),
		"path error = {path_error:?}, glob error = {glob_error:?}, want ErrInvalidMacro"
	);
}

#[test]
fn inline_macro_intrinsic_selects_linked_parameter_and_preserves_return_cast() {
	let source = [
		"/** @metadata {@link value intrinsic-inline} */",
		"declare function select(discarded: string, value: number): boolean;",
		r#"select("discarded", 42);"#,
	]
	.join("\n");
	let got = with_macro_state(FlameworkConfig::default(), &source, |state, file| {
		let calls = collect_call_expressions(file);
		assert_eq!(calls.len(), 1, "call count");
		let result =
			transform_flamework_call(state, calls[0], &MacroRuntime::default()).expect("transformFlameworkCall");
		emit_node(result.expression.expect("macro expression"), file)
	});

	assert_eq!(
		got, "42 as boolean",
		"inline expression, want selected argument cast to boolean"
	);
}

#[test]
fn shuffled_indexes_returns_macro_error_when_injected_random_index_is_outside_range() {
	let config = FlameworkConfig {
		obfuscation: true,
		..FlameworkConfig::default()
	};
	let error = with_macro_state(config, "export {};", |state, _| {
		let runtime = MacroRuntime {
			random_index: Some(Rc::new(|_| Ok(4))),
			..MacroRuntime::default()
		};
		shuffled_indexes(state, 4, &runtime).err()
	});

	assert!(
		error
			.as_ref()
			.is_some_and(|error| error.is(ErrorKind::InvalidMacro) && error.message().contains("outside [0, 4)")),
		"shuffledIndexes() error = {error:?}, want bounded ErrInvalidMacro"
	);
}

#[test]
fn guard_macros_return_guard_builder_absent_when_no_guard_runtime_is_configured() {
	let (generic_error, tuple_error) = with_macro_state(
		FlameworkConfig::default(),
		"type Input = [name: string, ...values: number[]];",
		|state, file| {
			let target = first_alias_type(state, file, 0);
			let generic = UserMacro {
				kind: MacroKind::Generic,
				target: Some(target),
				metadata: "guard".to_owned(),
				..UserMacro::default()
			};
			let tuple = UserMacro {
				kind: MacroKind::Intrinsic,
				intrinsic: "tuple-guards".to_owned(),
				inputs: vec![target],
				..UserMacro::default()
			};
			(
				build_generic_macro(state, file.as_node(), &generic, &MacroRuntime::default()).err(),
				build_intrinsic_macro(state, file.as_node(), &tuple, &MacroRuntime::default()).err(),
			)
		},
	);

	assert!(
		generic_error
			.as_ref()
			.is_some_and(|error| error.is(ErrorKind::GuardBuilderAbsent))
			&& tuple_error
				.as_ref()
				.is_some_and(|error| error.is(ErrorKind::GuardBuilderAbsent)),
		"generic error = {generic_error:?}, tuple error = {tuple_error:?}, want ErrGuardBuilderAbsent"
	);
}

#[test]
fn project_persistence_preserves_obfuscation_hash_after_reopen() {
	let (_dir, directory) = temp_dir();
	std::fs::write(
		super::join(&[&directory, "package.json"]),
		r#"{"name":"fixture-game","version":"1.0.0"}"#,
	)
	.expect("write package.json");
	let options = ProjectOptions {
		project_dir: directory,
		root_dir: "src".to_owned(),
		out_dir: "out".to_owned(),
		config: FlameworkConfig {
			obfuscation: true,
			..FlameworkConfig::default()
		},
		..ProjectOptions::default()
	};
	let mut project = open_project(&options).expect("OpenProject");

	let hash = project.hash_string("submitted", "remotes").expect("HashString");
	project.persist().expect("Persist");
	let mut reopened = open_project(&options).expect("reopen OpenProject");
	let reopened_hash = reopened
		.hash_string("submitted", "remotes")
		.expect("reopened HashString");

	// Go's `^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$`.
	let groups: Vec<&str> = hash.split('-').collect();
	let hex = |text: &str| {
		text.bytes()
			.all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
	};
	assert!(
		groups.iter().map(|group| group.len()).eq([8, 4, 4, 4, 12])
			&& groups.iter().all(|group| hex(group))
			&& groups[2].starts_with('4')
			&& groups[3].starts_with(['8', '9', 'a', 'b']),
		"obfuscation hash = {hash:?}, want UUID v4"
	);
	let stored = reopened
		.build_info_snapshot()
		.string_hashes
		.and_then(|hashes| hashes.get("remotes:submitted").cloned());
	assert!(
		reopened_hash == hash && stored.as_deref() == Some(hash.as_str()),
		"reopened obfuscation hashes = {reopened_hash:?}, {stored:?}, want {hash:?}"
	);
}
