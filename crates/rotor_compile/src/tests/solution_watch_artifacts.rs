//! `solution_watch_artifacts_test.go` and `solution_watch_memo_test.go`: watch sets leave out
//! the solution's own artifacts.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;

use super::Temp;
use super::project_reference_import::new_reference_fixture;
use super::solution::join;
use crate::crossproject::{CrossProjectMetadata, SolutionWriteMetadata, path_within_any_root};
use crate::reference_validation::ReferenceRojoCache;
use crate::solution::{SolutionCoordinator, SolutionGraph, SolutionProject};
use crate::solution_drain::{Drainer, SolutionBuildDrainer};
use crate::solution_watch::{
	SolutionWatchSet, canonical_watch_directory, canonical_watch_paths, filter_solution_watch_artifacts,
};

fn symlink(original: &str, link: &str) {
	#[cfg(unix)]
	std::os::unix::fs::symlink(original, link).unwrap_or_else(|error| panic!("cannot create symlink: {error}"));
	#[cfg(windows)]
	std::os::windows::fs::symlink_dir(original, link).unwrap_or_else(|error| panic!("cannot create symlink: {error}"));
}

#[test]
fn watch_sets_exclude_artifacts_owned_by_other_projects() {
	let mut f = new_reference_fixture();
	f.project("logger", &[], &json!({}));
	f.project("game", &["logger"], &json!({}));
	f.write("game/out/generated/nested.luau", "return {}\n");
	f.write("game/include/runtime.luau", "return {}\n");
	f.write("game/.rotor/cache/state.json", "{}\n");
	f.write("game/next/nested/asset.luau", "return {}\n");
	f.write("external-assets/icon.png", "external asset\n");
	f.rojo(
		"default.project.json",
		"",
		&json!({
			"gameInclude": {"$path": "game/include"},
			"gameCache": {"$path": "game/.rotor"},
			"next": {"$path": "game/next"},
			"external": {"$path": "external-assets"},
		}),
	);
	let mut coordinator =
		SolutionCoordinator::new(&f.path("game/tsconfig.json"), &f.options()).unwrap_or_else(|error| panic!("{error}"));
	let external = join(&f.dir, &["external-assets"]);
	let assert_artifacts = |sets: Vec<SolutionWatchSet>| {
		let artifacts: Vec<String> = sets
			.iter()
			.flat_map(|set| canonical_watch_paths(&set.artifact_directories))
			.collect();
		for set in &sets {
			for directory in &set.rojo_directories {
				assert!(
					!path_within_any_root(&canonical_watch_directory(directory), &artifacts),
					"{} watches solution artifact {directory}",
					set.project_path
				);
			}
			assert!(
				set.rojo_directories.contains(&external),
				"{} lost intentional external assets: {:?}",
				set.project_path,
				set.rojo_directories
			);
		}
	};
	assert_artifacts(coordinator.watch_sets());
	// Updating one project's output ownership must also filter the cached Rojo set of an
	// otherwise unchanged project that inherits the same tree.
	f.set_compiler_option("game", "outDir", json!("next"));
	coordinator.invalidate(&[f.path("game/tsconfig.json")]);
	assert_artifacts(coordinator.watch_sets());
}

#[test]
fn canonical_watch_directory_preserves_missing_parents() {
	let temp = Temp::new();
	let root = temp.dir();
	let canonical = rotor_support::fsutil::eval_symlinks(&root).expect("the canonical root");
	let path = join(&root, &["missing", "nested", "out"]);
	assert_eq!(
		canonical_watch_directory(&path),
		join(&canonical, &["missing", "nested", "out"]),
		"canonical missing directory"
	);
}

#[test]
fn canonical_watch_directory_unicode_normalization() {
	let temp = Temp::new();
	let existing = join(&temp.dir(), &["cafe\u{301}"]);
	std::fs::create_dir_all(&existing).expect("the directory");
	let nfc = icu_normalizer::ComposingNormalizerBorrowed::new_nfc();
	for path in [existing.clone(), join(&existing, &["missing\u{301}", "nested"])] {
		let canonical = canonical_watch_directory(&path);
		let want_nfc = cfg!(target_os = "macos");
		assert_eq!(
			nfc.is_normalized(&canonical),
			want_nfc,
			"canonical path {canonical:?}, want NFC={want_nfc}"
		);
	}
}

#[test]
fn watch_sets_exclude_artifact_symlink_aliases() {
	let mut f = new_reference_fixture();
	f.project("logger", &[], &json!({}));
	f.project("game", &["logger"], &json!({}));
	f.write("game/out/asset.luau", "return {}\n");
	let alias = f.path("generated-alias");
	symlink(&f.path("game/out"), &alias);
	f.rojo(
		"default.project.json",
		"",
		&json!({"generatedAlias": {"$path": "generated-alias"}}),
	);
	let mut coordinator =
		SolutionCoordinator::new(&f.path("game/tsconfig.json"), &f.options()).unwrap_or_else(|error| panic!("{error}"));
	for set in coordinator.watch_sets() {
		for directory in &set.rojo_directories {
			assert_ne!(
				canonical_watch_directory(directory),
				canonical_watch_directory(&alias),
				"{} watches artifact through symlink {directory}",
				set.project_path
			);
		}
	}
}

