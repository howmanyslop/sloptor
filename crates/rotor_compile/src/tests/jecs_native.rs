//! `jecs_native_test.go`

use super::Temp;
use crate::jecs_native::{
	NATIVE_JECS_TRANSFORM, hash_jecs_transformer_sources, jecs_source_hash_cache, native_jecs_stage,
	register_native_jecs_source_hash,
};
use crate::project::lock;
use crate::transformer_order::TransformerPluginConfig;

/// `jecsTestPlugin`
fn jecs_test_plugin(raw: &str) -> TransformerPluginConfig {
	let fields: serde_json::Value = serde_json::from_str(raw).expect("plugin JSON");
	TransformerPluginConfig {
		transform: fields
			.get("transform")
			.and_then(serde_json::Value::as_str)
			.unwrap_or_default()
			.to_owned(),
		raw: raw.to_owned(),
	}
}

/// `installFakeJecs`: a transformer package under `dir/node_modules`, its hash registered as a
/// ported build. Go removes the hash when the test ends; a hash of a temporary package matches
/// nothing else, so the port leaves it.
fn install_fake_jecs(temp: &Temp) -> String {
	let source_dir = temp.join(&format!("node_modules/{NATIVE_JECS_TRANSFORM}/out/src"));
	temp.write(
		&format!("node_modules/{NATIVE_JECS_TRANSFORM}/out/src/index.js"),
		"export default () => () => (file) => file;\n",
	);
	let source_dir = source_dir.to_string_lossy().into_owned();
	let hash = hash_jecs_transformer_sources(&source_dir).expect("hash");
	register_native_jecs_source_hash(&hash);
	source_dir
}

/// Catches jecs being run natively ahead of another transformer that the sidecar would have run
/// before it, which changes the input both of them see. The sidecar runs every `before`
/// transformer in list order and then every `after` one, so jecs is first exactly when no
/// `before` entry precedes it.
#[test]
fn native_jecs_stage_keeps_transformer_order() {
	let temp = Temp::new();
	install_fake_jecs(&temp);
	let jecs_entry = r#"{"transform":"rbxts-transformer-jecs","silent":true}"#;
	let after_entry = r#"{"transform":"applier","after":true}"#;
	let before_entry = r#"{"transform":"other"}"#;

	let cases: [(&str, Vec<&str>, bool, Vec<&str>); 5] = [
		("alone", vec![jecs_entry], true, vec![]),
		(
			"before an after transformer",
			vec![jecs_entry, after_entry],
			true,
			vec!["applier"],
		),
		(
			"listed after an after transformer",
			vec![after_entry, jecs_entry],
			true,
			vec!["applier"],
		),
		(
			"behind another before transformer",
			vec![before_entry, jecs_entry],
			false,
			vec![],
		),
		(
			"with an option the port does not know",
			vec![r#"{"transform":"rbxts-transformer-jecs","resolutionHost":{}}"#],
			false,
			vec![],
		),
	];
	for (name, raw_plugins, native, remaining) in cases {
		let plugins: Vec<TransformerPluginConfig> = raw_plugins.iter().map(|raw| jecs_test_plugin(raw)).collect();
		let (config, rest) = native_jecs_stage(&temp.dir(), &plugins);
		assert_eq!(
			config.is_some(),
			native,
			"{name}: native = {}, want {native}",
			config.is_some()
		);
		if !native {
			assert_eq!(
				rest.len(),
				plugins.len(),
				"{name}: declined stage dropped plugins: {} of {} left",
				rest.len(),
				plugins.len()
			);
			continue;
		}
		let names: Vec<&str> = rest.iter().map(|plugin| plugin.transform.as_str()).collect();
		assert!(
			names.len() == remaining.len() && (names.is_empty() || names[0] == remaining[0]),
			"{name}: remaining = {names:?}, want {remaining:?}"
		);
	}
}

/// Catches a re-patched or upgraded transformer running through a port written for a different
/// build of it.
#[test]
fn native_jecs_stage_declines_an_unported_build() {
	let temp = Temp::new();
	let source_dir = install_fake_jecs(&temp);
	std::fs::write(format!("{source_dir}/util.js"), "export const changed = true;\n").expect("write util.js");
	// Go clears the whole cache; another test's package directory never collides with this one.
	lock(jecs_source_hash_cache()).retain(|directory, _| !directory.starts_with(&temp.dir()));
	let (config, _) = native_jecs_stage(
		&temp.dir(),
		&[jecs_test_plugin(r#"{"transform":"rbxts-transformer-jecs"}"#)],
	);
	assert!(
		config.is_none(),
		"ran natively against a transformer build the port does not match"
	);
}
