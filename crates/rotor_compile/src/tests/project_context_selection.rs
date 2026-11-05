//! `project_context_selection_test.go`: an empty transform selection skips the import-resolution
//! setup without losing diagnostics, validation, cancellation or the next build's import context.

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
#[cfg(unix)]
use std::time::{Duration, SystemTime};

use rotor_support::{gopath, logservice};
use rotor_transformer::ProjectType;
use serde_json::json;

#[cfg(unix)]
use super::large_stack;
#[cfg(unix)]
use super::output::set_mtime;
use super::output::{enable_incremental_builds, try_build};
use super::{Temp, set_compiler_options, write_project};
use crate::compile::diagnostic_info_messages;
use crate::output::build_failure_messages;
use crate::project::Cancellation;
#[cfg(unix)]
use crate::project::new_project_program_with_options;
#[cfg(unix)]
use crate::project::{new_project_context, new_project_context_with_import_context};
use crate::timings::{BuildTimingCounts, BuildTimings};
use crate::{BuildFailure, BuildResult, ProjectOptions};

const PROJECT_CONTEXT_SELECTION_MODEL_ROJO: &str =
	r#"{"name":"kind","tree":{"$path":"out","include":{"$path":"include"}}}"#;
#[cfg(unix)]
const PROJECT_CONTEXT_SELECTION_GAME_ROJO: &str = r#"{"name":"kind","tree":{"$className":"DataModel","ReplicatedStorage":{"out":{"$path":"out"},"include":{"$path":"include"},"node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}}"#;

const KINDS: [ProjectType; 3] = [ProjectType::Package, ProjectType::Model, ProjectType::Game];

/// `setProjectContextSelectionOptions`
fn set_options(temp: &Temp, overrides: &serde_json::Value) {
	set_compiler_options(&temp.join("tsconfig.json"), overrides);
}

/// `readProjectContextSelectionOutputs`
fn read_outputs(dir: &str, paths: &[String]) -> HashMap<String, Vec<u8>> {
	paths
		.iter()
		.map(|name| {
			let data = fs::read(gopath::join(&[dir, &gopath::from_slash(name)]))
				.unwrap_or_else(|error| panic!("read {name}: {error}"));
			(name.clone(), data)
		})
		.collect()
}

/// A build with fresh timings: the result and the counts.
fn timed_build(dir: &str, opts: &ProjectOptions) -> (Result<BuildResult, BuildFailure>, BuildTimingCounts) {
	let timings = Arc::new(BuildTimings::new());
	let opts = ProjectOptions {
		timings: Some(Arc::clone(&timings)),
		..opts.clone()
	};
	let result = try_build(dir, &opts);
	(result, timings.report().counts)
}

/// A build that must succeed with no diagnostics.
fn clean(result: Result<BuildResult, BuildFailure>, what: &str) -> BuildResult {
	match result {
		Ok(result) => {
			assert!(
				result.diagnostics.is_empty(),
				"{what}: diagnostics {:?}",
				result.diagnostics
			);
			result
		}
		Err(failure) => panic!("{what}: {} ({:?})", failure.failure, build_failure_messages(&failure)),
	}
}

#[cfg(unix)]
#[test]
fn project_context_selection_omits_only_transform_imports() {
	for kind in KINDS {
		let temp = super::pnpm::write_pnpm_fixture();
		temp.write("default.project.json", PROJECT_CONTEXT_SELECTION_GAME_ROJO);
		large_stack(|| {
			let (abs_dir, program) = match new_project_program_with_options(&temp.dir(), "", &ProjectOptions::default())
			{
				Ok(loaded) => loaded,
				Err(failure) => panic!("{}: program: {failure} ({:?})", kind.as_str(), failure.messages()),
			};
			let opts = ProjectOptions {
				project_type: Some(kind),
				project_reference_paths: Some(Arc::new(HashMap::from([(
					"reference.d.ts".to_owned(),
					"reference.luau".to_owned(),
				)]))),
				..ProjectOptions::default()
			};
			let full = new_project_context(&abs_dir, program, &opts).unwrap_or_else(|failure| {
				panic!("{}: full context: {failure} ({:?})", kind.as_str(), failure.messages())
			});
			let empty =
				new_project_context_with_import_context(&abs_dir, program, &opts, false).unwrap_or_else(|failure| {
					panic!(
						"{}: empty-selection context: {failure} ({:?})",
						kind.as_str(),
						failure.messages()
					)
				});
			let want_package_resolvers = if kind == ProjectType::Package {
				program.options().type_roots.as_ref().map_or(0, Vec::len)
			} else {
				0
			};
			assert_eq!(
				full.rojo_context.pkg_rojo_resolvers.len(),
				want_package_resolvers,
				"{}: full wrapper package resolvers",
				kind.as_str()
			);
			assert!(
				!full.rojo_context.node_modules_path_mapping.exact.is_empty(),
				"{}: full wrapper omitted node_modules entry mappings",
				kind.as_str()
			);
			assert!(
				empty.rojo_context.pkg_rojo_resolvers.is_empty()
					&& empty.rojo_context.node_modules_path_mapping.exact.is_empty()
					&& empty.rojo_context.node_modules_path_mapping.patterns.is_empty(),
				"{}: empty selection constructed transform import context",
				kind.as_str()
			);
			assert!(
				empty.dir == full.dir && empty.project_type == full.project_type,
				"{}: empty selection lost project identity",
				kind.as_str()
			);
			let (got, want) = (&empty.rojo_context, &full.rojo_context);
			assert!(
				got.path_translator == want.path_translator
					&& got.runtime_lib_rbx_path == want.runtime_lib_rbx_path
					&& got.project_path == want.project_path
					&& got.node_modules_path == want.node_modules_path
					&& got.type_roots == want.type_roots
					&& got.use_case_sensitive_file_names == want.use_case_sensitive_file_names
					&& got.project_reference_paths == want.project_reference_paths,
				"{}: empty selection changed retained resolver/path/reference context",
				kind.as_str()
			);
		});
	}
}

