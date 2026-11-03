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

fn drain_coordinator(coordinator: &mut crate::solution::SolutionCoordinator, what: &str) {
	let (_, diags, error) = large_stack(|| coordinator.drain());
	if let Some(error) = error {
		panic!("{what}: {error} ({diags:?})");
	}
}

fn new_coordinator(config: &str) -> crate::solution::SolutionCoordinator {
	large_stack(|| crate::solution::SolutionCoordinator::new(config, &ProjectOptions::default()))
		.unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn package_watch_retains_missing_explicit_project() {
	use crate::solution_watch::{canonical_watch_directory, canonical_watch_paths};
	let temp = write_package_layout_fixture();
	let dir = temp.dir();
	temp.write(
		"node_modules/@rbxts/lib/default.project.json",
		r#"{"name":"lib","tree":{"Mounted":{"$path":"layout.project.json"}}}"#,
	);
	let named = temp
		.join("node_modules/@rbxts/lib/layout.project.json")
		.to_string_lossy()
		.into_owned();
	temp.write(
		"node_modules/@rbxts/lib/layout.project.json",
		r#"{"name":"ignored","tree":{"$path":"out"}}"#,
	);
	let mut coordinator = new_coordinator(&rotor_support::gopath::join(&[&dir, "tsconfig.json"]));
	drain_coordinator(&mut coordinator, "initial drain");
	let config = coordinator.watch_sets()[0].project_path.clone();
	fs::remove_file(&named).expect("remove");
	assert!(
		coordinator.refresh_package_watch_inputs(&config),
		"removing explicit composition did not change the layout"
	);
	let inputs = coordinator.watch_sets()[0].package_inputs.clone();
	assert!(
		inputs
			.as_ref()
			.is_some_and(|inputs| canonical_watch_paths(&inputs.files).contains(&canonical_watch_directory(&named))),
		"missing explicit project lost its watch: {inputs:?}"
	);
	temp.write(
		"node_modules/@rbxts/lib/layout.project.json",
		r#"{"name":"ignored","tree":{"$path":"out"}}"#,
	);
	assert!(
		coordinator.refresh_package_watch_inputs(&config),
		"recreating explicit composition did not change the layout"
	);
}

#[test]
fn package_watch_explicit_junction_retarget() {
	use crate::solution_watch::{canonical_watch_directory, canonical_watch_paths};
	for suffix in ["", "/shared.project.json", "/sub/deeper/shared.project.json"] {
		let temp = write_package_layout_fixture();
		let dir = temp.dir();
		temp.write(
			"node_modules/@rbxts/lib/default.project.json",
			format!(r#"{{"name":"lib","tree":{{"Mounted":{{"$path":"../../../links/runtime{suffix}"}}}}}}"#),
		);
		for name in ["One", "Two"] {
			let body =
				format!(r#"{{"name":"ignored","tree":{{"{name}":{{"$path":"../../node_modules/@rbxts/lib/out"}}}}}}"#);
			temp.write(&format!("targets/{name}/default.project.json"), &body);
			temp.write(&format!("targets/{name}/shared.project.json"), &body);
			temp.write(
				&format!("targets/{name}/sub/deeper/shared.project.json"),
				body.replacen("../../node_modules", "../../../../node_modules", 1),
			);
		}
		let alias = temp.join("links/runtime");
		fs::create_dir_all(alias.parent().expect("a parent")).expect("mkdir");
		let link = |name: &str| {
			#[cfg(unix)]
			std::os::unix::fs::symlink(temp.join(&format!("targets/{name}")), &alias).expect("symlink");
			#[cfg(windows)]
			std::os::windows::fs::symlink_dir(temp.join(&format!("targets/{name}")), &alias).expect("symlink");
		};
		link("One");
		let mut coordinator = new_coordinator(&rotor_support::gopath::join(&[&dir, "tsconfig.json"]));
		drain_coordinator(&mut coordinator, "initial drain");
		let set = coordinator.watch_sets()[0].clone();
		let parent = canonical_watch_directory(&temp.join("links").to_string_lossy());
		assert!(
			set.package_inputs
				.as_ref()
				.is_some_and(|inputs| canonical_watch_paths(&inputs.directories).contains(&parent)),
			"{suffix}: junction parent is not watched: {:?}",
			set.package_inputs
		);
		fs::remove_file(&alias).expect("remove");
		link("Two");
		assert!(
			coordinator.refresh_package_watch_inputs(&set.project_path),
			"{suffix}: junction retarget did not change the layout"
		);
		coordinator.invalidate(std::slice::from_ref(&set.project_path));
		drain_coordinator(&mut coordinator, "retargeted drain");
		let got = fs::read_to_string(temp.join("out/main.luau")).expect("read");
		assert!(
			got.contains(".Mounted.Two"),
			"{suffix}: retargeted layout is stale:\n{got}"
		);
	}
}
