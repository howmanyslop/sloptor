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
pub fn legacy_flamework_fixture() -> (Temp, SessionsGuard) {
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

/// `nativeFlameworkFixtureDir`: Go links the legacy fixture's packages into
/// testdata/transformers/native/project and builds there; nextest runs tests in concurrent
/// processes that would race on that link and its outputs, so the port copies the fixture into a
/// temporary directory with the packages linked, as Go's add-paths test does.
fn native_flamework_fixture() -> Temp {
	let fixture_root = repo_root().join("testdata/transformers");
	let legacy_node_modules = fixture_root.join("project/node_modules");
	assert!(
		legacy_node_modules.join("@flamework/core/package.json").exists(),
		"Flamework fixture dependencies not installed (run `bun install --no-save` in testdata/transformers/project)"
	);
	let fixture = fixture_root.join("native/project");
	let temp = Temp::new();
	copy_dir(&fixture.join("src"), &temp.join("src"));
	for name in ["default.project.json", "package.json", "tsconfig.json", "rotor.toml"] {
		std::fs::copy(fixture.join(name), temp.join(name)).expect("copy");
	}
	super::solution_flamework_integration::symlink(&legacy_node_modules, &temp.join("node_modules"));
	temp
}

#[test]
fn flamework_native_fixture_emits_service_metadata() {
	// Given: a native-mode fixture with no rbxts-transformer-flamework plugin.
	let temp = native_flamework_fixture();

	// When: the real build pipeline compiles the [flamework]-enabled project.
	let result = must_build(&temp.dir(), &ProjectOptions::default(), "BuildProjectWithOptions");

	// Then: native output contains the observable Flamework metadata emitted by the legacy
	// transformer.
	let service_out = output_text(&result, "out/server/services/test.service.luau");
	assert!(
		service_out.contains("identifier") && service_out.contains("defineMetadata"),
		"native [flamework] mode did not inject identifier metadata:\n{service_out}"
	);
	// addPaths expands to a static import so bundlers can follow it.
	let main_out = output_text(&result, "out/server/main.server.luau");
	let import_line = r#"TS.import(script, game:GetService("ServerScriptService"), "TS", "services", "test.service")"#;
	let before = main_out.find(r#"print("before addPaths")"#);
	let import_at = main_out.find(import_line);
	let after = main_out.find(r#"print("after addPaths")"#);
	let ordered = matches!((before, import_at, after), (Some(before), Some(import_at), Some(after)) if import_at > before && after > import_at);
	assert!(
		ordered && !main_out.contains("_addPaths") && !main_out.contains("TS.Promise.new"),
		"native addPaths did not expand to a static import:\n{main_out}"
	);
}

#[test]
fn flamework_native_add_paths_rebuilds_on_added_and_removed_module() {
	let temp = native_flamework_fixture();
	let dir = temp.dir();
	let build = || -> String {
		let result = must_build(&dir, &ProjectOptions::default(), "BuildProjectWithOptions");
		output_text(&result, "out/server/main.server.luau")
	};
	let module = temp.join("src/server/services/added.ts");
	let output = build();
	assert!(
		!output.contains(r#""added""#),
		"initial build imported nonexistent module:\n{output}"
	);
	std::fs::write(&module, "export {};\n").expect("write");
	let output = build();
	assert!(
		output.contains(r#""services", "added""#),
		"added module did not invalidate addPaths:\n{output}"
	);
	std::fs::remove_file(&module).expect("remove");
	let output = build();
	assert!(
		!output.contains(r#""services", "added""#),
		"removed module survived addPaths rebuild:\n{output}"
	);
	let build_info = std::fs::read_to_string(temp.join("flamework.build")).expect("read flamework.build");
	assert!(
		!build_info.contains(r#""globs""#),
		"plain addPaths changed runtime glob metadata: {build_info}"
	);
}
