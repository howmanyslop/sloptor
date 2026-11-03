//! `solution_watch_test.go` and `solution_watch_config_paths_test.go`: the coordinator's watch
//! entry points. The watch loop itself is Stage 7.

use std::path::Path;
use std::sync::Arc;

use super::solution::{
	RecordingDrainer, builders, coordinator_with, join, write_buildable_solution_project, write_solution_config,
	write_solution_file,
};
use super::solution_drain::{must_read_file, write_cross_project_solution};
use super::{Temp, large_stack};
use crate::crossproject::path_within_any_root;
use crate::project::ProjectOptions;
use crate::rbxts_options::RbxtsOptions;
use crate::solution::SolutionCoordinator;
use crate::solution_watch::{WatchAssetEvent, canonical_watch_paths};

fn symlink(original: &str, link: &str) {
	#[cfg(unix)]
	std::os::unix::fs::symlink(original, link).unwrap_or_else(|error| panic!("cannot create symlink: {error}"));
	#[cfg(windows)]
	std::os::windows::fs::symlink_dir(original, link).unwrap_or_else(|error| panic!("cannot create symlink: {error}"));
}

fn new_coordinator(config: &str, options: &ProjectOptions) -> SolutionCoordinator {
	large_stack(|| SolutionCoordinator::new(config, options))
		.unwrap_or_else(|error| panic!("NewSolutionCoordinator: {error}"))
}

fn drain(coordinator: &mut SolutionCoordinator, what: &str) {
	let (_, messages, error) = coordinator.drain();
	if let Some(error) = error {
		panic!("{what}: {error} ({messages:?})");
	}
}

/// Prepends `prefix` to the config object at `path`.
fn extend_config(path: &str, prefix: &str) {
	let config = String::from_utf8(must_read_file(path)).expect("utf8");
	std::fs::write(path, format!("{prefix}{}", &config[1..])).expect("write");
}

#[test]
fn rojo_watch_invalidation() {
	let temp = Temp::new();
	let root = temp.dir();
	write_solution_config(&root, "tsconfig.json", &["./app"], true);
	write_solution_config(&join(&root, &["app"]), "tsconfig.json", &["../shared"], false);
	write_solution_config(&join(&root, &["shared"]), "tsconfig.json", &[], false);
	let drainer = Arc::new(RecordingDrainer::default());
	let mut coordinator = coordinator_with(&join(&root, &["tsconfig.json"]), &builders(1), drainer.clone());
	drain(&mut coordinator, "initial Drain");
	crate::project::lock(&drainer.drained).clear();
	let shared_config = join(&root, &["shared", "tsconfig.json"]);
	assert_eq!(
		coordinator.invalidate(std::slice::from_ref(&shared_config)),
		[shared_config, join(&root, &["app", "tsconfig.json"])],
		"Invalidate()"
	);
	drain(&mut coordinator, "Drain after Rojo invalidation");
	assert_eq!(drainer.drained(), ["shared", "app"], "drained projects");
}

#[test]
fn watch_excludes_out_dir() {
	let temp = Temp::new();
	let project_dir = temp.dir();
	write_buildable_solution_project(&project_dir);
	let config_path = join(&project_dir, &["tsconfig.json"]);
	let config = String::from_utf8(must_read_file(&config_path)).expect("utf8").replacen(
		r#","include""#,
		r#","rbxts":{"rojo":"./game.project.json"},"include""#,
		1,
	);
	std::fs::write(&config_path, config).expect("write");
	std::fs::write(
		join(&project_dir, &["game.project.json"]),
		r#"{
		"name": "watch-artifacts",
		"tree": {
			"assets": {"$path": "assets"},
			"out": {"$path": "out"},
			"include": {"$path": "include"},
			"cache": {"$path": ".rotor"}
		}
	}"#,
	)
	.expect("write");
	let asset_dir = join(&project_dir, &["assets"]);
	for directory in [
		asset_dir.clone(),
		join(&project_dir, &["out", "nested"]),
		join(&project_dir, &["include", "nested"]),
		join(&project_dir, &[".rotor", "cache"]),
	] {
		std::fs::create_dir_all(directory).expect("mkdir");
	}
	let mut coordinator = new_coordinator(&config_path, &ProjectOptions::default());
	let watch_sets = coordinator.watch_sets();
	assert_eq!(watch_sets.len(), 1, "WatchSets() returned sets");
	let set = &watch_sets[0];
	assert!(
		path_within_any_root(&asset_dir, &set.rojo_directories),
		"RojoDirectories = {:?}, want asset directory {asset_dir:?}",
		set.rojo_directories
	);
	for artifact_dir in [
		join(&project_dir, &["out"]),
		join(&project_dir, &["include"]),
		join(&project_dir, &[".rotor"]),
	] {
		assert!(
			!path_within_any_root(&artifact_dir, &set.rojo_directories),
			"RojoDirectories = {:?}, should exclude {artifact_dir:?} and descendants",
			set.rojo_directories
		);
	}
}