/// An empty pass must not leave import context missing on the next selected build.
#[cfg(unix)]
#[test]
fn project_context_selection_no_change_then_edit_matches_fresh() {
	let imports = "import { dummy } from \"@rbxts/dummy\";\nimport { linked } from \"@rbxts/linked\";\n";
	let edited = format!("{imports}export const result = dummy() + linked() + 1;\nprint(result);\n");
	let prepare = |source: &str| {
		let temp = super::pnpm::write_pnpm_fixture();
		enable_incremental_builds(&temp);
		set_options(
			&temp,
			&json!({"declaration": true, "declarationMap": true, "sourceMap": true}),
		);
		temp.write("src/main.ts", source);
		temp.write("src/data.json", "{\"copied\":true}\n");
		temp
	};
	let opts = ProjectOptions {
		project_type: Some(ProjectType::Package),
		..ProjectOptions::default()
	};
	let build = |dir: &str| {
		let (result, counts) = timed_build(dir, &opts);
		(clean(result, "build"), counts)
	};
	let temp = prepare(&format!(
		"{imports}export const result = dummy() + linked();\nprint(result);\n"
	));
	let dir = temp.dir();
	let (seed, _) = build(&dir);
	let before = read_outputs(&dir, &seed.owned_outputs);
	let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
	for name in &seed.owned_outputs {
		set_mtime(&temp.join(name), old);
	}
	let (nc, counts) = build(&dir);
	assert!(
		counts.selected_sources == 0
			&& counts.actual_writes == 0
			&& nc.outputs.is_empty()
			&& nc.emitted_files.is_empty(),
		"no-change pass selected/emitted/wrote: {counts:?}, outputs {:?}, emitted {:?}",
		nc.outputs.keys(),
		nc.emitted_files
	);
	assert!(
		nc.owned_outputs == seed.owned_outputs && read_outputs(&dir, &nc.owned_outputs) == before,
		"no-change pass lost owned outputs or changed their bytes"
	);
	for name in &nc.owned_outputs {
		let modified = fs::metadata(temp.join(name))
			.and_then(|meta| meta.modified())
			.expect("the output's mtime");
		assert_eq!(modified, old, "no-change pass rewrote {name}");
	}

	temp.write("src/main.ts", &edited);
	let (changed, counts) = build(&dir);
	assert_eq!(counts.selected_sources, 1, "edited pass selected sources, want 1");
	let fresh_temp = prepare(&edited);
	let fresh_dir = fresh_temp.dir();
	let (fresh, _) = build(&fresh_dir);
	assert!(
		changed.owned_outputs == fresh.owned_outputs
			&& read_outputs(&dir, &changed.owned_outputs) == read_outputs(&fresh_dir, &fresh.owned_outputs),
		"build after empty selection differs from fresh Luau/declaration/map/copied output"
	);
	let main =
		String::from_utf8_lossy(changed.outputs.get("out/main.luau").map_or(&[][..], Vec::as_slice)).into_owned();
	for want in [
		r#"TS.getModule(script, "@rbxts", "dummy").out"#,
		r#"TS.getModule(script, "@rbxts", "linked")"#,
	] {
		assert!(main.contains(want), "edited output lost import {want}:\n{main}");
	}
}

