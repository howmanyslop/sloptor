//! `state_test.go`

use rotor_config::config::FlameworkConfig;

use super::{join, temp_dir, write_project_fixture};
use crate::project::{ProjectOptions, open_project};
use crate::state::DeclarationIdentity;

fn options(root: &str, config: FlameworkConfig) -> ProjectOptions {
	ProjectOptions {
		project_dir: root.to_owned(),
		root_dir: join(&[root, "src"]),
		out_dir: join(&[root, "out"]),
		config,
		..ProjectOptions::default()
	}
}

fn declaration(internal_id: &str, name: &str, lua: &str) -> DeclarationIdentity {
	DeclarationIdentity {
		internal_id: internal_id.to_owned(),
		declaration_name: name.to_owned(),
		lua_file_name: lua.to_owned(),
		..DeclarationIdentity::default()
	}
}

#[test]
fn project_identifier_when_generated_twice() {
	let (_dir, root) = temp_dir();
	write_project_fixture(&root, r#"{"name":"fixture-game","version":"1.0.0"}"#);
	let mut project = open_project(&options(
		&root,
		FlameworkConfig {
			obfuscation: true,
			id_generation_mode: "obfuscated".to_owned(),
			salt: "salt".to_owned(),
			..FlameworkConfig::default()
		},
	))
	.unwrap();
	let declaration = declaration("fixture-game:out/services/Foo@Bar", "Bar", "foo");
	let first = project.identifier(&declaration).unwrap();
	let second = project.identifier(&declaration).unwrap();
	assert_eq!((first.as_str(), second.as_str()), ("XG", "XG"));
	let snapshot = project.build_info_snapshot();
	assert_eq!(snapshot.identifiers[&declaration.internal_id], "XG");
	assert_eq!(snapshot.salt, None);
}

#[test]
fn project_identifier_when_full_mode_does_not_need_salt() {
	let (_dir, root) = temp_dir();
	write_project_fixture(&root, r#"{"name":"fixture-game","version":"1.0.0"}"#);
	let mut project = open_project(&options(&root, FlameworkConfig::default())).unwrap();
	project
		.identifier(&declaration("fixture-game:out/main@Main", "Main", "main"))
		.unwrap();
	assert_eq!(project.build_info().salt(), None);
}

#[test]
fn project_preload_identifiers_when_input_order_varies() {
	let (_dir, root) = temp_dir();
	write_project_fixture(&root, r#"{"name":"fixture-game","version":"1.0.0"}"#);
	let mut project = open_project(&options(
		&root,
		FlameworkConfig {
			id_generation_mode: "tiny".to_owned(),
			salt: "salt".to_owned(),
			preload_ids: true,
			..FlameworkConfig::default()
		},
	))
	.unwrap();
	let got = project
		.preload_identifiers(&[
			declaration("fixture-game:out/z@Z", "Z", "z"),
			declaration("fixture-game:out/a@A", "A", "a"),
		])
		.unwrap();
	let want = [
		("fixture-game:out/a@A".to_owned(), "A{XG}".to_owned()),
		("fixture-game:out/z@Z".to_owned(), "Z{dv}".to_owned()),
	]
	.into();
	assert_eq!(got, want);
}

#[test]
fn project_hash_string_when_context_repeats() {
	let (_dir, root) = temp_dir();
	write_project_fixture(&root, r#"{"name":"fixture-game","version":"1.0.0"}"#);
	let mut project = open_project(&options(&root, FlameworkConfig::default())).unwrap();
	let first = project.hash_string("message", "remotes").unwrap();
	let second = project.hash_string("message", "remotes").unwrap();
	assert_eq!(first, second);
	assert_eq!(
		project.build_info_snapshot().string_hashes.unwrap()["remotes:message"],
		first
	);
}
