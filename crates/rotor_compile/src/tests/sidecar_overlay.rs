//! `sidecar_overlay_test.go`: caller overlays reach the worker and are undone when they go away.

use std::collections::BTreeMap;

use super::census::census_by_file;
use super::sidecar::{
	SessionsGuard, build, output_text, plugin_config, set_repo_sidecar_path, write_sidecar_plugin_fixture,
};
use super::sidecar_declaration::sidecar_declaration_config;
use super::{Temp, large_stack, write_project};
use crate::census::{FileOutcome, compile_project_diagnostics};
use crate::compile::compile_file_detailed_with_options;
use crate::project::ProjectOptions;
use crate::sidecar::SidecarSession;

/// `writeOverlayPluginProject`: `prefixStringPlugin` over a src/main.ts.
fn write_overlay_plugin_project(pkg_name: &str, disk_text: &str) -> (Temp, SessionsGuard) {
	set_repo_sidecar_path();
	let sessions = SessionsGuard::new();
	let temp = write_project(pkg_name, "");
	write_sidecar_plugin_fixture(
		&temp.dir(),
		"",
		&plugin_config(
			"[\n\t\t\t{\n\t\t\t\t\"transform\": \"./plugins/prefix-string.js\",\n\t\t\t\t\"prefix\": \"plugin\"\n\t\t\t}\n\t\t]",
		),
	);
	temp.write("src/main.ts", disk_text);
	(temp, sessions)
}

/// `writeOverlaySourceFile`: the slash-form name a program reports, and the native path a
/// request carries.
fn write_overlay_source_file(temp: &Temp, text: &str) -> (String, String) {
	temp.write("main.ts", text);
	let native = temp.join("main.ts").to_string_lossy().into_owned();
	(rotor_support::gopath::to_slash(&native).into_owned(), native)
}

fn overlays(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
	entries
		.iter()
		.map(|(key, text)| ((*key).to_owned(), (*text).to_owned()))
		.collect()
}

#[test]
fn changed_files_for_ships_overlays_on_a_fresh_session() {
	// Given a fresh session and an overlay for its only file
	let temp = Temp::new();
	let (file_name, native_path) = write_overlay_source_file(&temp, "disk\n");
	let mut session = SidecarSession::default();
	// When the round trip's changed files are collected
	let changed = session
		.changed_files_for(&[file_name], &overlays(&[(&native_path, "overlay\n")]))
		.unwrap_or_else(|error| panic!("changedFilesFor: {error}"));
	// Then the overlay ships anyway.
	assert_eq!(changed.len(), 1, "changed = {changed:?}, want the overlay");
	assert_eq!(changed[0].text, "overlay\n", "text, want the overlay's");
	assert_eq!(changed[0].file_name, native_path, "fileName");
}

#[test]
fn changed_files_for_leaves_unoverlaid_files_to_the_worker() {
	// Given a fresh session and no overlays
	let temp = Temp::new();
	let (file_name, _) = write_overlay_source_file(&temp, "disk\n");
	let mut session = SidecarSession::default();
	// When the round trip's changed files are collected, then nothing ships
	let changed = session
		.changed_files_for(&[file_name], &BTreeMap::new())
		.unwrap_or_else(|error| panic!("changedFilesFor: {error}"));
	assert!(
		changed.is_empty(),
		"changed = {changed:?}, want nothing on a fresh session"
	);
}

#[test]
fn changed_files_for_ships_overlays_outside_the_stamped_set() {
	// Given an overlay for a file the stamp list does not carry.
	let temp = Temp::new();
	let (stamped, _) = write_overlay_source_file(&temp, "disk\n");
	temp.write("types.d.ts", "declare const disk: number;\n");
	let ambient = temp.join("types.d.ts").to_string_lossy().into_owned();
	let mut session = SidecarSession::default();
	// When the round trip's changed files are collected
	let changed = session
		.changed_files_for(
			&[stamped],
			&overlays(&[(&ambient, "declare const overlaid: number;\n")]),
		)
		.unwrap_or_else(|error| panic!("changedFilesFor: {error}"));
	// Then it ships anyway, under the caller's spelling.
	assert_eq!(changed.len(), 1, "changed = {changed:?}, want the unstamped overlay");
	assert_eq!(changed[0].file_name, ambient, "fileName");
	assert_eq!(
		changed[0].text, "declare const overlaid: number;\n",
		"text, want the overlay's"
	);
}

