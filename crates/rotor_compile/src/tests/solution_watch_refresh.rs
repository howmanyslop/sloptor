//! `solution_watch_refresh_test.go`: watch cycles refresh only the metadata a change touches.

use serde_json::{Value, json};

use super::large_stack;
use super::project_reference_import::{ReferenceFixture, new_reference_fixture};
use super::solution::join;
use super::solution_drain::must_read_file;
use crate::project::{
	ProjectOptions, create_path_translator, get_root_dirs, new_project_program_with_options, resolve_rojo_config_path,
};
use crate::solution::{SolutionCoordinator, SolutionProject};
use crate::solution_watch::{filter_rojo_watch_directories, new_rojo_watch_state, solution_watch_artifact_directories};

impl ReferenceFixture {
	/// `coordinator`: a coordinator over `root`'s solution, drained once.
	pub fn coordinator(&self, root: &str) -> SolutionCoordinator {
		let mut coordinator =
			large_stack(|| SolutionCoordinator::new(&self.path(&format!("{root}/tsconfig.json")), &self.options()))
				.unwrap_or_else(|error| panic!("{error}"));
		Self::drain(&mut coordinator);
		coordinator
	}

	/// `drain`
	pub fn drain(coordinator: &mut SolutionCoordinator) {
		let (_, messages, error) = coordinator.drain();
		if let Some(error) = error {
			panic!("drain: {error} ({messages:?})");
		}
	}

	/// `invalidateAndDrain`
	pub fn invalidate_and_drain(&self, coordinator: &mut SolutionCoordinator, projects: &[&str]) {
		for project in projects {
			coordinator.invalidate(&[self.path(&format!("{project}/tsconfig.json"))]);
		}
		Self::drain(coordinator);
	}

	/// `setCompilerOption`
	pub fn set_compiler_option(&self, project: &str, key: &str, value: Value) {
		let path = format!("{project}/tsconfig.json");
		let mut config = self.read_json(&path);
		config["compilerOptions"][key] = value;
		self.json(&path, &Value::Object(config));
	}
}

/// `coordinatorMetadataWork`
fn metadata_work(coordinator: &SolutionCoordinator) -> (usize, usize) {
	(coordinator.metadata.config_parses, coordinator.metadata.refreshes)
}

/// `assertMetadataWork`
fn assert_metadata_work(coordinator: &SolutionCoordinator, before: (usize, usize), parses: usize, refreshes: usize) {
	let after = metadata_work(coordinator);
	assert_eq!(
		(after.0 - before.0, after.1 - before.1),
		(parses, refreshes),
		"metadata work (parses, refreshes)"
	);
}

/// `assertGameImport`
fn assert_game_import(f: &ReferenceFixture, want: &str) {
	let output = String::from_utf8(must_read_file(&f.path("game/out/init.luau"))).expect("utf8");
	assert!(output.contains(want), "game output lacks {want}:\n{output}");
}

/// `hasReferencePathSuffix`
fn has_reference_path_suffix(coordinator: &SolutionCoordinator, config_path: &str, suffix: &str) -> bool {
	coordinator
		.project_state(config_path)
		.and_then(|state| state.project.options.project_reference_paths)
		.is_some_and(|paths| {
			paths
				.keys()
				.any(|key| rotor_support::gopath::to_slash(key).to_lowercase().ends_with(suffix))
		})
}

#[test]
fn watch_text_edit_skips_metadata_refresh() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({"declarationDir": "types"}));
	f.project("game", &["shared"], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/types"; export const result = value;"#,
	);
	let mut coordinator = f.coordinator("game");
	coordinator.watch_sets();
	let before = metadata_work(&coordinator);
	f.write("shared/src/index.ts", "export const value = 42;\n");
	f.invalidate_and_drain(&mut coordinator, &["shared"]);
	coordinator.watch_sets();
	// The two invalidated configs are parsed once; nothing else is.
	assert_metadata_work(&coordinator, before, 2, 0);
	let output = String::from_utf8(must_read_file(&f.path("shared/out/init.luau"))).expect("utf8");
	assert!(output.contains("42"), "edit was not emitted:\n{output}");
}

