//! `incremental_plugin_salt_test.go`: transformer plugin identity reaches the incremental salt.

use serde_json::{Value, json};

use super::types_options::luau_types_argv;
use super::{Temp, large_stack, set_compiler_options, write_project};
use crate::ProjectOptions;
use crate::incremental_plugin_salt::{TransformerPluginFingerprint, transformer_plugin_fingerprints};
use crate::incremental_salt::incremental_salt_with_flamework;
use crate::package_layout::hex_sha256;
use crate::project::new_project_program_with_options;

/// `writeFakeTransformerPackage`: a transformer under the project's `node_modules`.
fn write_fake_transformer_package(temp: &Temp, name: &str, manifest: &Value, entry_relative: &str, entry_text: &str) {
	temp.write(&format!("node_modules/{name}/{entry_relative}"), entry_text);
	temp.write(
		&format!("node_modules/{name}/package.json"),
		serde_json::to_vec(manifest).expect("the manifest encodes"),
	);
}

/// `writePluginTsconfig`
fn write_plugin_tsconfig(temp: &Temp, entries: &[Value]) {
	set_compiler_options(&temp.join("tsconfig.json"), &json!({ "plugins": entries }));
}

fn fingerprints(temp: &Temp) -> Vec<TransformerPluginFingerprint> {
	let config_path = temp.join("tsconfig.json").to_string_lossy().into_owned();
	let fingerprints =
		transformer_plugin_fingerprints(&temp.dir(), &config_path).unwrap_or_else(|error| panic!("{error}"));
	assert_eq!(fingerprints.len(), 1, "fingerprinted plugins, want 1");
	fingerprints
}

#[test]
fn transformer_plugin_fingerprints_pin_version_and_entry_contents() {
	let temp = write_project("@scope/plugin-salt", "");
	write_fake_transformer_package(
		&temp,
		"rotor-fake-transformer",
		&json!({"name": "rotor-fake-transformer", "version": "1.2.3", "main": "dist/index.js"}),
		"dist/index.js",
		"module.exports = () => (sf) => sf;\n",
	);
	write_plugin_tsconfig(&temp, &[json!({"transform": "rotor-fake-transformer", "after": true})]);

	let first = fingerprints(&temp).remove(0);
	assert_eq!(first.unresolved, "", "plugin unresolved");
	assert!(
		first.version == "1.2.3" && !first.entry_hash.is_empty(),
		"fingerprint = {first:?}, want the declared version and an entry hash"
	);
	assert!(
		first.config.contains(r#""after":true"#),
		"fingerprint config = {}, want the whole tsconfig entry",
		first.config
	);

	// An edit to the plugin's own code is what a version number cannot see.
	temp.write(
		"node_modules/rotor-fake-transformer/dist/index.js",
		"module.exports = () => (sf) => sf; // changed\n",
	);
	let edited = fingerprints(&temp);
	assert_ne!(
		edited[0].entry_hash, first.entry_hash,
		"editing the plugin entry left the fingerprint unchanged"
	);
}

#[test]
fn transformer_plugin_fingerprints_follow_package_exports() {
	let temp = write_project("@scope/plugin-salt-exports", "");
	write_fake_transformer_package(
		&temp,
		"@scope/exported-transformer",
		&json!({
			"name": "@scope/exported-transformer",
			"version": "0.0.9",
			"exports": {".": "./dist/index.cjs", "./package.json": "./package.json"},
		}),
		"dist/index.cjs",
		"module.exports = () => (sf) => sf;\n",
	);
	write_plugin_tsconfig(&temp, &[json!({"transform": "@scope/exported-transformer"})]);

	let found = fingerprints(&temp);
	assert!(
		found[0].unresolved.is_empty() && found[0].version == "0.0.9" && !found[0].entry_hash.is_empty(),
		"fingerprint = {:?}, want the exports entry resolved",
		found[0]
	);
}

/// A plugin that is not installed still has to be described, or two projects with different
/// broken plugin lists would share a salt.
#[test]
fn transformer_plugin_fingerprints_record_unresolved_packages() {
	let temp = write_project("@scope/plugin-salt-missing", "");
	write_plugin_tsconfig(
		&temp,
		&[json!({"transform": "rotor-transformer-that-is-not-installed"})],
	);

	let found = fingerprints(&temp);
	assert_eq!(
		found[0].unresolved, "package not installed",
		"fingerprint = {:?}, want it marked unresolved",
		found[0]
	);
}

/// `incrementalSaltWithFlamework(projectDir, program, opts, "", nil)` over a fresh program.
fn salt_of(temp: &Temp, opts: &[ProjectOptions]) -> Vec<String> {
	large_stack(|| {
		let (project_dir, program) = match new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default())
		{
			Ok(loaded) => loaded,
			Err(failure) => panic!("newProjectProgram: {failure} (diags: {:?})", failure.messages()),
		};
		opts.iter()
			.map(|opts| {
				incremental_salt_with_flamework(&project_dir, program, opts, "", None)
					.unwrap_or_else(|error| panic!("{error}"))
			})
			.collect()
	})
}

