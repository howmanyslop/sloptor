//! `transformers_integration_test.go`: the real transformer packages of testdata/transformers.

use std::path::Path;

use super::sidecar::{SessionsGuard, must_build, output_text};
use super::{Temp, copy_dir, repo_root};
use crate::project::ProjectOptions;
use crate::sidecar::set_test_env;

/// `transformersFixtureDir`: the fixture must be installed (`bun install --no-save` in
/// testdata/transformers/project); a silently skipped real-package test would claim coverage it
/// did not have, so the port fails instead.
fn transformers_fixture_dir() -> std::path::PathBuf {
	let dir = repo_root().join("testdata/transformers/project");
	assert!(
		dir.join("node_modules/rbxts-transformer-flamework/package.json")
			.exists(),
		"transformers fixture dependencies not installed (run `bun install --no-save` in testdata/transformers/project)"
	);
	dir
}

/// `legacyFlameworkFixture`: the fixture's sources and configs over its installed packages, with
/// the embedded worker (Go's empty `ROTOR_SIDECAR_PATH`) and the env transformer's variable.
fn legacy_flamework_fixture() -> (Temp, SessionsGuard) {
	let fixture = transformers_fixture_dir();
	let temp = Temp::new();
	copy_dir(&fixture.join("src"), &temp.join("src"));
	for name in ["default.project.json", "package.json", "tsconfig.json"] {
		std::fs::copy(fixture.join(name), temp.join(name)).expect("copy");
	}
	#[cfg(unix)]
	std::os::unix::fs::symlink(fixture.join("node_modules"), temp.join("node_modules")).expect("symlink");
	#[cfg(windows)]
	std::os::windows::fs::symlink_dir(fixture.join("node_modules"), temp.join("node_modules")).expect("symlink");
	let sessions = SessionsGuard::new();
	// The extraction lands in the user cache directory, which the port cannot redirect without
	// touching the process environment; it is the directory the CLI extracts to anyway.
	set_test_env("ROTOR_SIDECAR_PATH", "");
	set_test_env("ROTOR_FIXTURE_API_URL", "https://env.example");
	(temp, sessions)
}

#[test]
fn build_project_legacy_flamework_uses_sidecar() {
	let (temp, _sessions) = legacy_flamework_fixture();
	let result = must_build(&temp.dir(), &ProjectOptions::default(), "BuildProjectWithOptions");
	let env_out = output_text(&result, "out/shared/env.luau");
	assert!(
		env_out.contains("https://env.example"),
		"rbxts-transform-env did not inline ROTOR_FIXTURE_API_URL:\n{env_out}"
	);
	assert!(
		!env_out.contains("$env"),
		"rbxts-transform-env left $env macros in output:\n{env_out}"
	);
	let service_out = output_text(&result, "out/server/services/test.service.luau");
	assert!(
		service_out.contains("identifier") && service_out.contains("defineMetadata"),
		"rbxts-transformer-flamework did not inject identifier metadata:\n{service_out}"
	);
	let main_out = output_text(&result, "out/server/main.server.luau");
	assert!(
		!main_out.contains(r#""src/server/services""#),
		"Flamework.addPaths was not rewritten:\n{main_out}"
	);
}

#[test]
fn build_project_legacy_flamework_uses_sidecar_without_active_native_profile() {
	let (temp, _sessions) = legacy_flamework_fixture();
	temp.write("rotor.toml", "[flamework.profiles.\"tsconfig.other.json\"]\n");
	must_build(&temp.dir(), &ProjectOptions::default(), "BuildProjectWithOptions");
	assert!(
		Path::new(&temp.join("out/server/main.server.luau")).exists(),
		"no output"
	);
}

#[test]
#[ignore = "needs Stage 6: native Flamework"]
fn flamework_native_fixture_emits_service_metadata() {}

#[test]
#[ignore = "needs Stage 6: native Flamework"]
fn flamework_native_add_paths_rebuilds_on_added_and_removed_module() {}