#[test]
fn changed_files_for_reverts_an_overlay_that_went_away() {
	// Given a session that shipped an overlay on an earlier round trip
	let temp = Temp::new();
	let (file_name, native_path) = write_overlay_source_file(&temp, "disk\n");
	let mut session = SidecarSession::default();
	session
		.changed_files_for(
			std::slice::from_ref(&file_name),
			&overlays(&[(&native_path, "overlay\n")]),
		)
		.unwrap_or_else(|error| panic!("first round trip: {error}"));
	// When the next round trip has no overlay for it, then the disk text ships
	let changed = session
		.changed_files_for(std::slice::from_ref(&file_name), &BTreeMap::new())
		.unwrap_or_else(|error| panic!("second round trip: {error}"));
	assert!(
		changed.len() == 1 && changed[0].text == "disk\n",
		"changed = {changed:?}, want the disk text resent"
	);
	// And once reverted it stays quiet
	let changed = session
		.changed_files_for(&[file_name], &BTreeMap::new())
		.unwrap_or_else(|error| panic!("third round trip: {error}"));
	assert!(
		changed.is_empty(),
		"changed = {changed:?}, want nothing once the revert landed"
	);
}

/// The product case: `sloptor diagnostics` over a real build tsconfig, plugins and all,
/// reporting on the caller's text.
#[test]
fn compile_project_diagnostics_censuses_a_plugin_project_under_overlays() {
	let (temp, _sessions) = write_overlay_plugin_project(
		"@scope/overlay-census-fixture",
		"import { label } from \"./helper\";\nexport const shout = label + \"!\";\n",
	);
	let dir = temp.dir();
	temp.write("src/helper.ts", "export const label = \"disk\";\n");
	let helper_path = temp.join("src/helper.ts").to_string_lossy().into_owned();
	let (control, error) = large_stack(|| compile_project_diagnostics(&dir, &ProjectOptions::default()));
	assert!(
		error.is_none(),
		"census of the unmodified tree: {error:?} ({:?})",
		control.diagnostics
	);
	for file in &control.files {
		assert_eq!(
			file.outcome,
			FileOutcome::Ok,
			"unmodified tree is not clean: {}",
			file.file_name
		);
	}
	// When the census runs with that overlay
	let opts = ProjectOptions {
		overlays: overlays(&[(&helper_path, "export const gone: number = \"not a number\";\n")]),
		..ProjectOptions::default()
	};
	let (census, error) = large_stack(|| compile_project_diagnostics(&dir, &opts));
	assert!(
		error.is_none(),
		"CompileProjectDiagnostics: {error:?} ({:?})",
		census.diagnostics
	);
	// Then the overlay is counted, its own type error is reported against it, and the file
	// that imported what it removed is reported too
	assert_eq!(census.overlay_matches, 1, "OverlayMatches");
	let by_file = census_by_file(&census);
	assert_eq!(
		by_file.get("helper.ts").map(|file| file.outcome),
		Some(FileOutcome::TypeError),
		"helper.ts outcome"
	);
	assert_eq!(
		by_file.get("main.ts").map(|file| file.outcome),
		Some(FileOutcome::TypeError),
		"main.ts outcome — the overlay's removed export did not propagate"
	);
}

/// Compiles main.ts with the helper overlaid to a string through `overlay_path`.
fn compile_main_with_helper_overlay(dir: &str, overlay_path: &str) -> String {
	let opts = ProjectOptions {
		overlays: overlays(&[(overlay_path, "export const label = \"text\";\n")]),
		..ProjectOptions::default()
	};
	let output =
		large_stack(|| compile_file_detailed_with_options(dir, "src/main.ts", &opts)).unwrap_or_else(|failure| {
			panic!(
				"CompileFileDetailedWithOptions: {} (diags: {:?})",
				failure.error,
				failure.messages()
			)
		});
	String::from_utf8_lossy(&output.text).into_owned()
}