/// The salt is what an incremental build compares before it trusts a previous manifest, so the
/// plugin identity has to reach it.
#[test]
fn incremental_salt_moves_with_the_transformer_plugin_entry() {
	let temp = write_project("@scope/plugin-salt-build", "");
	write_fake_transformer_package(
		&temp,
		"rotor-fake-transformer",
		&json!({"name": "rotor-fake-transformer", "version": "1.0.0", "main": "index.js"}),
		"index.js",
		"module.exports = () => (sf) => sf;\n",
	);
	write_plugin_tsconfig(&temp, &[json!({"transform": "rotor-fake-transformer"})]);

	large_stack(|| {
		let (project_dir, program) = match new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default())
		{
			Ok(loaded) => loaded,
			Err(failure) => panic!("newProjectProgram: {failure} (diags: {:?})", failure.messages()),
		};
		let salt = || {
			incremental_salt_with_flamework(&project_dir, program, &ProjectOptions::default(), "", None)
				.unwrap_or_else(|error| panic!("{error}"))
		};
		let before = salt();
		temp.write(
			"node_modules/rotor-fake-transformer/index.js",
			"module.exports = () => (sf) => sf; // v2\n",
		);
		let after = salt();
		assert_ne!(
			before, after,
			"the incremental salt survived a transformer plugin change, so the previous outputs would be reused"
		);
	});
}

#[test]
fn incremental_salt_moves_with_optimized_array_appends() {
	let temp = write_project("@scope/array-append-salt", "");
	let salts = salt_of(
		&temp,
		&[
			ProjectOptions::default(),
			ProjectOptions {
				optimized_array_appends: true,
				..ProjectOptions::default()
			},
		],
	);
	assert_ne!(
		salts[0], salts[1],
		"optimizedArrayAppends did not invalidate incremental outputs"
	);
}

/// `nativeTypes` changes emitted text, so flipping it must force a rebuild.
#[test]
fn incremental_salt_moves_with_native_types() {
	let temp = write_project("@scope/native-types-salt", "");
	let salts = salt_of(
		&temp,
		&[
			ProjectOptions::default(),
			ProjectOptions {
				native_types: true,
				..ProjectOptions::default()
			},
		],
	);
	assert_ne!(salts[0], salts[1], "nativeTypes did not invalidate incremental outputs");
}

/// `luauTypes` changes emitted text, so flipping it must force a rebuild from any of its surfaces;
/// the command line turning it off restores the pre-option salt.
#[test]
fn incremental_salt_moves_with_luau_types() {
	let temp = write_project("@scope/luau-types-salt", "");
	let salts = salt_of(
		&temp,
		&[
			ProjectOptions::default(),
			ProjectOptions {
				luau_types: true,
				..ProjectOptions::default()
			},
			ProjectOptions {
				solution_argv: Some(luau_types_argv(true)),
				..ProjectOptions::default()
			},
			ProjectOptions {
				luau_types: true,
				solution_argv: Some(luau_types_argv(false)),
				..ProjectOptions::default()
			},
		],
	);
	let (before, after) = (&salts[0], &salts[1]);
	assert_ne!(before, after, "luauTypes did not invalidate incremental outputs");
	assert_eq!(
		&salts[2], after,
		"--luauTypes must salt the same as ProjectOptions.LuauTypes"
	);
	assert_eq!(&salts[3], before, "--luauTypes=false must restore the pre-option salt");
}

