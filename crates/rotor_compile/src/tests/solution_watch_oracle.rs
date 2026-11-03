//! `solution_watch_oracle_test.go`: incremental watch metadata matches a fresh derivation.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::large_stack;
use super::project_reference_import::{ReferenceFixture, new_reference_fixture};
use crate::crossproject::{new_cross_project_metadata, project_restored_declarations};
use crate::solution::{SolutionCoordinator, SolutionProject, SolutionProjectSelection, build_solution_graph};
use crate::solution_drain::Drainer;

/// `oracleStep`: changes the fixture and names the projects a watcher would invalidate for that
/// change. `reload` runs `ReloadForWatch` first, as the watcher does for a config event.
struct OracleStep {
	name: &'static str,
	change: Box<dyn Fn(&mut ReferenceFixture)>,
	invalidate: Vec<&'static str>,
	reload: bool,
	/// A step after a change no project was invalidated for; its metadata may lag the disk, so
	/// only the drain error is checked.
	missed: bool,
	/// A step whose drain must fail.
	fails: bool,
}

/// `failingStep`
fn failing_step(mut step: OracleStep) -> OracleStep {
	step.fails = true;
	step
}

/// `missedStep`
fn missed_step(mut step: OracleStep) -> OracleStep {
	step.missed = true;
	step
}

fn step(
	name: &'static str,
	change: impl Fn(&mut ReferenceFixture) + 'static,
	invalidate: &[&'static str],
) -> OracleStep {
	OracleStep {
		name,
		change: Box::new(change),
		invalidate: invalidate.to_vec(),
		reload: false,
		missed: false,
		fails: false,
	}
}

/// `includeStep`
fn include_step(name: &'static str, project: &'static str, include: &'static str) -> OracleStep {
	step(
		name,
		move |f| {
			let path = format!("{project}/tsconfig.json");
			let mut config = f.read_json(&path);
			config.insert("include".to_owned(), json!([include]));
			f.json(&path, &Value::Object(config));
		},
		&[project],
	)
}

/// `writeStep`
fn write_step(name: &'static str, path: &'static str, text: &'static str, invalidate: &[&'static str]) -> OracleStep {
	step(name, move |f| f.write(path, text), invalidate)
}

/// `removeStep`
fn remove_step(name: &'static str, path: &'static str, invalidate: &[&'static str]) -> OracleStep {
	step(
		name,
		move |f| std::fs::remove_file(f.path(path)).expect("remove"),
		invalidate,
	)
}

/// `renameStep`
fn rename_step(name: &'static str, from: &'static str, to: &'static str, invalidate: &[&'static str]) -> OracleStep {
	step(
		name,
		move |f| std::fs::rename(f.path(from), f.path(to)).expect("rename"),
		invalidate,
	)
}