#[test]
fn watch_reference_file_set_changes() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({"declarationDir": "types"}));
	f.project("other", &[], &json!({}));
	f.project("game", &["shared", "other"], &json!({}));
	f.write("shared/src/old.ts", "export const old = 1;\n");
	f.write(
		"game/src/index.ts",
		r#"import { old } from "../../shared/types/old"; export const result = old;"#,
	);
	let mut coordinator = f.coordinator("game");
	let game_config = f.path("game/tsconfig.json");

	let before = metadata_work(&coordinator);
	f.write("shared/src/added.ts", "export const added = 8;\n");
	f.write(
		"game/src/index.ts",
		r#"import { added } from "../../shared/types/added"; export const result = added;"#,
	);
	f.invalidate_and_drain(&mut coordinator, &["shared"]);
	assert_metadata_work(&coordinator, before, 2, 1);
	assert_game_import(&f, r#""shared", "added")"#);

	let before = metadata_work(&coordinator);
	std::fs::rename(f.path("shared/src/old.ts"), f.path("shared/src/renamed.ts")).expect("rename");
	f.write(
		"game/src/index.ts",
		r#"import { old } from "../../shared/types/renamed"; export const result = old;"#,
	);
	f.invalidate_and_drain(&mut coordinator, &["shared"]);
	assert_metadata_work(&coordinator, before, 2, 1);
	assert_game_import(&f, r#""shared", "renamed")"#);
	assert!(
		!has_reference_path_suffix(&coordinator, &game_config, "/src/old.ts")
			&& has_reference_path_suffix(&coordinator, &game_config, "/src/renamed.ts"),
		"rename did not move the reference path"
	);

	let before = metadata_work(&coordinator);
	std::fs::remove_file(f.path("shared/src/added.ts")).expect("remove");
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/types"; export const result = value;"#,
	);
	f.invalidate_and_drain(&mut coordinator, &["shared"]);
	assert_metadata_work(&coordinator, before, 2, 1);
	assert!(
		!has_reference_path_suffix(&coordinator, &game_config, "/src/added.ts")
			&& !has_reference_path_suffix(&coordinator, &game_config, "/types/added.d.ts"),
		"removed file kept its reference paths"
	);
}

#[test]
fn watch_transitive_added_source() {
	let mut f = new_reference_fixture();
	f.project("common", &[], &json!({"declarationDir": "types"}));
	f.project("left", &["common"], &json!({}));
	f.project("game", &["left"], &json!({}));
	let mut coordinator = f.coordinator("game");
	f.write("common/src/added.ts", "export const added = 3;\n");
	f.write(
		"game/src/index.ts",
		r#"import { added } from "../../common/types/added"; export const result = added;"#,
	);
	f.invalidate_and_drain(&mut coordinator, &["common"]);
	assert_game_import(&f, r#""common", "added")"#);
}

/// The shared → game fixture whose Rojo tree mounts `shared/next` as `moved`.
fn moved_fixture() -> ReferenceFixture {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({"declarationDir": "types"}));
	f.project("game", &["shared"], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/types"; export const result = value;"#,
	);
	f.rojo("default.project.json", "", &json!({"moved": {"$path": "shared/next"}}));
	f
}

#[test]
fn watch_config_edit_without_reload() {
	let f = moved_fixture();
	let mut coordinator = f.coordinator("game");
	let before = metadata_work(&coordinator);
	f.set_compiler_option("shared", "outDir", json!("next"));
	f.invalidate_and_drain(&mut coordinator, &["shared"]);
	assert_metadata_work(&coordinator, before, 2, 1);
	assert_game_import(&f, r#"game:GetService("ReplicatedStorage"), "moved")"#);
}

#[test]
fn watch_config_reload_refreshes_once() {
	let f = moved_fixture();
	let root_config = f.path("game/tsconfig.json");
	let mut coordinator = f.coordinator("game");
	let before = metadata_work(&coordinator);
	f.set_compiler_option("shared", "outDir", json!("next"));
	let clean_stale_outputs = coordinator
		.reload_for_watch(&root_config, &f.options())
		.unwrap_or_else(|error| panic!("{error}"));
	f.invalidate_and_drain(&mut coordinator, &["shared"]);
	clean_stale_outputs().unwrap_or_else(|error| panic!("{error}"));
	let refreshes = metadata_work(&coordinator).1 - before.1;
	assert_eq!(
		refreshes, 1,
		"config change refreshed metadata {refreshes} times, want 1"
	);
	assert_game_import(&f, r#"game:GetService("ReplicatedStorage"), "moved")"#);
}

#[test]
fn watch_on_disk_env_declaration_refreshes() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({}));
	f.project("game", &["shared"], &json!({}));
	let mut coordinator = f.coordinator("game");
	let before = metadata_work(&coordinator);
	f.write(
		&format!("shared/src/{}", rotor_tsrs::project::ENV_DECL_FILE_NAME),
		&format!(
			"// rotor compiler extension (synthetic, in-memory): the $env compile-time\n// environment macro. This file is injected by the rotor compiler and does\n// not exist on disk.\n{}",
			rotor_tsrs::project::ENV_DECL_BODY
		),
	);
	f.invalidate_and_drain(&mut coordinator, &["shared"]);
	assert_metadata_work(&coordinator, before, 2, 1);
}

