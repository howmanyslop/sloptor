//! `runtime_config_test.go`

use std::collections::BTreeMap;

use super::{join, temp_dir};
use crate::ErrorKind;
use crate::runtime_config::{
	RUNTIME_CONFIG_FILE_NAME, RuntimeConfig, load_runtime_config, marshal_runtime_config_artifact,
};

#[test]
fn load_runtime_config_distinguishes_absent_and_empty() {
	let (_dir, root) = temp_dir();
	let (_, present) = load_runtime_config(&root).unwrap();
	assert!(!present);
	std::fs::write(join(&[&root, RUNTIME_CONFIG_FILE_NAME]), "{}").unwrap();
	let (config, present) = load_runtime_config(&root).unwrap();
	assert!(present);
	assert_eq!(config.marshal_json(), "{}");
}

#[test]
fn load_runtime_config_preserves_explicit_zero_and_schema_additions() {
	let (_dir, root) = temp_dir();
	let input = r#"{"logLevel":"none","profiling":false,"disableDependencyWarnings":false,"salt":"fixed","future":{"value":1}}"#;
	std::fs::write(join(&[&root, RUNTIME_CONFIG_FILE_NAME]), input).unwrap();
	let (config, present) = load_runtime_config(&root).unwrap();
	assert!(present && config.profiling == Some(false) && config.disable_dependency_warnings == Some(false));
	let want = r#"{"disableDependencyWarnings":false,"future":{"value":1},"logLevel":"none","profiling":false,"salt":"fixed"}"#;
	assert_eq!(config.marshal_json(), want);
}

fn load_error(input: &str) -> crate::Error {
	let (_dir, root) = temp_dir();
	std::fs::write(join(&[&root, RUNTIME_CONFIG_FILE_NAME]), input).unwrap();
	load_runtime_config(&root).unwrap_err()
}

#[test]
fn load_runtime_config_rejects_invalid_known_option() {
	assert!(load_error(r#"{"logLevel":"debug"}"#).is(ErrorKind::InvalidRuntimeConfig));
}

#[test]
fn load_runtime_config_rejects_null_boolean() {
	assert!(load_error(r#"{"profiling":null}"#).is(ErrorKind::InvalidRuntimeConfig));
}

#[test]
fn load_runtime_config_rejects_trailing_json_value() {
	assert!(load_error("{} {}").is(ErrorKind::InvalidRuntimeConfig));
}

#[test]
fn marshal_runtime_config_artifact_is_deterministic_and_omits_absent_game() {
	let packages: BTreeMap<String, RuntimeConfig> = [
		(
			"pkg-b".to_owned(),
			RuntimeConfig {
				profiling: Some(true),
				..RuntimeConfig::default()
			},
		),
		("pkg-a".to_owned(), RuntimeConfig::default()),
	]
	.into();
	let got = marshal_runtime_config_artifact(None, &packages).expect("emit");
	assert_eq!(
		String::from_utf8(got).unwrap(),
		r#"{"packages":{"pkg-a":{},"pkg-b":{"profiling":true}}}"#
	);
}