#[test]
fn tsconfig_chain_reload() {
	let temp = Temp::new();
	let root = temp.dir();
	let child_dir = join(&root, &["child"]);
	let root_config = join(&root, &["tsconfig.json"]);
	let child_config = join(&child_dir, &["tsconfig.json"]);
	let base_config = join(&root, &["base.json"]);
	write_solution_config(&root, "tsconfig.json", &["./child"], true);
	write_solution_config(&child_dir, "tsconfig.json", &[], false);
	std::fs::write(&base_config, r#"{"rbxts":{"rojo":"./first.project.json"}}"#).expect("write");
	std::fs::write(&child_config, r#"{"extends":"../base.json"}"#).expect("write");
	let mut coordinator = new_coordinator(&root_config, &ProjectOptions::default());
	let want = canonical_watch_paths(&[child_config.clone(), base_config.clone()]);
	let child_paths = coordinator
		.watch_sets()
		.into_iter()
		.find(|set| set.project_path == child_config)
		.map(|set| set.ts_config_paths);
	assert_eq!(child_paths, Some(want), "child TsConfigPaths");
	std::fs::write(&base_config, r#"{"rbxts":{"rojo":"./second.project.json"}}"#).expect("write");
	coordinator
		.reload(&root_config, &ProjectOptions::default())
		.unwrap_or_else(|error| panic!("Reload: {error}"));
	let state = coordinator
		.project_state(&child_config)
		.expect("child project state missing after Reload");
	assert_eq!(
		state.project.options.rojo_config_path,
		join(&root, &["second.project.json"]),
		"RojoConfigPath"
	);
}

#[test]
fn watch_sets_include_skipped_coordinator_config_chain() {
	let temp = Temp::new();
	let root = temp.dir();
	let root_config = join(&root, &["tsconfig.json"]);
	let base_config = join(&root, &["tsconfig.base.json"]);
	write_solution_file(&root, "tsconfig.base.json", r#"{"files":[],"include":[]}"#);
	write_solution_file(
		&root,
		"tsconfig.json",
		r#"{"extends":"./tsconfig.base.json","references":[{"path":"./child"}]}"#,
	);
	write_buildable_solution_project(&join(&root, &["child"]));
	let mut coordinator = new_coordinator(&root_config, &ProjectOptions::default());
	let sets = coordinator.watch_sets();
	let set = sets
		.iter()
		.find(|set| set.project_path == root_config)
		.unwrap_or_else(|| panic!("WatchSets() = {sets:?}, want skipped coordinator config chain"));
	assert_eq!(
		set.ts_config_paths,
		canonical_watch_paths(&[root_config.clone(), base_config]),
		"coordinator config paths"
	);
}

#[test]
fn skipped_coordinator_config_invalidates_referenced_projects() {
	let temp = Temp::new();
	let root = temp.dir();
	let root_config = join(&root, &["tsconfig.json"]);
	let child_config = join(&root, &["child", "tsconfig.json"]);
	write_solution_file(
		&root,
		"tsconfig.base.json",
		r#"{"files":[],"include":[],"rbxts":{"luau":true}}"#,
	);
	write_solution_file(
		&root,
		"tsconfig.json",
		r#"{"extends":"./tsconfig.base.json","references":[{"path":"./child"}]}"#,
	);
	write_buildable_solution_project(&join(&root, &["child"]));
	let drainer = Arc::new(RecordingDrainer::default());
	let entry = ProjectOptions {
		solution_argv: Some(RbxtsOptions::default()),
		..ProjectOptions::default()
	};
	let mut coordinator = coordinator_with(&root_config, &entry, drainer.clone());
	drain(&mut coordinator, "initial Drain");
	crate::project::lock(&drainer.drained).clear();
	write_solution_file(
		&root,
		"tsconfig.base.json",
		r#"{"files":[],"include":[],"rbxts":{"luau":false}}"#,
	);
	let clean_stale_outputs = coordinator
		.reload_for_watch(&root_config, &entry)
		.unwrap_or_else(|error| panic!("ReloadForWatch: {error}"));
	assert_eq!(
		coordinator.invalidate(std::slice::from_ref(&root_config)),
		[child_config],
		"Invalidate(root coordinator)"
	);
	drain(&mut coordinator, "reloaded Drain");
	clean_stale_outputs().unwrap_or_else(|error| panic!("clean stale outputs: {error}"));
	assert_eq!(drainer.drained(), ["child"], "drained projects");
}

#[test]
fn tsconfig_chain_reload_cleans_stale_lua_extension() {
	let temp = Temp::new();
	let root = temp.dir();
	let child_dir = join(&root, &["child"]);
	let root_config = join(&root, &["tsconfig.json"]);
	let child_config = join(&child_dir, &["tsconfig.json"]);
	let base_config = join(&root, &["base.json"]);
	write_solution_config(&root, "tsconfig.json", &["./child"], true);
	write_buildable_solution_project(&child_dir);
	extend_config(&child_config, r#"{"extends":"../base.json","#);
	std::fs::write(&base_config, r#"{"rbxts":{"luau":true}}"#).expect("write");
	let mut coordinator = new_coordinator(&root_config, &ProjectOptions::default());
	large_stack(|| drain(&mut coordinator, "initial Drain"));
	let old_output = join(&child_dir, &["out", "main.luau"]);
	assert!(Path::new(&old_output).exists(), "initial output");
	std::fs::write(&base_config, r#"{"rbxts":{"luau":false}}"#).expect("write");
	let clean_stale_outputs = coordinator
		.reload_for_watch(&root_config, &ProjectOptions::default())
		.unwrap_or_else(|error| panic!("ReloadForWatch: {error}"));
	coordinator.invalidate(&[child_config]);
	large_stack(|| drain(&mut coordinator, "reloaded Drain"));
	clean_stale_outputs().unwrap_or_else(|error| panic!("clean stale outputs: {error}"));
	assert!(
		Path::new(&join(&child_dir, &["out", "main.lua"])).exists(),
		"new extension output"
	);
	assert!(!Path::new(&old_output).exists(), "old extension output still exists");
}

/// Copies an asset through `drain_assets`, then deletes it the same way.
fn assert_asset_round_trip(coordinator: &mut SolutionCoordinator, config: &str, asset: &str, output: &str, text: &str) {
	std::fs::write(asset, text).expect("write");
	coordinator
		.drain_assets(
			config,
			&[WatchAssetEvent {
				path: asset.to_owned(),
				deleted: false,
			}],
		)
		.unwrap_or_else(|error| panic!("DrainAssets copy: {error}"));
	let got = std::fs::read_to_string(output);
	assert!(got.as_deref().is_ok_and(|got| got == text), "copied asset = {got:?}");
	std::fs::remove_file(asset).expect("remove");
	coordinator
		.drain_assets(
			config,
			&[WatchAssetEvent {
				path: asset.to_owned(),
				deleted: true,
			}],
		)
		.unwrap_or_else(|error| panic!("DrainAssets delete: {error}"));
	assert!(!Path::new(output).exists(), "asset output still exists");
}

#[test]
fn watch_asset_drain() {
	let (_temp, root, lib_dir, _) = write_cross_project_solution();
	let lib_config = join(&lib_dir, &["tsconfig.json"]);
	let mut coordinator = new_coordinator(&join(&root, &["tsconfig.json"]), &ProjectOptions::default());
	large_stack(|| drain(&mut coordinator, "initial Drain"));
	assert_asset_round_trip(
		&mut coordinator,
		&lib_config,
		&join(&lib_dir, &["src", "watch-asset.luau"]),
		&join(&lib_dir, &["out", "watch-asset.luau"]),
		"return { watch = true }\n",
	);
}

#[test]
fn watch_asset_drain_normalizes_canonical_event_path() {
	let temp = Temp::new();
	let root = temp.dir();
	let project_dir = join(&root, &["project"]);
	write_buildable_solution_project(&project_dir);
	let project_alias = join(&root, &["project-alias"]);
	symlink(&project_dir, &project_alias);
	let config_path = join(&project_alias, &["tsconfig.json"]);
	let mut coordinator = new_coordinator(&config_path, &ProjectOptions::default());
	large_stack(|| drain(&mut coordinator, "initial Drain"));
	assert_asset_round_trip(
		&mut coordinator,
		&config_path,
		&join(&project_dir, &["src", "watch-asset.luau"]),
		&join(&project_dir, &["out", "watch-asset.luau"]),
		"return { watch = true }\n",
	);
}

#[test]
fn watch_asset_drain_finds_rojo_inputs_through_config_aliases() {
	for spelling in ["symlink", "relative"] {
		let temp = Temp::new();
		let root = temp.dir();
		let project_dir = join(&root, &["project"]);
		write_buildable_solution_project(&project_dir);
		let config_path = join(&project_dir, &["tsconfig.json"]);
		let config = String::from_utf8(must_read_file(&config_path)).expect("utf8").replacen(
			r#""outDir":"out""#,
			r#""outDir":"build/out""#,
			1,
		);
		std::fs::write(&config_path, config).expect("write");
		write_solution_file(&project_dir, "assets/icon.luau", "return { external = true }\n");
		write_solution_file(
			&project_dir,
			"game.project.json",
			r#"{"name":"external-assets","tree":{"assets":{"$path":"assets"}}}"#,
		);
		let mut coordinator = new_coordinator(
			&config_path,
			&ProjectOptions {
				rojo_config_path: join(&project_dir, &["game.project.json"]),
				..ProjectOptions::default()
			},
		);
		let request_path = if spelling == "symlink" {
			let alias = join(&root, &["alias"]);
			symlink(&project_dir, &alias);
			join(&alias, &["tsconfig.json"])
		} else {
			// Each nextest test runs in its own process, so changing the directory is t.Chdir.
			std::env::set_current_dir(&project_dir).expect("chdir");
			"tsconfig.json".to_owned()
		};
		let asset = join(&project_dir, &["assets/icon.luau"]);
		let output = join(&project_dir, &["build/assets/icon.luau"]);
		std::fs::remove_file(&asset).expect("remove");
		assert_asset_round_trip(
			&mut coordinator,
			&request_path,
			&asset,
			&output,
			"return { external = true }\n",
		);
	}
}

#[test]
fn watch_sets_canonicalize_config_paths() {
	let temp = Temp::new();
	let root = temp.dir();
	let project_dir = join(&root, &["project"]);
	write_buildable_solution_project(&project_dir);
	let project_alias = join(&root, &["project-alias"]);
	symlink(&project_dir, &project_alias);
	let config_path = join(&project_dir, &["tsconfig.json"]);
	extend_config(&config_path, r#"{"extends":"./tsconfig.base.json","#);
	std::fs::write(
		join(&project_dir, &["tsconfig.base.json"]),
		r#"{"rbxts":{"rojo":"./game.project.json"}}"#,
	)
	.expect("write");
	std::fs::write(
		join(&project_dir, &["game.project.json"]),
		r#"{"name":"watch-configs","tree":{"Nested":{"$path":"nested.project.json"}}}"#,
	)
	.expect("write");
	std::fs::write(
		join(&project_dir, &["nested.project.json"]),
		r#"{"name":"nested","tree":{}}"#,
	)
	.expect("write");
	let mut coordinator = new_coordinator(&join(&project_alias, &["tsconfig.json"]), &ProjectOptions::default());
	let watch_sets = coordinator.watch_sets();
	assert_eq!(watch_sets.len(), 1, "WatchSets() returned sets");
	let canonical = rotor_support::fsutil::eval_symlinks(&project_dir).expect("canonical");
	assert_eq!(
		watch_sets[0].ts_config_paths,
		[
			join(&canonical, &["tsconfig.json"]),
			join(&canonical, &["tsconfig.base.json"])
		],
		"TsConfigPaths"
	);
	assert_eq!(
		watch_sets[0].rojo_configs,
		[
			join(&canonical, &["game.project.json"]),
			join(&canonical, &["nested.project.json"])
		],
		"RojoConfigs"
	);
}
