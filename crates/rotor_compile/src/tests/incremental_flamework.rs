//! `incremental_flamework_test.go`

use std::sync::{Arc, Mutex};

use rotor_config::config::{FlameworkConfig, FlameworkOptimizations};

use super::{Temp, large_stack};
use crate::flamework_pipeline::FlameworkPipeline;
use crate::incremental_flamework::{
	FlameworkIncrementalFile, FlameworkIncrementalGlob, FlameworkIncrementalInputs, flamework_incremental_inputs,
	flamework_incremental_salt,
};

fn file(path: &str, contents: &str) -> FlameworkIncrementalFile {
	FlameworkIncrementalFile {
		path: path.to_owned(),
		contents: contents.as_bytes().to_vec(),
	}
}

fn glob(pattern: &str, matches: &[&str]) -> FlameworkIncrementalGlob {
	FlameworkIncrementalGlob {
		pattern: pattern.to_owned(),
		matches: Some(matches.iter().map(|&path| path.to_owned()).collect()),
	}
}

fn salt(inputs: &FlameworkIncrementalInputs) -> String {
	flamework_incremental_salt(Some(inputs)).expect("salt")
}

#[test]
fn flamework_incremental_salt_deterministic_across_input_order() {
	// Given: equivalent effective Flamework inputs in different discovery order.
	let first = FlameworkIncrementalInputs {
		effective_config: FlameworkConfig {
			id_generation_mode: "full".to_owned(),
			optimizations: FlameworkOptimizations {
				guard_generation_dedup_limit: Some(0),
			},
			..FlameworkConfig::default()
		},
		transformer_version: "1.3.2-native".to_owned(),
		effective_plugins: vec![
			r#"{"transform":"first","z":1,"a":2}"#.to_owned(),
			r#"{"transform":"second"}"#.to_owned(),
		],
		package_inputs: vec![
			file(r"node_modules\\@scope\\two\\package.json", r#"{"name":"@scope/two"}"#),
			file("node_modules/@scope/one/package.json", r#"{"name":"@scope/one"}"#),
		],
		build_info_inputs: vec![
			file("node_modules/@scope/two/flamework.build", r#"{"version":2}"#),
			file("node_modules/@scope/one/flamework.build", r#"{"version":1}"#),
		],
		relevant_globs: vec![
			glob(r"src\\server\\**\\*.ts", &[r"src\\server\\main.ts"]),
			glob("src/shared/**/*.ts", &["src/shared/two.ts", "src/shared/one.ts"]),
		],
		..FlameworkIncrementalInputs::default()
	};
	let mut second = FlameworkIncrementalInputs {
		effective_plugins: vec![
			r#"{"a":2,"transform":"first","z":1}"#.to_owned(),
			r#"{"transform":"second"}"#.to_owned(),
		],
		relevant_globs: vec![
			glob("src/shared/**/*.ts", &["src/shared/one.ts", "src/shared/two.ts"]),
			glob("src/server/**/*.ts", &["src/server/main.ts"]),
		],
		..first.clone()
	};
	second.package_inputs.reverse();
	second.build_info_inputs.reverse();

	// When: both salt contributions are calculated.
	// Then: the output hashes are byte-for-byte deterministic.
	assert_eq!(salt(&first), salt(&second), "equivalent Flamework salts differ");
}

type Mutation = fn(&mut FlameworkIncrementalInputs);

#[test]
fn flamework_incremental_salt_changes_for_every_transformer_input() {
	// Given: one effective native Flamework input snapshot.
	let base = FlameworkIncrementalInputs {
		effective_config: FlameworkConfig {
			id_generation_mode: "full".to_owned(),
			..FlameworkConfig::default()
		},
		transformer_version: "1.3.2-native".to_owned(),
		effective_plugins: vec![r#"{"transform":"plugin","option":true}"#.to_owned()],
		runtime_config: Some(br#"{"profiling":false}"#.to_vec()),
		package_inputs: vec![file("package.json", r#"{"name":"fixture","version":"1.0.0"}"#)],
		build_info_inputs: vec![file("node_modules/@scope/pkg/flamework.build", r#"{"version":1}"#)],
		relevant_globs: vec![glob("src/**/*.ts", &["src/main.ts"])],
	};
	let base_salt = salt(&base);

	let tests: [(&str, Mutation); 9] = [
		("effective config", |inputs| {
			"changed".clone_into(&mut inputs.effective_config.hash_prefix);
		}),
		("transformer version", |inputs| {
			"1.3.3-native".clone_into(&mut inputs.transformer_version);
		}),
		("effective plugin config", |inputs| {
			inputs.effective_plugins[0] = r#"{"transform":"plugin","option":false}"#.to_owned();
		}),
		("effective plugin removal", |inputs| inputs.effective_plugins.clear()),
		("runtime config", |inputs| {
			inputs.runtime_config = Some(br#"{"profiling":true}"#.to_vec());
		}),
		("package state", |inputs| {
			inputs.package_inputs[0].contents = br#"{"name":"fixture","version":"2.0.0"}"#.to_vec();
		}),
		("package build info", |inputs| {
			inputs.build_info_inputs[0].contents = br#"{"version":2}"#.to_vec();
		}),
		("glob pattern", |inputs| {
			"src/server/**/*.ts".clone_into(&mut inputs.relevant_globs[0].pattern);
		}),
		("glob matches", |inputs| {
			if let Some(matches) = &mut inputs.relevant_globs[0].matches {
				matches.push("src/added.ts".to_owned());
			}
		}),
	];
	for (name, mutate) in tests {
		let mut changed = base.clone();
		mutate(&mut changed);

		// When: one real transformer input changes.
		// Then: the observable cache salt changes.
		assert_ne!(salt(&changed), base_salt, "{name} change preserved salt {base_salt}");
	}
}

#[test]
fn flamework_incremental_inputs_excludes_generated_root_build_state() {
	// Given: a native project with one dependency build input and persisted root glob state.
	let temp = Temp::new();
	let root = temp.dir();
	temp.write("package.json", r#"{"name":"fixture","version":"1.0.0"}"#);
	temp.write("flamework.json", r#"{"profiling":false}"#);
	temp.write(
		"flamework.build",
		r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{"root":"one"},"metadata":{"globs":{"paths":{"src/**/*.ts":["src/main.ts"]},"origins":{}}}}"#,
	);
	temp.write(
		"node_modules/@scope/dependency/package.json",
		r#"{"name":"@scope/dependency","version":"1.0.0"}"#,
	);
	let dependency_build = temp
		.join("node_modules/@scope/dependency/flamework.build")
		.to_string_lossy()
		.into_owned();
	temp.write(
		"node_modules/@scope/dependency/flamework.build",
		r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{"dependency":"one"}}"#,
	);
	// Go's pipeline has no program, whose static addPaths walk then finds nothing; the port's
	// needs one, over a project with no addPaths call.
	temp.write("tsconfig.json", super::WRITE_PROJECT_TSCONFIG);
	temp.write("src/main.ts", "export {};\n");
	large_stack(|| {
		let (_, program) =
			crate::project::new_project_program_with_options(&root, "", &crate::project::ProjectOptions::default())
				.unwrap_or_else(|failure| panic!("newProjectProgram: {}", failure.error));
		let config = FlameworkConfig {
			id_generation_mode: "full".to_owned(),
			..FlameworkConfig::default()
		};
		let mut project = rotor_flamework::open_project(&rotor_flamework::ProjectOptions {
			project_dir: root.clone(),
			root_dir: "src".to_owned(),
			out_dir: "out".to_owned(),
			config: config.clone(),
			..rotor_flamework::ProjectOptions::default()
		})
		.expect("open project");
		project
			.add_package_build_info(&dependency_build)
			.expect("add package build info");
		let state = FlameworkPipeline {
			config,
			project: Arc::new(Mutex::new(project)),
			program,
			plugins: Vec::new(),
			prefix: Vec::new(),
			suffix: Vec::new(),
		};
		let inputs = || {
			flamework_incremental_inputs(Some(&state))
				.expect("inputs")
				.expect("some inputs")
		};
		let before_salt = salt(&inputs());

		// When: only the generated root identifier cache changes.
		temp.write(
			"flamework.build",
			r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{"root":"two"},"metadata":{"globs":{"paths":{"src/**/*.ts":["src/main.ts"]},"origins":{}}}}"#,
		);
		let after_salt = salt(&inputs());

		// Then: the generated artifact does not self-invalidate, while dependency state remains
		// salted.
		assert_eq!(after_salt, before_salt, "generated root build changed salt");
		temp.write(
			"node_modules/@scope/dependency/flamework.build",
			r#"{"version":1,"flameworkVersion":"1.3.2","identifiers":{"dependency":"two"}}"#,
		);
		let dependency_salt = salt(&inputs());
		assert_ne!(
			dependency_salt, before_salt,
			"dependency build state preserved salt {before_salt}"
		);
	});
}
