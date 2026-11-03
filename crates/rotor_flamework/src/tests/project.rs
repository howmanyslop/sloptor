//! `project_test.go`

use rotor_config::config::FlameworkConfig;

use super::{join, temp_dir, write_project_fixture};
use crate::ErrorKind;
use crate::project::{IdMode, ProjectOptions, open_project};

fn options(root: &str, config: FlameworkConfig) -> ProjectOptions {
	ProjectOptions {
		project_dir: root.to_owned(),
		root_dir: join(&[root, "src"]),
		out_dir: join(&[root, "out"]),
		config,
		..ProjectOptions::default()
	}
}

#[test]
fn open_project_when_game_package_has_defaults() {
	let (_dir, root) = temp_dir();
	write_project_fixture(&root, r#"{"name":"fixture-game","version":"1.0.0"}"#);
	let project = open_project(&options(&root, FlameworkConfig::default())).unwrap();
	assert_eq!(project.package_name(), "fixture-game");
	assert!(project.is_game());
	assert_eq!(project.include_directory(), join(&[&root, "include"]));
	assert_eq!(project.id_mode(), &IdMode::Full);
	assert_eq!(project.hash_prefix(), "");
}

#[test]
fn open_project_when_scoped_package_uses_identity_prefix() {
	let (_dir, root) = temp_dir();
	write_project_fixture(&root, r#"{"name":"@scope/library","version":"2.0.0"}"#);
	let project = open_project(&options(
		&root,
		FlameworkConfig {
			obfuscation: true,
			..FlameworkConfig::default()
		},
	))
	.unwrap();
	assert!(!project.is_game());
	assert_eq!(project.hash_prefix(), "@scope/library");
	assert_eq!(project.id_mode(), &IdMode::Obfuscated);
}

#[test]
fn open_project_when_package_json_is_malformed() {
	let (_dir, root) = temp_dir();
	write_project_fixture(&root, r#"{"name":"#);
	let error = open_project(&options(&root, FlameworkConfig::default())).unwrap_err();
	assert!(error.is(ErrorKind::InvalidPackage), "{error}");
}
