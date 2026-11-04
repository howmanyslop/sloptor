//! `task7_diag_dirty_state_test.go`

use std::path::{Path, PathBuf};
use std::process::Command;

use super::sidecar::build;
use super::task7_acceptance::task7_artifact_hash;
use super::{Temp, copy_dir, repo_root};
use crate::project::ProjectOptions;

/// `task7DIAG002Message`
const TASK7_DIAG002_MESSAGE: &str = "Flamework cannot be built in a dirty environment, please delete your tsbuildinfo";

/// `task7DIAG002Result`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Task7Diag002Result {
	exit: i32,
	diagnostic: String,
}

type Lane = fn(&str) -> Task7Diag002Result;

#[test]
fn task7_diag002_dirty_incremental_state_matches_public_upstream() {
	// Given: clean and warm incremental builds in independent real compiler lanes.
	let (fixture, modules) = task7_diag002_paths();
	let upstream = task7_diag002_project(&fixture, &modules, false);
	let native = task7_diag002_project(&fixture, &modules, true);
	let runs: [(&str, &Temp, Lane); 4] = [
		("upstream-clean", &upstream, task7_diag002_upstream),
		("upstream-warm", &upstream, task7_diag002_upstream),
		("native-clean", &native, task7_diag002_native),
		("native-warm", &native, task7_diag002_native),
	];
	for (name, root, run) in runs {
		let result = run(&root.dir());
		assert!(
			result.exit == 0 && result.diagnostic.is_empty(),
			"{name} = exit {} diagnostic {:?}, want clean success",
			result.exit,
			result.diagnostic
		);
	}

	// When: flamework.build disappears while each configured incremental state file remains.
	let lanes: [(&str, &Temp, &str, Lane); 2] = [
		("upstream", &upstream, "out/task7.tsbuildinfo", task7_diag002_upstream),
		("native", &native, "out/task7.rbxtsc.tsbuildinfo", task7_diag002_native),
	];
	for (name, root, state, run) in lanes {
		let root_dir = root.dir();
		assert!(
			root.join(state).exists(),
			"{name} incremental state precondition: {state} missing"
		);
		std::fs::remove_file(root.join("flamework.build")).expect("remove flamework.build");
		let before = task7_artifact_hash(&root_dir);
		let first = run(&root_dir);
		let second = run(&root_dir);
		assert!(
			first.exit == 1 && first.diagnostic == TASK7_DIAG002_MESSAGE,
			"{name} dirty result = exit {} diagnostic {:?}, want exit 1 {TASK7_DIAG002_MESSAGE:?}",
			first.exit,
			first.diagnostic
		);
		assert_eq!(second, first, "{name} dirty result changed");
		let after = task7_artifact_hash(&root_dir);
		assert_eq!(after, before, "{name} dirty state wrote artifacts");
		assert!(
			!root.join("flamework.build").exists(),
			"{name} dirty state recreated flamework.build"
		);
	}
}

#[test]
fn task7_diag002_malformed_persisted_state_fails_closed() {
	// Given: independent clean incremental projects with each real compiler-state adapter.
	let (fixture, modules) = task7_diag002_paths();
	let lanes: [(&str, &str, bool, Lane); 2] = [
		("upstream", "out/task7.tsbuildinfo", false, task7_diag002_upstream),
		("native", "out/task7.rbxtsc.tsbuildinfo", true, task7_diag002_native),
	];
	for (name, state, native, run) in lanes {
		let root = task7_diag002_project(&fixture, &modules, native);
		let root_dir = root.dir();
		let result = run(&root_dir);
		assert!(
			result.exit == 0 && result.diagnostic.is_empty(),
			"{name} clean result = {result:?}, want success"
		);
		std::fs::remove_file(root.join("flamework.build")).expect("remove flamework.build");

		// When: the remaining persisted compiler state becomes malformed.
		std::fs::write(root.join(state), "{\n").expect("write state");
		let before = task7_artifact_hash(&root_dir);
		let result = run(&root_dir);

		// Then: state content cannot turn the missing-build-info condition into a write or
		// success.
		assert!(
			result.exit == 1 && result.diagnostic == TASK7_DIAG002_MESSAGE,
			"{name} malformed result = {result:?}"
		);
		assert_eq!(
			task7_artifact_hash(&root_dir),
			before,
			"{name} malformed state wrote artifacts"
		);
	}
}

/// `task7DIAG002Paths`
fn task7_diag002_paths() -> (PathBuf, PathBuf) {
	let repo = repo_root();
	(
		repo.join("testdata/compile-unit/flamework/task7-diag-dirty"),
		repo.join("testdata/transformers/project/node_modules"),
	)
}

/// `task7DIAG002Project`
fn task7_diag002_project(fixture: &Path, modules: &Path, native: bool) -> Temp {
	let root = Temp::new();
	copy_dir(fixture, &root.join(""));
	#[cfg(unix)]
	std::os::unix::fs::symlink(modules, root.join("node_modules")).expect("symlink");
	#[cfg(windows)]
	std::os::windows::fs::symlink_dir(modules, root.join("node_modules")).expect("symlink");
	if !native {
		return root;
	}
	let config = std::fs::read_to_string(root.join("tsconfig.json")).expect("read tsconfig");
	let native_config = config.replacen(
		r#""plugins": [{ "transform": "rbxts-transformer-flamework", "idGenerationMode": "short", "salt": "task7-diag-dirty" }]"#,
		r#""plugins": []"#,
		1,
	);
	assert_ne!(
		native_config, config,
		"native fixture did not remove the legacy transformer"
	);
	root.write("tsconfig.json", native_config);
	root.write(
		"rotor.toml",
		"[flamework]\nidGenerationMode = \"short\"\nsalt = \"task7-diag-dirty\"\n",
	);
	root
}

/// `task7DIAG002Upstream`: the pinned `rbxtsc` over the project. Go bounds it with a 30 second
/// context.
fn task7_diag002_upstream(root: &str) -> Task7Diag002Result {
	let output = Command::new(Path::new(root).join("node_modules/.bin/rbxtsc"))
		.args(["-p", "tsconfig.json"])
		.current_dir(root)
		.output()
		.expect("upstream command");
	if output.status.success() {
		return Task7Diag002Result::default();
	}
	let combined = format!(
		"{}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	let mut diagnostic = combined.trim().to_owned();
	if diagnostic.contains(TASK7_DIAG002_MESSAGE) {
		TASK7_DIAG002_MESSAGE.clone_into(&mut diagnostic);
	}
	Task7Diag002Result {
		exit: output.status.code().unwrap_or(-1),
		diagnostic,
	}
}

/// `task7DIAG002Native`
fn task7_diag002_native(root: &str) -> Task7Diag002Result {
	let (_, diagnostics, error) = build(root, &ProjectOptions::default());
	if error.is_none() {
		return Task7Diag002Result::default();
	}
	let mut diagnostic = diagnostics.join("\n");
	if diagnostic.contains(TASK7_DIAG002_MESSAGE) {
		TASK7_DIAG002_MESSAGE.clone_into(&mut diagnostic);
	}
	Task7Diag002Result { exit: 1, diagnostic }
}