#[test]
fn solution_watch_artifact_filter_canonicalizes_unique_paths_once() {
	let temp = Temp::new();
	let root = temp.dir();
	let out = join(&root, &["out"]);
	let alias = join(&root, &["alias"]);
	let assets = join(&root, &["cafe\u{301}"]);
	let spelled_out = format!("{out}{}.", std::path::MAIN_SEPARATOR);
	let directories = vec![out.clone(), alias.clone(), assets.clone(), assets.clone()];
	let mut sets = vec![
		SolutionWatchSet {
			artifact_directories: vec![out.clone(), alias.clone()],
			root_dirs: vec![out.clone()],
			rojo_directories: directories.clone(),
			..SolutionWatchSet::default()
		},
		SolutionWatchSet {
			artifact_directories: vec![spelled_out],
			rojo_directories: directories.clone(),
			..SolutionWatchSet::default()
		},
	];
	let calls = std::sync::Mutex::new(HashMap::<String, i32>::new());
	filter_solution_watch_artifacts(&mut sets, &|path| {
		let path = rotor_support::gopath::clean(&rotor_support::gopath::from_slash(path));
		*crate::project::lock(&calls).entry(path.clone()).or_default() += 1;
		if path == alias { out.clone() } else { path }
	});
	let want: HashMap<String, i32> = [(out.clone(), 1), (alias.clone(), 1), (assets.clone(), 1)].into();
	assert_eq!(*crate::project::lock(&calls), want, "canonical resolutions");
	for set in &sets {
		assert_eq!(
			set.rojo_directories,
			[assets.clone(), assets.clone()],
			"filtered directories, want duplicate intentional assets preserved"
		);
	}
	assert_eq!(
		sets[0].root_dirs,
		std::slice::from_ref(&out),
		"explicit source root inside artifact was changed"
	);
	assert_eq!(
		directories,
		[out, alias, assets.clone(), assets],
		"cached directory slice was mutated"
	);
}

/// A coordinator over one project whose watch set is `cached`.
fn cached_coordinator(config: &str, cached: SolutionWatchSet) -> SolutionCoordinator {
	let graph = SolutionGraph {
		projects: vec![SolutionProject {
			config_path: config.to_owned(),
			..SolutionProject::default()
		}],
		..SolutionGraph::default()
	};
	let mut coordinator = SolutionCoordinator::assemble(
		graph,
		Drainer::Build(SolutionBuildDrainer::default()),
		SolutionWriteMetadata::default(),
		CrossProjectMetadata {
			infos: HashMap::new(),
			references: Arc::new(ReferenceRojoCache::new(None)),
			config_parses: 0,
			refreshes: 0,
		},
		1,
		None,
	)
	.unwrap_or_else(|error| panic!("{error}"));
	coordinator.watch_sets.insert(config.to_owned(), cached);
	coordinator
}

#[test]
fn watch_sets_rechecks_cached_symlink_ownership() {
	let temp = Temp::new();
	let root = temp.dir();
	let out = join(&root, &["out"]);
	let external = join(&root, &["external"]);
	for path in [&out, &external] {
		std::fs::create_dir_all(path).expect("the directory");
	}
	let alias = join(&root, &["alias"]);
	symlink(&out, &alias);
	let config = join(&root, &["tsconfig.json"]);
	let cached = SolutionWatchSet {
		project_path: config.clone(),
		artifact_directories: vec![out],
		root_dirs: vec![alias.clone()],
		rojo_directories: vec![alias.clone(), external.clone()],
		..SolutionWatchSet::default()
	};
	let mut coordinator = cached_coordinator(&config, cached.clone());
	assert_eq!(
		coordinator.watch_sets()[0].rojo_directories,
		std::slice::from_ref(&external),
		"artifact alias was not excluded"
	);
	std::fs::remove_file(&alias).expect("remove");
	symlink(&external, &alias);
	let sets = coordinator.watch_sets();
	assert_eq!(
		sets[0].rojo_directories,
		[alias, external],
		"retargeted alias was not restored without invalidation"
	);
	assert!(
		sets[0].root_dirs == cached.root_dirs && coordinator.watch_sets[&config] == cached,
		"ownership filtering changed explicit roots or the cached set"
	);
}

#[test]
fn watch_sets_memo_preserves_routing_and_cached_directories() {
	let temp = Temp::new();
	let root = temp.dir();
	let out = join(&root, &["missing", "out"]);
	let generated = join(&out, &["nested"]);
	let nfd = join(&root, &["cafe\u{301}"]);
	let external = join(&root, &["missing", "assets"]);
	std::fs::create_dir_all(&nfd).expect("the directory");
	let config = join(&root, &["tsconfig.json"]);
	let mut cached = SolutionWatchSet {
		project_path: config.clone(),
		artifact_directories: vec![out.clone(), out],
		root_dirs: vec![generated.clone()],
		rojo_directories: vec![generated.clone(), nfd.clone(), external.clone()],
		..SolutionWatchSet::default()
	};
	let mut coordinator = cached_coordinator(&config, cached.clone());
	let mut first = coordinator.watch_sets();
	let second = coordinator.watch_sets();
	assert!(
		first == second && first[0].rojo_directories == [nfd.clone(), external.clone()],
		"cached watch routing changed: first={first:?}, second={second:?}"
	);
	assert_eq!(
		first[0].root_dirs,
		std::slice::from_ref(&generated),
		"explicit source root inside artifact changed"
	);
	first[0].rojo_directories[0] = "caller mutation".to_owned();
	assert_eq!(
		coordinator.watch_sets(),
		second,
		"returned Rojo directories mutated cache"
	);
	cached.artifact_directories = vec![nfd];
	coordinator.watch_sets.insert(config, cached);
	assert_eq!(
		coordinator.watch_sets()[0].rojo_directories,
		[generated, external],
		"changed artifact ownership did not refresh filter"
	);
}