/// The build's diagnostics and error, as Go's `(diags, err)` pair.
fn diags_and_error(result: &Result<BuildResult, BuildFailure>) -> (Vec<String>, Option<String>) {
	match result {
		Ok(result) => (diagnostic_info_messages(&result.diagnostics), None),
		Err(failure) => (build_failure_messages(failure), Some(failure.failure.to_string())),
	}
}

#[test]
fn project_context_selection_main_warnings_and_invalid_mount() {
	for kind in KINDS {
		for empty_source in [false, true] {
			let name = format!(
				"{}/{}",
				kind.as_str(),
				if empty_source { "empty-source" } else { "no-change" }
			);
			let temp = write_project("context-selection-fixture", PROJECT_CONTEXT_SELECTION_MODEL_ROJO);
			if empty_source {
				fs::remove_file(temp.join("src/main.ts")).expect("remove main.ts");
			}
			enable_incremental_builds(&temp);
			temp.write("node_modules/@rbxts/broken/default.project.json", "{");
			let main_rojo = r#"{"name":"kind","tree":{"$path":"out","include":{"$path":"include"},"Broken":{"$path":"node_modules/@rbxts/broken"},"Missing":{"$path":"missing.project.json"}}}"#;
			temp.write("default.project.json", main_rojo);
			let dir = temp.dir();
			let missing = gopath::join(&[&dir, "missing.project.json"]);
			let opts = ProjectOptions {
				project_type: Some(kind),
				..ProjectOptions::default()
			};
			let build = || {
				logservice::capture();
				let (result, counts) = timed_build(&dir, &opts);
				let log = logservice::take_capture();
				let lines: Vec<&str> = log.strip_suffix('\n').unwrap_or(&log).split('\n').collect();
				assert!(
					lines.len() == 2
						&& lines[0].starts_with("Compiler Warning: RojoResolver: Invalid configuration!")
						&& lines[1] == format!("Compiler Warning: RojoResolver: Path does not exist \"{missing}\""),
					"{name}: main warnings/order = {log:?}"
				);
				let (diags, error) = diags_and_error(&result);
				(counts, diags, error)
			};
			let (_, diags, error) = build();
			assert!(
				error.is_none() && diags.is_empty(),
				"{name}: seed: {error:?} ({diags:?})"
			);
			let (counts, diags, error) = build();
			assert!(
				error.is_none() && diags.is_empty() && counts.selected_sources == 0 && counts.actual_writes == 0,
				"{name}: empty selection: {error:?} ({diags:?}), counts {counts:?}"
			);
			// A source mount is invalid even in explicit package mode. The declaration-only input
			// case keeps selection empty after this edit.
			temp.write(
				"default.project.json",
				main_rojo.replacen(r#""$path":"out""#, r#""$path":"src""#, 1),
			);
			let (counts, diags, error) = build();
			assert!(
				error.is_some() && diags.join("\n").contains("Rojo"),
				"{name}: invalid main mount accepted: {error:?} ({diags:?})"
			);
			if empty_source {
				assert_eq!(
					counts.selected_sources, 0,
					"{name}: empty-source validation selected files"
				);
			}
			temp.write("default.project.json", main_rojo);
			let (counts, diags, error) = build();
			assert!(
				error.is_none() && diags.is_empty() && counts.selected_sources == 0 && counts.actual_writes == 0,
				"{name}: recovery: {error:?} ({diags:?}), counts {counts:?}"
			);
		}
	}
}

#[test]
fn project_context_selection_empty_diagnostics_and_cancellation() {
	for gate in ["program", "global", "cancellation"] {
		let temp = write_project("@scope/context-selection-diagnostics", "");
		fs::remove_file(temp.join("src/main.ts")).expect("remove main.ts");
		let mut opts = ProjectOptions::default();
		let mut want = "declarationMap";
		match gate {
			"program" => set_options(&temp, &json!({"declarationMap": true})),
			"global" => {
				temp.write("src/globals.d.ts", "// No fundamental global types.\n");
				want = "Cannot find global type";
			}
			_ => {
				let cancellation = Cancellation::default();
				cancellation.cancel();
				opts.context = Some(cancellation);
			}
		}
		let (result, counts) = timed_build(&temp.dir(), &opts);
		let (diags, error) = diags_and_error(&result);
		if gate == "cancellation" {
			assert!(
				error.as_deref().is_some_and(|error| error.contains("context canceled")),
				"{gate}: empty selection error = {error:?}, want cancellation"
			);
		} else {
			let has_result_diagnostics = match &result {
				Ok(_) => false,
				Err(failure) => failure
					.result
					.as_ref()
					.is_some_and(|result| !result.diagnostics.is_empty()),
			};
			assert!(
				error.is_some() && has_result_diagnostics && diags.join("\n").contains(want),
				"{gate}: empty selection lost diagnostics: {error:?} ({diags:?})"
			);
		}
		assert!(
			counts.selected_sources == 0 && counts.actual_writes == 0,
			"{gate}: empty selection selected/wrote output: {counts:?}"
		);
	}
}

#[test]
fn project_context_selection_empty_runtime_mount_validation() {
	for kind in [ProjectType::Model, ProjectType::Game] {
		for (name, rojo, want) in [
			(
				"missing-rojo",
				"",
				"Non-package projects must have a Rojo project file!",
			),
			(
				"missing-include",
				r#"{"name":"x","tree":{"$path":"out"}}"#,
				"Rojo project contained no data for include folder!",
			),
			(
				"network",
				r#"{"name":"x","tree":{"$className":"DataModel","ServerScriptService":{"out":{"$path":"out"},"include":{"$path":"include"}}}}"#,
				"Runtime library cannot be in a server-only or client-only container!",
			),
			(
				"isolated",
				r#"{"name":"x","tree":{"$className":"DataModel","PluginDebugService":{"include":{"$path":"include"}},"ReplicatedStorage":{"out":{"$path":"out"}}}}"#,
				"Runtime library cannot be in an isolated container!",
			),
		] {
			let temp = write_project("@scope/override-package", rojo);
			fs::remove_file(temp.join("src/main.ts")).expect("remove main.ts");
			let opts = ProjectOptions {
				project_type: Some(kind),
				..ProjectOptions::default()
			};
			let (result, counts) = timed_build(&temp.dir(), &opts);
			let (diags, error) = diags_and_error(&result);
			assert!(
				error.is_some() && diags == [want] && counts.selected_sources == 0,
				"{}/{name}: empty selection lost mount validation: {error:?} ({diags:?}), counts {counts:?}",
				kind.as_str()
			);
		}
	}
}

#[test]
fn project_context_selection_no_change_cancellation() {
	let temp = write_project("@scope/context-selection-cancel", "");
	enable_incremental_builds(&temp);
	let dir = temp.dir();
	let seed = clean(try_build(&dir, &ProjectOptions::default()), "seed");
	let want = read_outputs(&dir, &seed.owned_outputs);
	let cancellation = Cancellation::default();
	cancellation.cancel();
	let opts = ProjectOptions {
		context: Some(cancellation),
		..ProjectOptions::default()
	};
	let (result, counts) = timed_build(&dir, &opts);
	let (diags, error) = diags_and_error(&result);
	assert!(
		error.as_deref().is_some_and(|error| error.contains("context canceled"))
			&& counts.selected_sources == 0
			&& counts.actual_writes == 0,
		"no-change cancellation: {error:?} ({diags:?}), counts {counts:?}"
	);
	assert!(
		read_outputs(&dir, &seed.owned_outputs) == want,
		"cancelled no-change pass changed output bytes"
	);
}

#[cfg(unix)]
#[test]
fn project_context_selection_missing_output_restores_import_context() {
	for missing in [
		"out/main.luau",
		"out/main.luau.map",
		"out/main.d.ts",
		"out/main.d.ts.map",
	] {
		let temp = super::pnpm::write_pnpm_fixture();
		enable_incremental_builds(&temp);
		set_options(
			&temp,
			&json!({"declaration": true, "declarationMap": true, "sourceMap": true}),
		);
		let dir = temp.dir();
		let opts = ProjectOptions {
			project_type: Some(ProjectType::Package),
			..ProjectOptions::default()
		};
		let seed = clean(try_build(&dir, &opts), "seed");
		let want = read_outputs(&dir, &seed.owned_outputs);
		let (result, counts) = timed_build(&dir, &opts);
		clean(result, "no-change");
		assert_eq!(counts.selected_sources, 0, "{missing}: no-change counts {counts:?}");
		fs::remove_file(temp.join(missing)).expect("remove the output");
		let (result, counts) = timed_build(&dir, &opts);
		let recovered = clean(result, "recovery");
		assert!(
			counts.selected_sources == 1 && counts.actual_writes == 1,
			"{missing}: recovery did not select/recreate missing output: {counts:?}, emitted {:?}",
			recovered.emitted_files
		);
		assert!(
			recovered.owned_outputs == seed.owned_outputs && read_outputs(&dir, &recovered.owned_outputs) == want,
			"{missing}: missing-output recovery changed output ownership or bytes"
		);
	}
}
