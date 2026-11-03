//! `project_artifacts_test.go`

use super::{join, temp_dir, write_project_fixture};
use crate::project::{ProjectOptions, open_project};

fn assert_file_text(path: &str, want: &str) {
	assert_eq!(std::fs::read_to_string(path).unwrap(), want, "{path}");
}

#[test]
fn project_persist_when_runtime_config_and_glob_are_present() {
	let (_dir, root) = temp_dir();
	write_project_fixture(&root, r#"{"name":"fixture-game","version":"1.0.0"}"#);
	std::fs::write(join(&[&root, "flamework.json"]), r#"{"profiling":false}"#).unwrap();
	std::fs::write(join(&[&root, "src", "main.ts"]), "export {}").unwrap();
	std::fs::write(
		join(&[&root, "default.project.json"]),
		r#"{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"}}}}"#,
	)
	.unwrap();
	let mut project = open_project(&ProjectOptions {
		project_dir: root.clone(),
		root_dir: join(&[&root, "src"]),
		out_dir: join(&[&root, "out"]),
		..ProjectOptions::default()
	})
	.unwrap();
	project.add_glob("src/**/*.ts", "src/main.ts");
	project.persist().unwrap();
	assert_file_text(
		&join(&[&root, "include", "flamework", "config.json"]),
		r#"{"game":{"profiling":false},"packages":{}}"#,
	);
	assert_file_text(
		&join(&[&root, "include", "flamework", "globs.json"]),
		r#"{"game":{"src/**/*.ts":[["ReplicatedStorage","TS","main"]]},"packages":{}}"#,
	);
	let build = std::fs::read_to_string(join(&[&root, "flamework.build"])).unwrap();
	for fragment in [
		r#""flameworkVersion": "1.3.2""#,
		r#""src/**/*.ts": ["#,
		r#""out/main.lua""#,
	] {
		assert!(build.contains(fragment), "flamework.build missing {fragment}:\n{build}");
	}
}

#[test]
fn project_persist_when_project_is_package() {
	let (_dir, root) = temp_dir();
	write_project_fixture(&root, r#"{"name":"@scope/package","version":"1.0.0"}"#);
	std::fs::write(join(&[&root, "flamework.json"]), r#"{"profiling":true}"#).unwrap();
	let mut project = open_project(&ProjectOptions {
		project_dir: root.clone(),
		root_dir: join(&[&root, "src"]),
		out_dir: join(&[&root, "out"]),
		..ProjectOptions::default()
	})
	.unwrap();
	project.persist().unwrap();
	assert!(std::path::Path::new(&join(&[&root, "flamework.build"])).exists());
	assert!(!std::path::Path::new(&join(&[&root, "include"])).exists());
}
