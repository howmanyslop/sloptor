//! `flamework_config_test.go`

use super::sidecar::set_repo_sidecar_path;
use super::{large_stack, write_project};
use crate::flamework_config::prepare_flamework_config;
use crate::project::{ProjectOptions, new_project_program_with_options, project_source_files};
use crate::sidecar::{prepare_transformer_program_with_host, set_test_env, transformer_host_for};

fn add_plugins(temp: &super::Temp, plugins: &str) {
	let tsconfig = std::fs::read_to_string(temp.join("tsconfig.json")).expect("read tsconfig");
	let updated = tsconfig.replacen(
		r#""compilerOptions": {"#,
		&format!(r#""compilerOptions": {{"plugins": {plugins},"#),
		1,
	);
	temp.write("tsconfig.json", updated);
}

/// `prepareTransformerProgram(dir, program, projectSourceFiles(program), nil)`
fn prepare_transformer_program(dir: &str) -> Result<crate::sidecar::PreparedTransformerProgram, crate::Failure> {
	large_stack(|| {
		let (abs_dir, program) = new_project_program_with_options(dir, "", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {} (diags: {:?})", failure.error, failure.messages()));
		let opts = ProjectOptions::default();
		prepare_transformer_program_with_host(
			None,
			transformer_host_for(&opts),
			&abs_dir,
			program,
			&project_source_files(program),
			&std::collections::BTreeMap::new(),
		)
	})
}

#[test]
fn absent_rotor_config_leaves_generic_transformer_scheduled() {
	// Given: a project with a generic TypeScript transformer and no rotor.toml.
	let temp = write_project("generic-transformer-without-rotor-config", "");
	let dir = temp.dir();
	assert!(
		matches!(
			rotor_config::config::load(&dir),
			Err(rotor_config::config::LoadError::NotFound)
		),
		"Load() want ErrNotFound"
	);
	add_plugins(&temp, r#"[{"transform": "example-transformer"}]"#);
	set_repo_sidecar_path();
	set_test_env("ROTOR_NODE_PATH", &temp.join("missing-node").to_string_lossy());

	// When: the transformer program is prepared.
	let result = prepare_transformer_program(&dir);

	// Then: the generic transformer still reaches the Node scheduling seam.
	let Err(failure) = result else {
		panic!("prepareTransformerProgram() succeeded, want generic transformer scheduling error");
	};
	assert!(
		failure.error.to_string().contains("node executable not found"),
		"prepareTransformerProgram() = ({:?}, {}), want generic transformer scheduling error",
		failure.messages(),
		failure.error
	);
}

#[test]
fn prepare_transformer_program_activates_empty_flamework_config() {
	let temp = write_project("flamework-empty", "");
	set_test_env("ROTOR_NODE_PATH", &temp.join("missing-node").to_string_lossy());
	temp.write("rotor.toml", "[flamework]\n");
	let prepared = prepare_transformer_program(&temp.dir()).unwrap_or_else(|failure| {
		panic!(
			"prepareTransformerProgram: {} (diags: {:?})",
			failure.error,
			failure.messages()
		)
	});
	assert!(
		prepared.flamework.is_some(),
		"empty [flamework] must activate native preparation"
	);
}

#[test]
fn prepare_transformer_program_selects_flamework_profile_for_active_ts_config() {
	// Given: two configs whose legacy transformers required different anchors.
	let temp = write_project("flamework-profiles", "");
	let dir = temp.dir();
	let base = std::fs::read(temp.join("tsconfig.json")).expect("read tsconfig");
	temp.write("tsconfig.lib.json", base);
	temp.write(
		"rotor.toml",
		r#"[flamework.profiles."tsconfig.json"]
after = "redacted-react-compiler"

[flamework.profiles."./tsconfig.lib.json"]
after = "jest"
skipUnchangedFiles = true
"#,
	);
	let lib_config = temp.join("tsconfig.lib.json").to_string_lossy().into_owned();
	let selected = large_stack(|| {
		let (_, program) = new_project_program_with_options(&dir, &lib_config, &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("newProjectProgram: {} (diags: {:?})", failure.error, failure.messages()));

		// When: native configuration is prepared for tsconfig.lib.json.
		prepare_flamework_config(&dir, &program.command_line())
	});
	// Then: the lib profile is selected, not the root profile.
	let selected = selected.unwrap_or_else(|failure| {
		panic!(
			"prepareFlameworkConfig: {} (diags: {:?})",
			failure.error,
			failure.messages()
		)
	});
	assert!(
		selected
			.as_ref()
			.is_some_and(|selected| selected.after == "jest" && selected.skip_unchanged_files),
		"selected Flamework config = {selected:?}, want lib profile after jest with skipUnchangedFiles"
	);
}

#[test]
fn prepare_transformer_program_rejects_native_and_legacy_flamework() {
	let temp = write_project("flamework-double", "");
	temp.write("rotor.toml", "[flamework]\n");
	add_plugins(&temp, r#"[{"transform": "rbxts-transformer-flamework"}]"#);
	let result = prepare_transformer_program(&temp.dir());
	let Err(failure) = result else {
		panic!("prepareTransformerProgram() succeeded, want double-configuration error");
	};
	assert!(
		failure.error.to_string().contains("cannot be combined"),
		"prepareTransformerProgram() = ({:?}, {}), want double-configuration error",
		failure.messages(),
		failure.error
	);
}
