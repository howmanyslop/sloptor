//! Port of `internal/config/flamework_test.go`.

use std::fs;

use rotor_support::gopath;

use super::super::{CONFIG_FILE_NAME, Config, FlameworkConfig, FlameworkOptimizations, SCHEMA, load};

/// Writes `content` to `dir/name`, creating parent dirs.
fn write_config_file(dir: &str, name: &str, content: &str) {
	let path = gopath::join(&[dir, &gopath::from_slash(name)]);
	fs::create_dir_all(gopath::dir(&path)).expect("mkdir");
	fs::write(path, content).expect("write");
}

fn temp_dir() -> (tempfile::TempDir, String) {
	let dir = tempfile::tempdir().expect("temp dir");
	let path = dir.path().to_str().expect("UTF-8").to_owned();
	(dir, path)
}

#[test]
fn load_flamework_config() {
	let (_guard, dir) = temp_dir();
	write_config_file(
		&dir,
		CONFIG_FILE_NAME,
		r#"[flamework]
after = "rbxts-transform-env"
noSemanticDiagnostics = true
obfuscation = true
hashPrefix = "game"
salt = "salt"
preloadIds = true
skipUnchangedFiles = true

[flamework.optimizations]
guardGenerationDedupLimit = 0
"#,
	);

	let cfg = load(&dir).expect("load");
	let flamework = cfg.flamework.expect("flamework section missing");
	assert_eq!(flamework.id_generation_mode, "obfuscated", "flamework.idGenerationMode");
	assert!(
		flamework.after == "rbxts-transform-env"
			&& flamework.no_semantic_diagnostics
			&& flamework.obfuscation
			&& flamework.hash_prefix == "game"
			&& flamework.salt == "salt"
			&& flamework.preload_ids
			&& flamework.skip_unchanged_files
			&& flamework.optimizations.guard_generation_dedup_limit == Some(0),
		"flamework = {flamework:?}"
	);
}

#[test]
fn load_flamework_guard_generation_dedup_limit_presence() {
	let cases = [
		(
			"explicit zero is present",
			"[flamework.optimizations]\nguardGenerationDedupLimit = 0\n",
			true,
		),
		("omitted is absent", "[flamework.optimizations]\n", false),
	];
	for (name, content, want_present) in cases {
		let (_guard, dir) = temp_dir();
		write_config_file(&dir, CONFIG_FILE_NAME, content);

		let cfg = load(&dir).expect("load");

		let limit = cfg
			.flamework
			.expect("flamework")
			.optimizations
			.guard_generation_dedup_limit;
		assert_eq!(
			limit.is_some(),
			want_present,
			"{name}: guardGenerationDedupLimit presence"
		);
		if want_present {
			assert_eq!(limit, Some(0), "{name}: guardGenerationDedupLimit");
		}
	}
}

fn schema_root() -> serde_json::Value {
	serde_json::from_str(SCHEMA).expect("schema is JSON")
}

#[test]
fn flamework_schema() {
	let root = schema_root();
	let flamework = &root["properties"]["flamework"];
	assert_eq!(
		flamework["additionalProperties"], false,
		"flamework.additionalProperties"
	);
	let flamework_properties = &flamework["properties"];
	assert_eq!(
		flamework_properties["idGenerationMode"]["default"], "full",
		"flamework.idGenerationMode.default"
	);
	assert_eq!(
		flamework_properties["optimizations"]["additionalProperties"], false,
		"flamework.optimizations.additionalProperties"
	);
	let profile = &flamework_properties["profiles"]["additionalProperties"];
	assert_eq!(
		profile["additionalProperties"], false,
		"flamework profile additionalProperties"
	);
	for section in [flamework_properties, &profile["properties"]] {
		let skip = &section["skipUnchangedFiles"];
		assert_eq!(
			skip["type"], "boolean",
			"skipUnchangedFiles schema = {skip}, want boolean type"
		);
	}
}

