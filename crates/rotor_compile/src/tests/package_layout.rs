//! `package_layout_test.go`. The two watch tests drive the solution coordinator (5b).

use std::fs;
use std::sync::Arc;

use rotor_transformer::ProjectType;

use super::output::{build, enable_incremental_builds};
use super::{Temp, expect_compiled, large_stack, output, write_project};
use crate::ProjectOptions;
use crate::project::{compile_project, compile_project_with_options};
use crate::timings::BuildTimings;

/// `writePackageLayoutFixture`
fn write_package_layout_fixture() -> Temp {
	let temp = write_project("@scope/app", "");
	temp.write("src/main.ts", r#"export { answer } from "@rbxts/lib";"#);
	temp.write(
		"node_modules/@rbxts/lib/package.json",
		r#"{"name":"@rbxts/lib","main":"out/init.luau","types":"out/index.d.ts"}"#,
	);
	temp.write(
		"node_modules/@rbxts/lib/out/index.d.ts",
		"export declare const answer: number;",
	);
	temp.write("node_modules/@rbxts/lib/out/init.luau", "return { answer = 42 }");
	temp
}

#[test]
fn package_incremental_layout_changes() {
	let temp = write_package_layout_fixture();
	enable_incremental_builds(&temp);
	let dir = temp.dir();
	let check = |want_selected: bool| {
		let timings = Arc::new(BuildTimings::new());
		build(
			&dir,
			&ProjectOptions {
				timings: Some(Arc::clone(&timings)),
				..ProjectOptions::default()
			},
		);
		let selected = timings.report().counts.selected_sources;
		assert_eq!(
			selected > 0,
			want_selected,
			"selected {selected} sources, want changed={want_selected}"
		);
		let clean = expect_compiled(large_stack(|| compile_project(&dir)));
		let got = fs::read_to_string(temp.join("out/main.luau")).expect("out/main.luau reads");
		let clean_main = output(&clean, "out/main.luau");
		assert_eq!(got, clean_main, "incremental output differs from clean");
	};
	check(true);
	check(false);
	temp.write(
		"node_modules/@rbxts/lib/.cache/place/clean.project.json",
		r#"{"name":"test","tree":{"$path":"../../out"}}"#,
	);
	check(false);
	temp.write(
		"node_modules/@rbxts/lib/.cache/place/clean.project.json",
		r#"{"name":"renamed-test","tree":{"$path":"../../out"}}"#,
	);
	check(false);
	fs::remove_file(temp.join("node_modules/@rbxts/lib/.cache/place/clean.project.json")).expect("remove");
	check(false);
	let config = temp.join("node_modules/@rbxts/lib/default.project.json");
	temp.write(
		"node_modules/@rbxts/lib/default.project.json",
		r#"{"name":"lib","tree":{"$path":"out"}}"#,
	);
	check(true);
	let modified = fs::metadata(&config)
		.and_then(|meta| meta.modified())
		.expect("the config's mtime");
	temp.write(
		"node_modules/@rbxts/lib/default.project.json",
		r#"{"name":"renamed","tree":{"$path":"out"}}"#,
	);
	super::output::set_mtime(&config, modified);
	check(true);
	fs::remove_file(&config).expect("remove");
	check(true);
	temp.write("node_modules/@rbxts/lib/runtime/init.luau", "return { answer = 42 }");
	temp.write(
		"node_modules/@rbxts/lib/package.json",
		r#"{"name":"@rbxts/lib","main":"runtime/init.luau","types":"out/index.d.ts"}"#,
	);
	check(true);
	check(false);
}

#[test]
fn package_imports_ignore_discovered_named_projects() {
	for folder in [".cache/place", "tests/place", "node_modules/nested", "."] {
		let temp = write_package_layout_fixture();
		let mount = if folder == "." { "out" } else { "../../out" };
		temp.write(
			&format!("node_modules/@rbxts/lib/{folder}/clean.project.json"),
			format!(
				r#"{{"name":"lib-testing","tree":{{"$className":"DataModel","ReplicatedStorage":{{"lib":{{"out":{{"$path":"{mount}"}}}}}}}}}}"#
			),
		);
		let outputs = expect_compiled(large_stack(|| compile_project(&temp.dir())));
		let want = r#"TS.import(script, TS.getModule(script, "@rbxts", "lib").out).answer"#;
		let got = output(&outputs, "out/main.luau");
		assert!(
			got.contains(want),
			"{folder}: package import should retain its output path:\n{got}"
		);
	}
}

#[test]
fn package_imports_ignore_named_root_remap() {
	let temp = write_package_layout_fixture();
	temp.write(
		"node_modules/@rbxts/lib/testing.project.json",
		r#"{"name":"testing","tree":{"$path":"testing-output"}}"#,
	);
	let outputs = expect_compiled(large_stack(|| compile_project(&temp.dir())));
	let got = output(&outputs, "out/main.luau");
	assert!(
		got.contains(r#"TS.getModule(script, "@rbxts", "lib").out"#),
		"named root project remapped the runtime entry:\n{got}"
	);
}

#[cfg(unix)]
#[test]
fn package_imports_pnpm_projects() {
	let temp = super::pnpm::write_pnpm_fixture();
	temp.write(
		"node_modules/@rbxts/dummy/.cache/place/clean.project.json",
		r#"{"name":"testing","tree":{"$path":"../../out"}}"#,
	);
	let opts = ProjectOptions {
		project_type: Some(ProjectType::Package),
		..ProjectOptions::default()
	};
	let outputs = expect_compiled(large_stack(|| compile_project_with_options(&temp.dir(), &opts)));
	let got = output(&outputs, "out/main.luau");
	for want in [
		r#"TS.getModule(script, "@rbxts", "dummy").out"#,
		r#"TS.getModule(script, "@rbxts", "linked")"#,
	] {
		assert!(got.contains(want), "missing {want} in pnpm package output:\n{got}");
	}
}

#[test]
#[ignore = "needs 5b: the solution coordinator's package watch inputs"]
fn package_watch_retains_missing_explicit_project() {}

#[test]
#[ignore = "needs 5b: the solution coordinator's package watch inputs"]
fn package_watch_explicit_junction_retarget() {}