/// `programWatchSet`: the program-based watch set `WatchSets` used to build.
fn program_watch_set(project: &SolutionProject) -> (Vec<String>, Vec<String>) {
	let (dir, program) = new_project_program_with_options(
		&rotor_support::gopath::dir(&project.config_path),
		&project.config_path,
		&ProjectOptions::default(),
	)
	.unwrap_or_else(|failure| panic!("{}", failure.error));
	let translator = create_path_translator(program, !project.options.lua_extension);
	let root_dirs = get_root_dirs(program).unwrap_or_default();
	let declaration_dir = crate::project::ProjectOutputHost::options(program)
		.declaration_dir
		.clone();
	(
		root_dirs,
		solution_watch_artifact_directories(
			&dir,
			&translator.out_dir,
			&declaration_dir,
			&project.options.include_path,
		),
	)
}

#[test]
fn watch_sets_match_program_watch_sets() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({"declarationDir": "types"}));
	f.project("plain", &[], &json!({"rootDir": null, "rootDirs": ["src", "extra"]}));
	f.write("plain/extra/more.ts", "export const more = 1;\n");
	f.project("game", &["shared", "plain"], &json!({}));
	f.write("shared/assets/icon.txt", "icon\n");
	f.rojo(
		"default.project.json",
		"",
		&json!({"assets": {"$path": "shared/assets"}}),
	);
	let mut coordinator = f.coordinator("game");
	large_stack(|| {
		let sets = coordinator.watch_sets();
		let solution_artifacts: Vec<String> = sets
			.iter()
			.flat_map(|set| set.artifact_directories.iter().cloned())
			.collect();
		assert!(
			sets[0].rojo_directories.contains(&join(&f.dir, &["shared", "assets"])),
			"Rojo directories = {:?}, want the assets directory",
			sets[0].rojo_directories
		);
		for (index, project) in coordinator.graph.projects.iter().enumerate() {
			let (root_dirs, artifact_directories) = program_watch_set(project);
			assert!(
				sets[index].root_dirs == root_dirs && sets[index].artifact_directories == artifact_directories,
				"{}: watch set roots {:?} artifacts {:?}, program roots {root_dirs:?} artifacts {artifact_directories:?}",
				project.config_path,
				sets[index].root_dirs,
				sets[index].artifact_directories
			);
			let dir = rotor_support::gopath::dir(&project.config_path);
			let rojo_path = resolve_rojo_config_path(&dir, &project.options.rojo_config_path)
				.ok()
				.and_then(|(path, _)| path)
				.unwrap_or_else(|| panic!("{}: no Rojo config", project.config_path));
			let state = new_rojo_watch_state(&rotor_rojo::RojoResolver::from_path(&rojo_path));
			let rojo_directories = filter_rojo_watch_directories(&dir, &state, &solution_artifacts);
			assert!(
				sets[index].rojo_configs == state.configs && sets[index].rojo_directories == rojo_directories,
				"{}: watch set Rojo configs {:?} directories {:?}, fresh {:?} {rojo_directories:?}",
				project.config_path,
				sets[index].rojo_configs,
				sets[index].rojo_directories,
				state.configs
			);
		}
	});
}

/// The Rojo mount check caches each reference's rbxts options; a config reload must read them
/// again.
#[test]
fn watch_reference_rbxts_rojo_edit() {
	let mut f = new_reference_fixture();
	f.project("shared", &[], &json!({}));
	f.project("game", &["shared"], &json!({}));
	f.write(
		"game/src/index.ts",
		r#"import { value } from "../../shared/src"; export const result = value;"#,
	);
	f.rojo("shared/owner.project.json", "../", &json!({}));
	f.json(
		"shared/src/default.project.json",
		&json!({"name": "nested", "tree": {"current": {"$path": "init.luau"}}}),
	);
	f.rojo(
		"default.project.json",
		"",
		&json!({"shared": {"$path": "shared/out/init.luau"}}),
	);
	let mut coordinator = f.coordinator("game");
	let mut config = f.read_json("shared/tsconfig.json");
	config.insert(
		"rbxts".to_owned(),
		json!({"rojo": "owner.project.json", "type": "game"}),
	);
	f.json("shared/tsconfig.json", &Value::Object(config));
	let mut messages = Vec::new();
	let error = if let Err(error) = coordinator.reload_for_watch(&f.path("game/tsconfig.json"), &f.options()) {
		Some(error.to_string())
	} else {
		coordinator.invalidate(&[f.path("shared/tsconfig.json")]);
		let (_, drained, error) = coordinator.drain();
		messages = drained;
		error.map(|error| error.to_string())
	};
	assert!(
		error
			.as_deref()
			.is_some_and(|error| format!("{error}{}", messages.join("\n")).contains("must mount")),
		"inconsistent mount after rbxts edit: {error:?} ({messages:?})"
	);
}