#[test]
fn flamework_schema_rejects_reserved_hash_prefix() {
	// Given
	let root = schema_root();
	let pattern = root["properties"]["flamework"]["properties"]["hashPrefix"]["pattern"]
		.as_str()
		.expect("flamework.hashPrefix must define a pattern that rejects the reserved $ prefix");

	// When
	let re = regex::Regex::new(pattern)
		.unwrap_or_else(|err| panic!("flamework.hashPrefix.pattern {pattern:?} is invalid: {err}"));

	// Then
	assert!(
		!re.is_match("$internal"),
		"flamework.hashPrefix.pattern {pattern:?} accepts a reserved prefix"
	);
	assert!(
		re.is_match(""),
		"flamework.hashPrefix.pattern {pattern:?} rejects the valid empty prefix"
	);
}

#[test]
fn load_empty_flamework_config_is_present_with_full_default() {
	let (_guard, dir) = temp_dir();
	write_config_file(&dir, CONFIG_FILE_NAME, "[flamework]\n");

	let cfg = load(&dir).expect("load");
	let flamework = cfg.flamework.expect("empty flamework section must remain present");
	assert_eq!(flamework.id_generation_mode, "full", "flamework.idGenerationMode");
}

#[test]
fn load_flamework_profiles_applies_defaults() {
	let (_guard, dir) = temp_dir();
	write_config_file(
		&dir,
		CONFIG_FILE_NAME,
		"[flamework.profiles.\"tsconfig.lib.json\"]\nafter = \"jest\"\nskipUnchangedFiles = true\n",
	);

	let cfg = load(&dir).expect("load");
	let profile = cfg
		.flamework
		.expect("flamework")
		.profiles
		.remove("tsconfig.lib.json")
		.expect("tsconfig.lib.json profile missing");
	assert!(
		profile.after == "jest" && profile.id_generation_mode == "full" && profile.skip_unchanged_files,
		"profile = {profile:?}, want after jest, full ID mode, skipUnchangedFiles"
	);
}

#[test]
fn validate_flamework_config() {
	let cases: [(&str, FlameworkConfig, &str); 5] = [
		(
			"invalid id mode",
			FlameworkConfig {
				id_generation_mode: "random".to_owned(),
				..FlameworkConfig::default()
			},
			"idGenerationMode",
		),
		(
			"reserved hash prefix",
			FlameworkConfig {
				hash_prefix: "$internal".to_owned(),
				..FlameworkConfig::default()
			},
			"hashPrefix",
		),
		(
			"negative guard dedup limit",
			FlameworkConfig {
				optimizations: FlameworkOptimizations {
					guard_generation_dedup_limit: Some(-1),
				},
				..FlameworkConfig::default()
			},
			"guardGenerationDedupLimit",
		),
		(
			"zero guard dedup limit",
			FlameworkConfig {
				optimizations: FlameworkOptimizations {
					guard_generation_dedup_limit: Some(0),
				},
				..FlameworkConfig::default()
			},
			"",
		),
		(
			"accepted modes",
			FlameworkConfig {
				id_generation_mode: "tiny".to_owned(),
				..FlameworkConfig::default()
			},
			"",
		),
	];
	for (name, flamework, want) in cases {
		let errs = Config {
			flamework: Some(flamework),
			..Config::default()
		}
		.validate();
		if want.is_empty() {
			assert!(errs.is_empty(), "{name}: validate() = {errs:?}, want clean");
			continue;
		}
		assert!(
			errs.len() == 1 && errs[0].to_string().contains(want),
			"{name}: validate() = {errs:?}, want one error containing {want:?}"
		);
	}
}

#[test]
fn load_flamework_rejects_unknown_keys() {
	for content in [
		"[flamework]\nenabled = true\n",
		"[flamework.optimizations]\nunknown = 1\n",
		"[flamework.profiles.\"tsconfig.lib.json\"]\nunknown = 1\n",
	] {
		let (_guard, dir) = temp_dir();
		write_config_file(&dir, CONFIG_FILE_NAME, content);
		let err = load(&dir).expect_err("unknown flamework key must fail");
		assert!(
			err.to_string().contains("flamework"),
			"load({content:?}) = {err}, want Flamework unknown-key error"
		);
	}
}