#[test]
fn compile_file_keeps_overlays_on_files_the_sidecar_did_not_transform() {
	let (temp, _sessions) = write_overlay_plugin_project(
		"@scope/overlay-partial-fixture",
		"import { label } from \"./helper\";\nexport const tag = \"main\";\nexport const joined = label + label;\n",
	);
	temp.write("src/helper.ts", "export const label = 1;\n");
	let helper_path = temp.join("src/helper.ts").to_string_lossy().into_owned();
	let got = compile_main_with_helper_overlay(&temp.dir(), &helper_path);
	assert!(
		got.contains("label .. label"),
		"helper.ts overlay was dropped from the rebuilt program:\n{got}"
	);
}

/// Catches a partial sidecar response dropping an imported helper overlay named through a
/// symlinked parent.
#[cfg(unix)]
#[test]
fn compile_file_keeps_symlinked_helper_overlay_after_partial_sidecar_transform() {
	let (temp, _sessions) = write_overlay_plugin_project(
		"@scope/overlay-symlink-partial-fixture",
		"import { label } from \"./helper\";\nexport const tag = \"main\";\nexport const joined = label + label;\n",
	);
	let dir = temp.dir();
	temp.write("src/helper.ts", "export const label = 1;\n");
	let alias_temp = Temp::new();
	let alias = alias_temp.join("project").to_string_lossy().into_owned();
	std::os::unix::fs::symlink(rotor_support::gopath::dir(&dir), &alias).expect("symlink");
	let overlay_path = rotor_support::gopath::join(&[&alias, &rotor_support::gopath::base(&dir), "src", "helper.ts"]);
	let got = compile_main_with_helper_overlay(&dir, &overlay_path);
	assert!(
		got.contains("label .. label"),
		"symlinked helper overlay was dropped from the rebuilt program:\n{got}"
	);
}

#[test]
fn build_project_overlays_reach_declaration_path_rewriting() {
	set_repo_sidecar_path();
	let _sessions = SessionsGuard::new();
	let temp = write_project("@scope/overlay-declaration-fixture", "");
	let dir = temp.dir();
	write_sidecar_plugin_fixture(
		&dir,
		"",
		&sidecar_declaration_config("[]").replacen(
			r#""outDir": "out","#,
			r#""outDir": "out", "baseUrl": ".", "paths": { "@alias/*": ["src/*"] },"#,
			1,
		),
	);
	temp.write("src/value.ts", "export interface Value { label: string; }\n");
	temp.write("src/main.ts", "export type Output = string;\n");
	let main_path = temp.join("src/main.ts").to_string_lossy().into_owned();
	let opts = ProjectOptions {
		overlays: overlays(&[(
			&main_path,
			"import type { Value } from \"@alias/value\";\nexport type Output = Value;\n",
		)]),
		..ProjectOptions::default()
	};
	let (_, diags, error) = build(&dir, &opts);
	assert!(error.is_none(), "BuildProjectWithOptions: {error:?} (diags: {diags:?})");
	let declaration = std::fs::read_to_string(temp.join("out/main.d.ts")).expect("the declaration");
	assert!(
		declaration.contains(r#"from "./value""#),
		"declaration did not come from the overlay:\n{declaration}"
	);
}

#[test]
fn build_project_overlays_reach_transformer_plugins() {
	let (temp, _sessions) =
		write_overlay_plugin_project("@scope/overlay-plugin-fixture", "export const phase = \"disk\";\n");
	let main_path = temp.join("src/main.ts").to_string_lossy().into_owned();
	let opts = ProjectOptions {
		overlays: overlays(&[(&main_path, "export const phase = \"overlaid\";\n")]),
		..ProjectOptions::default()
	};
	let (result, diags, error) = build(&temp.dir(), &opts);
	assert!(error.is_none(), "BuildProjectWithOptions: {error:?} (diags: {diags:?})");
	let got = output_text(&result.unwrap_or_default(), "out/main.luau");
	assert!(
		got.contains(r#"local phase = "plugin:overlaid""#),
		"transformer did not see the overlay:\n{got}"
	);
	assert!(!got.contains("disk"), "disk text survived the overlay:\n{got}");
}
