//! Port of `internal/config/schema_test.go`.

use std::fs;

use rotor_support::gopath;

use super::super::{CONFIG_FILE_NAME, LoadError, SCHEMA, SCHEMA_DIRECTIVE, SCHEMA_URL, load};

fn temp_dir() -> (tempfile::TempDir, String) {
	let dir = tempfile::tempdir().expect("temp dir");
	let path = dir.path().to_str().expect("UTF-8").to_owned();
	(dir, path)
}

/// Writes `content` to `dir/name`, creating parent dirs.
fn write_config_file(dir: &str, name: &str, content: &str) {
	let path = gopath::join(&[dir, &gopath::from_slash(name)]);
	fs::create_dir_all(gopath::dir(&path)).expect("mkdir");
	fs::write(path, content).expect("write");
}

#[test]
fn schema_only_advertises_compiler_settings() {
	let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("schema is JSON");
	let properties = schema["properties"].as_object().expect("properties");
	assert!(
		schema["additionalProperties"] == false
			&& properties.len() == 4
			&& properties.contains_key("flamework")
			&& properties.contains_key("parallelTransformers")
			&& properties.contains_key("nativeTypes")
			&& properties.contains_key("luauTypes"),
		"unexpected compiler schema: {SCHEMA}"
	);
	assert_eq!(SCHEMA_DIRECTIVE, format!("#:schema {SCHEMA_URL}"));
}

/// `nativeTypes` is optional: unset must stay distinguishable from false so
/// the tsconfig declaration can win over `rotor.toml`.
#[test]
fn load_native_types() {
	let (_guard, dir) = temp_dir();
	let cfg = load(&dir);
	assert_eq!(cfg, Err(LoadError::NotFound), "load without rotor.toml = {cfg:?}");

	write_config_file(&dir, CONFIG_FILE_NAME, "");
	let cfg = load(&dir).expect("load");
	assert_eq!(cfg.native_types, None, "unset nativeTypes");

	write_config_file(&dir, CONFIG_FILE_NAME, "nativeTypes = false\n");
	let cfg = load(&dir).expect("load");
	assert_eq!(cfg.native_types, Some(false), "nativeTypes, want an explicit false");

	write_config_file(&dir, CONFIG_FILE_NAME, "nativeTypes = true\n");
	let cfg = load(&dir).expect("load");
	assert_eq!(cfg.native_types, Some(true), "nativeTypes");
	assert!(cfg.warnings.is_empty(), "nativeTypes must not warn: {:?}", cfg.warnings);
}

/// `luauTypes` is optional too: unset must stay distinguishable from false so
/// the command line and the tsconfig declaration can win over `rotor.toml`.
#[test]
fn load_luau_types() {
	let (_guard, dir) = temp_dir();
	write_config_file(&dir, CONFIG_FILE_NAME, "");
	let cfg = load(&dir).expect("load");
	assert_eq!(cfg.luau_types, None, "unset luauTypes");

	write_config_file(&dir, CONFIG_FILE_NAME, "luauTypes = false\n");
	let cfg = load(&dir).expect("load");
	assert_eq!(cfg.luau_types, Some(false), "luauTypes, want an explicit false");

	write_config_file(&dir, CONFIG_FILE_NAME, "luauTypes = true\nnativeTypes = true\n");
	let cfg = load(&dir).expect("load");
	assert_eq!(cfg.luau_types, Some(true), "luauTypes");
	assert_eq!(cfg.native_types, Some(true), "nativeTypes beside luauTypes");
	assert!(cfg.warnings.is_empty(), "luauTypes must not warn: {:?}", cfg.warnings);
}

#[test]
fn load_compiler_settings_and_warn_on_removed_sections() {
	let (_guard, dir) = temp_dir();
	write_config_file(
		&dir,
		CONFIG_FILE_NAME,
		"[flamework]\nidGenerationMode = 'full'\n[parallelTransformers]\nworkers = 2\n[assets]\nmode = 'macro'\n[deploy]\nunused = true\n",
	);
	let cfg = load(&dir).expect("load");
	assert!(
		cfg.flamework.is_some() && cfg.parallel_transformers.is_some() && !cfg.warnings.is_empty(),
		"load = {cfg:?}"
	);
	write_config_file(&dir, "rotor.config.js", "throw new Error('must never execute');");
	fs::remove_file(gopath::join(&[&dir, CONFIG_FILE_NAME])).expect("remove");
	assert_eq!(load(&dir), Err(LoadError::NotFound), "legacy executable config");
}

/// The schema is hosted (served from [`SCHEMA_URL`] via raw GitHub) rather
/// than written per-project, so a single `rotor.schema.json` is committed at
/// the repo root. It must never drift from [`SCHEMA`]; `rotor schema`
/// regenerates it.
#[test]
fn committed_schema_matches_constant() {
	let path = gopath::join(&[env!("CARGO_MANIFEST_DIR"), "..", "..", "rotor.schema.json"]);
	let data = fs::read_to_string(&path).unwrap_or_else(|err| {
		panic!("committed schema missing - run `rotor schema > rotor.schema.json` at the repo root: {err}")
	});
	assert_eq!(
		data, SCHEMA,
		"{path} is out of sync with SCHEMA; regenerate with `rotor schema > rotor.schema.json`"
	);
}