/// `optionStep`
fn option_step(
	name: &'static str,
	project: &'static str,
	key: &'static str,
	value: Value,
	reload: bool,
	invalidate: &[&'static str],
) -> OracleStep {
	let mut step = step(
		name,
		move |f| f.set_compiler_option(project, key, value.clone()),
		invalidate,
	);
	step.reload = reload;
	step
}

/// `newOracleFixture`
fn new_oracle_fixture() -> ReferenceFixture {
	let mut f = new_reference_fixture();
	f.project("core", &[], &json!({}));
	f.project("left", &["core"], &json!({"declarationDir": "types"}));
	f.project("mid", &["left"], &json!({}));
	f.project("right", &["core"], &json!({}));
	f.project("game", &["mid", "right"], &json!({}));
	f
}

/// The parts of a project its incremental salt and scheduling read; Go compares the whole value.
fn project_fingerprint(project: &SolutionProject) -> String {
	let sorted = |paths: Option<&std::sync::Arc<HashMap<String, String>>>| {
		let mut entries: Vec<(String, String)> = paths
			.map(|paths| paths.iter().map(|(key, value)| (key.clone(), value.clone())).collect())
			.unwrap_or_default();
		entries.sort();
		entries
	};
	let options = &project.options;
	format!(
		"{} {:?} {} {:?} {:?} {:?} lua={} include={} rojo={} type={:?} emit_include={}",
		project.config_path,
		project.references,
		project.coordinator,
		project.direct_references,
		sorted(options.project_reference_paths.as_ref()),
		sorted(options.project_reference_salt_paths.as_ref()),
		options.lua_extension,
		options.include_path,
		options.rojo_config_path,
		options.project_type,
		options.emit_include_files
	)
}

/// `assertMetadataMatchesFresh`: compares the coordinator's incremental metadata with metadata
/// derived from scratch for the files on disk now.
fn assert_metadata_matches_fresh(
	f: &ReferenceFixture,
	coordinator: &SolutionCoordinator,
	step: &str,
	drain_failed: bool,
) {
	let mut graph = build_solution_graph(&f.path("game/tsconfig.json"), &f.options())
		.unwrap_or_else(|error| panic!("{step}: fresh graph: {error}"));
	let (fresh, _) = new_cross_project_metadata(&mut graph);
	if let Some(error) = &fresh.validation_err {
		assert!(drain_failed, "{step}: drain passed, fresh metadata fails: {error}");
		return;
	}
	let Drainer::Build(drainer) = &coordinator.drainer else {
		panic!("{step}: the coordinator does not build");
	};
	assert_eq!(coordinator.write_roots, fresh.write_roots, "{step}: writeRoots");
	assert_eq!(
		coordinator.wait_only_dependencies, fresh.wait_only_dependencies,
		"{step}: waitOnlyDependencies"
	);
	assert_eq!(
		coordinator.reference_outputs, fresh.reference_outputs,
		"{step}: referenceOutputs"
	);
	assert_eq!(
		drainer.restored_metadata_errs, fresh.restored_metadata_errs,
		"{step}: restoredMetadataErrs"
	);
	// Project options feed each project's incremental salt, including projectReferencePaths.
	let got: Vec<String> = coordinator.graph.projects.iter().map(project_fingerprint).collect();
	let want: Vec<String> = graph.projects.iter().map(project_fingerprint).collect();
	assert_eq!(got, want, "{step}: projects");
	for project in &graph.projects {
		let state = &coordinator.states[&project.config_path];
		assert_eq!(
			project_fingerprint(&state.project),
			project_fingerprint(project),
			"{step}: state project {}",
			project.config_path
		);
	}
	for (config_path, declarations) in &drainer.restored_declarations {
		let Some(state) = coordinator.project_state(config_path) else {
			continue;
		};
		if let Ok(want) = project_restored_declarations(&state.project) {
			assert_eq!(*declarations, want, "{step}: restored declarations of {config_path}");
		}
	}
}

/// `runOracleSteps`
fn run_oracle_steps(steps: &[OracleStep]) {
	let mut f = new_oracle_fixture();
	let mut coordinator = f.coordinator("game");
	let root_config = f.path("game/tsconfig.json");
	for step in steps {
		(step.change)(&mut f);
		if step.reload
			&& let Err(error) = coordinator.reload_for_watch(&root_config, &f.options())
		{
			panic!("{}: reload: {error}", step.name);
		}
		for project in &step.invalidate {
			coordinator.invalidate(&[f.path(&format!("{project}/tsconfig.json"))]);
		}
		let (_, messages, error) = coordinator.drain();
		assert_eq!(
			step.fails,
			error.is_some(),
			"{}: drain error = {error:?} ({messages:?}), want failure {}",
			step.name,
			step.fails
		);
		if step.missed {
			continue;
		}
		large_stack(|| assert_metadata_matches_fresh(&f, &coordinator, step.name, error.is_some()));
	}
}

#[test]
#[expect(clippy::too_many_lines, reason = "one Go test's table")]
fn watch_metadata_matches_fresh_derivation() {
	let sequences: Vec<(&str, Vec<OracleStep>)> = vec![
		(
			"sources",
			vec![
				write_step(
					"core text edit",
					"core/src/index.ts",
					"export const value = 2;\n",
					&["core"],
				),
				write_step("left add", "left/src/added.ts", "export const added = 1;\n", &["left"]),
				write_step("core add", "core/src/extra.ts", "export const extra = 1;\n", &["core"]),
				rename_step("left rename", "left/src/added.ts", "left/src/renamed.ts", &["left"]),
				remove_step("core remove", "core/src/extra.ts", &["core"]),
				write_step("game edit", "game/src/index.ts", "export const value = 3;\n", &["game"]),
				write_step(
					"right nested add",
					"right/src/nested/deep.ts",
					"export const deep = 1;\n",
					&["right"],
				),
				rename_step("right folder rename", "right/src/nested", "right/src/moved", &["right"]),
			],
		),
		(
			"configs",
			vec![
				option_step(
					"left declarationDir",
					"left",
					"declarationDir",
					json!("decl"),
					false,
					&["left"],
				),
				option_step("core outDir", "core", "outDir", json!("build"), false, &["core"]),
				option_step(
					"mid declarationDir with reload",
					"mid",
					"declarationDir",
					json!("types"),
					true,
					&["mid"],
				),
				option_step(
					"core outDir back with reload",
					"core",
					"outDir",
					json!("out"),
					true,
					&[],
				),
				write_step(
					"left add after configs",
					"left/src/added.ts",
					"export const added = 1;\n",
					&["left"],
				),
			],
		),
		(
			"upstream not invalidated",
			vec![
				// A change the watcher missed must not break a consumer-only cycle; the next cycle
				// that invalidates its owner picks it up.
				missed_step(write_step(
					"left add, not invalidated",
					"left/src/added.ts",
					"export const added = 1;\n",
					&[],
				)),
				missed_step(write_step(
					"game edit",
					"game/src/index.ts",
					"export const value = 3;\n",
					&["game"],
				)),
				write_step("left edit", "left/src/index.ts", "export const value = 2;\n", &["left"]),
			],
		),
		(
			"config error recovers",
			vec![
				include_step("right no inputs", "right", "missing"),
				include_step("right fixed", "right", "src"),
				write_step(
					"right edit",
					"right/src/index.ts",
					"export const value = 4;\n",
					&["right"],
				),
			],
		),
		(
			"project removed",
			vec![
				{
					let mut drop_right = step(
						"drop right",
						|f| {
							f.project("game", &["mid"], &json!({}));
							std::fs::remove_file(f.path("right/tsconfig.json")).expect("remove");
						},
						&["game"],
					);
					drop_right.reload = true;
					drop_right
				},
				write_step("game edit", "game/src/index.ts", "export const value = 6;\n", &["game"]),
			],
		),
		(
			"validation error recovers",
			vec![
				failing_step(option_step(
					"left not composite",
					"left",
					"composite",
					json!(false),
					false,
					&["left"],
				)),
				failing_step(write_step(
					"game edit while invalid",
					"game/src/index.ts",
					"export const value = 5;\n",
					&["game"],
				)),
				option_step("left composite", "left", "composite", json!(true), false, &["left"]),
				write_step("left edit", "left/src/index.ts", "export const value = 2;\n", &["left"]),
			],
		),
	];
	for (_, steps) in &sequences {
		run_oracle_steps(steps);
	}
}

#[test]
fn watch_import_only_source_refreshes_satisfied_declarations() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({}));
	f.project("game", &["shared"], &json!({"declaration": true}));
	// game is not composite, so a file only its imports reach is legal.
	(include_step("", "game", "src/index.ts").change)(&mut f);
	let mut coordinator = f.coordinator("game");
	let selection = SolutionProjectSelection {
		selected: vec![f.path("shared/tsconfig.json")],
		satisfied: vec![f.path("game/tsconfig.json")],
	};
	let output = coordinator.drain_with_project_results_for_selection(Some(&selection));
	assert!(
		output.error.is_none(),
		"selection drain: {:?} ({:?})",
		output.error,
		output.messages
	);
	// The import adds a declaration output but leaves the config's file names, so the metadata
	// stays current.
	f.write("game/src/extra.ts", "export const extra = 1;\n");
	f.write(
		"game/src/index.ts",
		r#"export { extra } from "./extra"; export const value = 1;"#,
	);
	f.invalidate_and_drain(&mut coordinator, &["game"]);
	let output = coordinator.drain_with_project_results_for_selection(Some(&selection));
	assert!(
		output.error.is_none(),
		"selection drain: {:?} ({:?})",
		output.error,
		output.messages
	);
	large_stack(|| assert_metadata_matches_fresh(&f, &coordinator, "import-only source", false));
	let Drainer::Build(drainer) = &coordinator.drainer else {
		panic!("the coordinator does not build");
	};
	let restored = drainer.restored_declarations.get(&f.path("game/tsconfig.json"));
	assert_eq!(
		restored.map_or(0, Vec::len),
		2,
		"restored declarations = {:?}, want index and extra",
		drainer.restored_declarations
	);
}