/// A `transform` that names a subpath inside the package resolves to that file, not to the
/// package entry.
#[test]
fn transformer_plugin_fingerprints_resolve_subpath_specifiers() {
	let temp = write_project("@scope/plugin-salt-subpath", "");
	write_fake_transformer_package(
		&temp,
		"rotor-multi-transformer",
		&json!({"name": "rotor-multi-transformer", "version": "2.0.0", "main": "out/index.js"}),
		"out/index.js",
		"module.exports = () => (sf) => sf; // entry\n",
	);
	let other = "module.exports = () => (sf) => sf; // other\n";
	temp.write("node_modules/rotor-multi-transformer/out/other.js", other);
	write_plugin_tsconfig(&temp, &[json!({"transform": "rotor-multi-transformer/out/other"})]);

	let found = fingerprints(&temp);
	assert!(
		found[0].unresolved.is_empty() && found[0].version == "2.0.0",
		"fingerprint = {:?}, want the subpath resolved",
		found[0]
	);
	assert_eq!(
		found[0].entry_hash,
		hex_sha256(other.as_bytes()),
		"fingerprint hashed the package entry, not the subpath the tsconfig named"
	);
}

/// A package with `exports` and no `main` still resolves, and a `main` naming a directory
/// follows Node's index lookup.
#[test]
fn transformer_plugin_fingerprints_resolve_awkward_package_shapes() {
	for (name, pkg, manifest, entry) in [
		(
			"exports only",
			"rotor-exports-only",
			json!({"name": "rotor-exports-only", "version": "3.1.0", "exports": {".": {"require": "./dist/main.cjs", "import": "./dist/main.mjs"}}}),
			"dist/main.cjs",
		),
		(
			"main names a directory",
			"rotor-main-directory",
			json!({"name": "rotor-main-directory", "version": "4.2.0", "main": "lib"}),
			"lib/index.js",
		),
		(
			"no main at all",
			"rotor-implicit-index",
			json!({"name": "rotor-implicit-index", "version": "5.0.0"}),
			"index.js",
		),
	] {
		let temp = write_project(&format!("@scope/plugin-salt-{pkg}"), "");
		write_fake_transformer_package(&temp, pkg, &manifest, entry, "module.exports = () => (sf) => sf;\n");
		write_plugin_tsconfig(&temp, &[json!({"transform": pkg})]);

		let found = fingerprints(&temp);
		assert!(
			found[0].unresolved.is_empty() && !found[0].entry_hash.is_empty(),
			"{name}: fingerprint = {:?}, want {entry} resolved",
			found[0]
		);
		assert_eq!(
			Some(found[0].version.as_str()),
			manifest["version"].as_str(),
			"{name}: fingerprint version"
		);
	}
}

/// An unresolvable plugin has no version and no entry to hash, so its tsconfig entry is the only
/// thing left that can tell two of them apart.
#[test]
fn unresolved_transformer_plugin_fingerprints_still_follow_the_tsconfig() {
	let temp = write_project("@scope/plugin-salt-unresolved-config", "");
	write_plugin_tsconfig(
		&temp,
		&[json!({"transform": "rotor-missing-transformer", "mode": "first"})],
	);
	let before = fingerprints(&temp);

	write_plugin_tsconfig(
		&temp,
		&[json!({"transform": "rotor-missing-transformer", "mode": "second"})],
	);
	let after = fingerprints(&temp);

	assert!(
		!before[0].unresolved.is_empty() && !after[0].unresolved.is_empty(),
		"expected both fingerprints unresolved, got {:?} and {:?}",
		before[0],
		after[0]
	);
	assert_ne!(
		before[0].config, after[0].config,
		"an unresolved plugin's option change left the fingerprint identical"
	);
}
