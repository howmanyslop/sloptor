//! `flamework_program_preparse_test.go`. Most tests run one update on two fresh programs loaded
//! from the same project: the serial control is single-threaded, so it takes the original update
//! path, and the candidate is multi-threaded, so its overlays preparse.

use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use rotor_support::gopath;
use rotor_tsrs::ast::{Diagnostic, ExternalModuleIndicatorOptions, SourceFile, SourceFileParseOptions};
use rotor_tsrs::compiler::{CompilerHost, Program, new_compiler_host};
use rotor_tsrs::tspath::{self, Path};
use rotor_tsrs::vfs::vfstest;
use rotor_tsrs::{Context, P};

use super::{Temp, large_stack, write_project};
use crate::flamework_program_update::{
	LAST_PREPARSE, LastPreparse, NativeProgramUpdate, NativeSourceOverlay, OverlayAliasEntry, OverlayParse,
	PreparsedOverlays, compose_native_overlay_traces, merge_overlay_aliases, new_native_overlay_host_with_aliases,
	parallel_overlay_aliases, preparse_overlays, update_native_flamework_program, update_program_files_with_overlays,
	update_program_with_text_overlays,
};
use crate::incremental_manifest::normalize_source_file_path;
use crate::overlayfs::overlay_aliases;
use crate::project::{ProjectOptions, lock, new_project_program_with_options, project_source_files};
use crate::sidecar::normalize_overlay_path;
use crate::source_trace::{DiagnosticTraces, SourceTraceMap, compose_source_trace_maps};

const PREPARSE_DEPRECATED_SOURCE: &str = "/**\n * Old helper.\n * @deprecated Use fresh.\n */\nexport function old(): number {\n\treturn 1;\n}\n\nexport function fresh(): number {\n\treturn 2;\n}\n";

const PREPARSE_DEPRECATED_USE: &str = "import { old } from \"./dep\";\n\n/** Uses the old helper. */\nexport const a: number = \"x\" as unknown as string;\nexport const legacy = old();\n";

const PREPARSE_NEW_IMPORT: &str = "import { extra } from \"../lib/extra\";\nexport const a = extra;\n";

/// A rebuild scenario: its name, its overlays by project path, a changed file the rebuilt graph
/// leaves out, and whether the native update runs it too.
type RebuildScenario<'a> = (&'a str, Vec<(&'a str, &'a str)>, &'a str, bool);

/// A preparse scenario: its name, the changed paths and the files that preparse, if any.
type PreparseScenario<'a> = (&'a str, Vec<Path>, Option<Vec<&'a str>>);

/// `preparseRebuildFiles`: a changed source can rebuild the program: the lib files are outside
/// the include set, and only z.ts imports late.ts.
const PREPARSE_REBUILD_FILES: [(&str, &str); 6] = [
	("lib/extra.ts", "export const extra = 1;\n"),
	("lib/late.ts", "export const late = 1;\n"),
	("lib/ref.d.ts", "export declare const referenced: number;\n"),
	("src/a.ts", "export const a = 1;\n"),
	("src/m.ts", "export const m = 1;\n"),
	(
		"src/z.ts",
		"import { late } from \"../lib/late\";\nexport const z = late;\n",
	),
];

fn trace(file_name: &str, text: &str, lines: &[(u32, u32)]) -> Arc<SourceTraceMap> {
	let mappings: Vec<(u32, u32, u32, u32)> = lines
		.iter()
		.map(|&(generated_line, source_line)| (generated_line, 0, source_line, 0))
		.collect();
	Arc::new(SourceTraceMap::for_test(file_name, text, &mappings))
}

fn overlay(file_name: &str, text: &str, trace: Option<&Arc<SourceTraceMap>>) -> NativeSourceOverlay {
	NativeSourceOverlay {
		file_name: file_name.to_owned(),
		text: text.to_owned(),
		trace: trace.cloned(),
		rebase: None,
	}
}

/// The preparse the last update on this thread left on its program's host.
fn last_preparse() -> Option<LastPreparse> {
	LAST_PREPARSE.with(|last| last.borrow_mut().take())
}

#[test]
fn preparse_native_update_matches_serial_control() {
	// Given: reverse-ordered native overlays that change text, imports, JSDoc use and
	// diagnostics, next to untouched sources and shared or absent traces.
	let temp = write_preparse_project(&[
		("src/a.ts", "export const a = 1;\n"),
		("src/b.ts", "export const b = 1;\n"),
		("src/c.tsx", "export const c = 1;\n"),
		("src/dep.ts", PREPARSE_DEPRECATED_SOURCE),
		("src/untouched.ts", "export const untouched = 1;\n"),
	]);
	let dir = temp.dir();
	large_stack(|| {
		let [control, candidate] = load_preparse_programs(&dir);
		let shared = trace("shared.ts", "shared", &[(0, 2)]);
		let authored = trace("a.ts", "authored", &[(0, 7), (3, 9)]);
		let native = trace("a.ts", "native", &[(0, 0), (2, 3)]);
		let prefix = trace("prefix.ts", "prefix", &[]);
		let run = |program: &'static Program| {
			let (a, b, c) = (
				preparse_source(program, &dir, "src/a.ts"),
				preparse_source(program, &dir, "src/b.ts"),
				preparse_source(program, &dir, "src/c.tsx"),
			);
			let traces = DiagnosticTraces(HashMap::from([
				(normalize_source_file_path(a.file_name()), Arc::clone(&authored)),
				(normalize_source_file_path(b.file_name()), Arc::clone(&shared)),
				("prefix".to_owned(), Arc::clone(&prefix)),
			]));
			let sources = project_source_files(program);
			let result = update_native_flamework_program(NativeProgramUpdate {
				program,
				source_files: &sources,
				traces: &traces,
				overlays: vec![
					overlay(c.file_name(), "export const c: number = 2;\n", Some(&shared)),
					overlay(b.file_name(), "export const b = ;\n", None),
					overlay(a.file_name(), PREPARSE_DEPRECATED_USE, Some(&native)),
				],
			})
			.unwrap_or_else(|error| panic!("{error}"));
			(result, last_preparse())
		};

		// When: the serial control and the candidate apply the same overlays.
		let ((control_updated, control_remapped, control_composed), control_preparse) = run(control);
		let ((candidate_updated, candidate_remapped, candidate_composed), candidate_preparse) = run(candidate);

		// Then: the candidate matches the control file for file, in every ordered diagnostic list
		// and lazy JSDoc, in the remapped sources and in the traces.
		let control_shape = preparse_shape(control, control_updated);
		require_same_shape(&control_shape, &preparse_shape(candidate, candidate_updated));
		require_same_remap(
			control_updated,
			&control_remapped,
			candidate_updated,
			&candidate_remapped,
		);
		require_same_traces(
			&control_composed,
			&candidate_composed,
			&[&shared, &authored, &native, &prefix],
		);
		require_preparse_release(control_preparse.as_ref(), candidate_preparse.as_ref());
		for want in ["TS1109", "TS2322", "is deprecated", "jsdoc "] {
			assert!(
				control_shape.iter().any(|line| line.contains(want)),
				"fixture shape has no {want:?}; it no longer covers what it names"
			);
		}
		for name in ["src/dep.ts", "src/untouched.ts", "src/globals.d.ts"] {
			let path = format!("{dir}/{name}");
			assert!(
				candidate_updated.get_source_file(&path) == candidate.get_source_file(&path),
				"untouched {name} lost its parse tree"
			);
		}
	});
}

#[test]
fn preparse_text_overlay_update_matches_serial_control() {
	// Given: sidecar overlays that change sources and a declaration, repeat one source's text,
	// and name a source deleted from disk after load.
	let temp = write_preparse_project(&[
		("src/a.ts", "export const a = 1;\n"),
		("src/b.ts", "export const b = 1;\n"),
		("src/decl.d.ts", "export declare const decl: number;\n"),
		("src/gone.ts", "export const gone = 1;\n"),
		("src/same.ts", "export const same = 1;\n"),
		("src/untouched.ts", "export const untouched = 1;\n"),
	]);
	let dir = temp.dir();
	large_stack(|| {
		let [control, candidate] = load_preparse_programs(&dir);
		let gone = format!("{dir}/src/gone.ts");
		std::fs::remove_file(&gone).expect("remove gone.ts");
		let overlays = BTreeMap::from([
			(
				format!("{dir}/src/a.ts"),
				"export const a: number = \"x\" as unknown as string;\n".to_owned(),
			),
			(format!("{dir}/src/b.ts"), "export const b = ;\n".to_owned()),
			(
				format!("{dir}/src/decl.d.ts"),
				"export declare const decl: string;\n".to_owned(),
			),
			(format!("{dir}/src/same.ts"), "export const same = 1;\n".to_owned()),
			(gone, "export const gone = 2;\n".to_owned()),
		]);
		let run = |program: &'static Program| {
			let (updated, reused) =
				update_program_with_text_overlays(program, &overlays).unwrap_or_else(|error| panic!("{error}"));
			let preparse = last_preparse();
			assert!(reused, "text-only overlays rebuilt the Program");
			for name in ["src/same.ts", "src/untouched.ts"] {
				let path = format!("{dir}/{name}");
				assert!(
					updated.get_source_file(&path) == program.get_source_file(&path),
					"{name} lost its parse tree"
				);
			}
			(preparse, preparse_shape(program, updated))
		};

		// When: the serial control and the candidate apply the same overlays.
		let (control_preparse, control_shape) = run(control);
		let (candidate_preparse, candidate_shape) = run(candidate);

		// Then: both give the same files, texts, kept parse trees and diagnostics.
		require_same_shape(&control_shape, &candidate_shape);
		require_preparse_release(control_preparse.as_ref(), candidate_preparse.as_ref());
	});
}

#[test]
fn preparse_no_op_updates_keep_the_input_program() {
	// Given: overlays that change nothing.
	let temp = write_preparse_project(&[
		("src/a.ts", "export const a = 1;\n"),
		("src/b.ts", "export const b = 1;\n"),
	]);
	let dir = temp.dir();
	large_stack(|| {
		let [control, candidate] = load_preparse_programs(&dir);
		let same = BTreeMap::from([
			(format!("{dir}/src/a.ts"), "export const a = 1;\n".to_owned()),
			(format!("{dir}/src/b.ts"), "export const b = 1;\n".to_owned()),
		]);
		for program in [control, candidate] {
			// When/Then: each update returns its input program, sources and traces.
			for overlays in [BTreeMap::new(), same.clone()] {
				let result = update_program_with_text_overlays(program, &overlays);
				assert!(
					matches!(result, Ok((updated, true)) if std::ptr::eq(updated, program)),
					"no-op text update = {:?}, want input",
					result.map(|(_, reused)| reused)
				);
			}
			let sources = project_source_files(program);
			let traces = DiagnosticTraces(HashMap::from([(
				"prefix".to_owned(),
				trace("prefix.ts", "prefix", &[]),
			)]));
			let result = update_native_flamework_program(NativeProgramUpdate {
				program,
				source_files: &sources,
				traces: &traces,
				overlays: Vec::new(),
			});
			assert!(
				matches!(&result, Ok((updated, remapped, composed))
					if std::ptr::eq(*updated, program)
						&& *remapped == sources
						&& composed.0.len() == 1
						&& Arc::ptr_eq(&composed.0["prefix"], &traces.0["prefix"])),
				"no-op native update did not return its input"
			);
		}
	});
}

#[test]
fn preparse_rebuild_matches_serial_control() {
	let z_text = "import { late } from \"../lib/late\";\nexport const z = late + 1;\n";
	let scenarios: [RebuildScenario<'_>; 3] = [
		(
			"new import first",
			vec![
				("src/a.ts", PREPARSE_NEW_IMPORT),
				("src/m.ts", "export const m = 2;\n"),
				("src/z.ts", z_text),
			],
			"",
			true,
		),
		(
			"new reference in the middle",
			vec![
				("src/a.ts", "export const a = 2;\n"),
				(
					"src/m.ts",
					"/// <reference path=\"../lib/ref.d.ts\" />\nexport const m = 2;\n",
				),
				("src/z.ts", z_text),
			],
			"",
			false,
		),
		(
			"later changed file left out of the rebuilt graph",
			vec![
				("src/a.ts", PREPARSE_NEW_IMPORT),
				("lib/late.ts", "export const late = 2;\n"),
				("src/z.ts", "export const z = 2;\n"),
			],
			"lib/late.ts",
			false,
		),
	];
	for (name, scenario_overlays, absent, native) in scenarios {
		// Given: overlays that make the update rebuild the whole program.
		let temp = write_preparse_project(&PREPARSE_REBUILD_FILES);
		let dir = temp.dir();
		large_stack(|| {
			let [control, candidate] = load_preparse_programs(&dir);
			let overlays: BTreeMap<String, String> = scenario_overlays
				.iter()
				.map(|(file, text)| (format!("{dir}/{file}"), (*text).to_owned()))
				.collect();
			let run = |program: &'static Program| {
				let (updated, reused) =
					update_program_with_text_overlays(program, &overlays).unwrap_or_else(|error| panic!("{error}"));
				let preparse = last_preparse();
				assert!(!reused, "{name}: changed file shape did not rebuild the Program");
				assert!(
					absent.is_empty() || updated.get_source_file(&format!("{dir}/{absent}")).is_none(),
					"{name}: rebuilt graph kept {absent}"
				);
				(preparse, preparse_shape(program, updated))
			};

			// When: the serial control and the candidate apply the same overlays.
			let (control_preparse, control_shape) = run(control);
			let (candidate_preparse, candidate_shape) = run(candidate);

			// Then: both rebuild to the same program.
			require_same_shape(&control_shape, &candidate_shape);
			require_preparse_release(control_preparse.as_ref(), candidate_preparse.as_ref());
			if !native {
				return;
			}

			// And: on fresh programs, the native update rebuilds the same way.
			let [control, candidate] = load_preparse_programs(&dir);
			let native_run = |program: &'static Program| {
				let overlays = scenario_overlays
					.iter()
					.map(|(file, text)| overlay(preparse_source(program, &dir, file).file_name(), text, None))
					.collect();
				let sources = project_source_files(program);
				let result = update_native_flamework_program(NativeProgramUpdate {
					program,
					source_files: &sources,
					traces: &DiagnosticTraces::default(),
					overlays,
				})
				.unwrap_or_else(|error| panic!("{error}"));
				(result, last_preparse())
			};
			let ((control_updated, control_remapped, control_composed), control_preparse) = native_run(control);
			let ((candidate_updated, candidate_remapped, candidate_composed), candidate_preparse) =
				native_run(candidate);
			require_same_shape(
				&preparse_shape(control, control_updated),
				&preparse_shape(candidate, candidate_updated),
			);
			require_same_remap(
				control_updated,
				&control_remapped,
				candidate_updated,
				&candidate_remapped,
			);
			require_same_traces(&control_composed, &candidate_composed, &[]);
			require_preparse_release(control_preparse.as_ref(), candidate_preparse.as_ref());
		});
	}
}

#[test]
#[expect(
	clippy::significant_drop_tightening,
	reason = "the lock guards what the assertions read"
)]
fn preparse_rebuild_leaves_unrequested_parses_unused() {
	// Given: a changed file whose new import rebuilds the program, ahead of a changed file the
	// rebuilt graph keeps and one it leaves out.
	let temp = write_preparse_project(&PREPARSE_REBUILD_FILES);
	let dir = temp.dir();
	large_stack(|| {
		let program = load_preparse_program(&dir, false);
		let case_sensitive = program.use_case_sensitive_file_names();
		let (a, late, z) = (
			preparse_source(program, &dir, "src/a.ts"),
			preparse_source(program, &dir, "lib/late.ts"),
			preparse_source(program, &dir, "src/z.ts"),
		);
		let z_text = "export const z = 2;\n";
		let overlays = HashMap::from([
			(
				normalize_overlay_path(a.file_name(), case_sensitive),
				PREPARSE_NEW_IMPORT.to_owned(),
			),
			(
				normalize_overlay_path(late.file_name(), case_sensitive),
				"export const late = 2;\n".to_owned(),
			),
			(normalize_overlay_path(z.file_name(), case_sensitive), z_text.to_owned()),
		]);
		let changed = [a.path().clone(), late.path().clone(), z.path().clone()];
		let aliases = parallel_overlay_aliases(&overlays, case_sensitive);
		let mut host = new_native_overlay_host_with_aliases(
			Arc::clone(program.host()),
			program.host().fs(),
			overlays.clone(),
			aliases,
		);
		let preparsed = preparse_overlays(program, &overlays, &changed, case_sensitive)
			.map(Arc::new)
			.expect("the changed files were not all preparsed");
		let parsed: HashMap<Path, Option<P<SourceFile>>> = {
			let files = lock(&preparsed.files);
			let files = files.as_ref().expect("preparsed files");
			assert_eq!(files.len(), changed.len(), "the changed files were not all preparsed");
			files.iter().map(|(opts, job)| (opts.path.clone(), job.file)).collect()
		};
		host.preparsed = Some(Arc::clone(&preparsed));

		// When: the update runs with the preparsed host.
		let (updated, reused) = rotor_tsrs::project::update_program_files(program, &changed, Arc::new(host), None);

		// Then: the rebuild asked for a.ts again and parsed it fresh, took the z.ts parse, and
		// left the late.ts parse unused until release.
		assert!(!reused, "new import did not rebuild the Program");
		let got = updated.get_source_file(a.file_name());
		assert!(
			got.is_some_and(|got| Some(got) != parsed[a.path()] && got.text() == PREPARSE_NEW_IMPORT),
			"rebuild did not parse the requested-again a.ts fresh"
		);
		assert!(
			updated.get_source_file(z.file_name()) == parsed[z.path()],
			"rebuild did not take the preparsed z.ts"
		);
		assert!(
			updated.get_source_file(late.file_name()).is_none(),
			"rebuilt graph kept late.ts"
		);
		{
			let files = lock(&preparsed.files);
			let files = files.as_ref().expect("preparsed files");
			assert!(
				files.len() == 1 && files.contains_key(late.parse_options()),
				"unused preparsed files = {}, want late.ts only",
				files.len()
			);
		}
		preparsed.release();
		assert!(lock(&preparsed.files).is_none(), "release kept the unused parse");
		let got = updated.host().get_source_file(z.parse_options().clone());
		assert!(
			got.is_some_and(|got| Some(got) != parsed[z.path()] && got.text() == z_text),
			"retained host did not parse fresh after release"
		);
	});
}

#[test]
#[expect(clippy::too_many_lines, reason = "one Go test")]
fn preparse_missing_and_redirect_positions_match_serial_control() {
	let shared_package = r#"{"name":"shared","version":"1.0.0","types":"index.d.ts"}"#;
	let shared_text = "export declare class Shared {\n\tprivate brand: number;\n}\n";
	let files = [
		(
			"node_modules/left/index.d.ts",
			"import { Shared } from \"shared\";\nexport declare const left: Shared;\n",
		),
		("node_modules/left/node_modules/shared/index.d.ts", shared_text),
		("node_modules/left/node_modules/shared/package.json", shared_package),
		(
			"node_modules/left/package.json",
			r#"{"name":"left","version":"1.0.0","types":"index.d.ts"}"#,
		),
		(
			"node_modules/right/index.d.ts",
			"import { Shared } from \"shared\";\nexport declare const right: Shared;\n",
		),
		("node_modules/right/node_modules/shared/index.d.ts", shared_text),
		("node_modules/right/node_modules/shared/package.json", shared_package),
		(
			"node_modules/right/package.json",
			r#"{"name":"right","version":"1.0.0","types":"index.d.ts"}"#,
		),
		("src/a.ts", "export const a = 1;\n"),
		("src/b.ts", "export const b = 1;\n"),
		("src/c.ts", "export const c = 1;\n"),
		(
			"src/main.ts",
			"import { left } from \"left\";\nimport { right } from \"right\";\nexport const both = [left, right];\n",
		),
	];
	let (missing, target, source) = ("missing", "redirect target", "redirect source");
	for (changed_names, jobs) in [
		(vec![missing, "a", "b", "c"], 0),
		(vec!["a", "b", missing, "c"], 2),
		(vec!["a", "b", "c", missing], 3),
		(vec![target, "a", "b", "c"], 0),
		(vec!["a", "b", target, "c"], 2),
		(vec!["a", "b", source, "c"], 2),
		(vec!["a", "b", "c", source], 3),
	] {
		let name = changed_names.join(",");
		// Given: changed paths with a missing path or a package redirect at this position.
		let temp = write_preparse_project(&files);
		let dir = temp.dir();
		large_stack(|| {
			let [control, candidate] = load_preparse_programs(&dir);
			let run = |program: &'static Program, preparse: bool| {
				let case_sensitive = program.use_case_sensitive_file_names();
				// Package files resolve to real paths, so find the kept copy in the program rather
				// than spell its path from dir.
				let target_file = program
					.source_files()
					.iter()
					.copied()
					.rfind(|file| file.file_name().ends_with("/node_modules/shared/index.d.ts"));
				let target_file = target_file
					.filter(|file| program.get_redirect_targets(file.path()).len() == 1)
					.unwrap_or_else(|| panic!("{name}: fixture did not form a package redirect"));
				let source_name = program.get_redirect_targets(target_file.path())[0].clone();
				let source_path = tspath::to_path(&source_name, program.get_current_directory(), case_sensitive);
				assert!(
					source_path != *target_file.path()
						&& program.get_source_file_by_path(&source_path) == Some(target_file),
					"{name}: fixture redirect source does not map to its target"
				);
				let changed_shared = "export declare class Shared {\n\tprivate brand: string;\n}\n";
				let mut overlays = HashMap::from([
					(
						normalize_overlay_path(target_file.file_name(), case_sensitive),
						changed_shared.to_owned(),
					),
					(
						normalize_overlay_path(&source_name, case_sensitive),
						changed_shared.to_owned(),
					),
				]);
				let missing_path = tspath::to_path(
					&gopath::to_slash(&format!("{dir}/src/missing.ts")),
					program.get_current_directory(),
					case_sensitive,
				);
				let mut paths: HashMap<&str, Path> = HashMap::from([
					(missing, missing_path),
					(target, target_file.path().clone()),
					(source, source_path),
				]);
				for file_name in ["a", "b", "c"] {
					let file = preparse_source(program, &dir, &format!("src/{file_name}.ts"));
					paths.insert(file_name, file.path().clone());
					overlays.insert(
						normalize_overlay_path(file.file_name(), case_sensitive),
						format!("export const {file_name} = 2;\n"),
					);
				}
				let changed: Vec<Path> = changed_names.iter().map(|name| paths[name].clone()).collect();
				if preparse {
					let preparsed = preparse_overlays(program, &overlays, &changed, case_sensitive);
					let got = preparsed
						.as_ref()
						.map_or(0, |preparsed| lock(&preparsed.files).as_ref().map_or(0, HashMap::len));
					if let Some(preparsed) = &preparsed {
						preparsed.release();
					}
					assert_eq!(
						got, jobs,
						"{name}: preparse jobs (the paths before the first missing or redirect path)"
					);
				}
				let (updated, reused) = update_program_files_with_overlays(program, overlays, &changed);
				assert!(!reused, "{name}: missing or redirect path did not rebuild the Program");
				preparse_shape(program, updated)
			};

			// When: the serial control and the candidate apply the same update.
			let control_shape = run(control, false);
			let candidate_shape = run(candidate, true);

			// Then: both rebuild to the same program.
			require_same_shape(&control_shape, &candidate_shape);
		});
	}
}

#[test]
fn preparse_duplicate_and_retained_requests_match_serial_control() {
	// Given: a changed path that is listed twice.
	let temp = write_preparse_project(&[
		("src/a.ts", "export const a = 1;\n"),
		("src/b.ts", "export const b = 1;\n"),
		("src/c.ts", "export const c = 1;\n"),
	]);
	let dir = temp.dir();
	let a_text = "export const a = 2;\n";
	large_stack(|| {
		let [control, candidate] = load_preparse_programs(&dir);
		let run = |program: &'static Program| {
			let case_sensitive = program.use_case_sensitive_file_names();
			let (a, b) = (
				preparse_source(program, &dir, "src/a.ts"),
				preparse_source(program, &dir, "src/b.ts"),
			);
			let overlays = HashMap::from([
				(normalize_overlay_path(a.file_name(), case_sensitive), a_text.to_owned()),
				(
					normalize_overlay_path(b.file_name(), case_sensitive),
					"export const b = 2;\n".to_owned(),
				),
			]);

			// When: the update runs, then the host that the program keeps gets two more requests.
			let (updated, reused) = update_program_files_with_overlays(
				program,
				overlays,
				&[a.path().clone(), b.path().clone(), a.path().clone()],
			);
			let preparse = last_preparse();
			assert!(reused, "duplicate text-only update rebuilt the Program");
			let first = updated.host().get_source_file(a.parse_options().clone());
			let second = updated.host().get_source_file(a.parse_options().clone());

			// Then: each of these requests parses fresh from the overlay.
			let current = updated.get_source_file(a.file_name());
			assert!(
				matches!((first, second), (Some(first), Some(second))
					if first != second
						&& Some(first) != current
						&& Some(second) != current
						&& first.text() == a_text
						&& first.parse_options() == a.parse_options()),
				"retained host request did not parse fresh from the overlay"
			);
			(preparse, preparse_shape(program, updated))
		};
		let (control_preparse, control_shape) = run(control);
		let (candidate_preparse, candidate_shape) = run(candidate);
		require_same_shape(&control_shape, &candidate_shape);
		require_preparse_release(control_preparse.as_ref(), candidate_preparse.as_ref());
	});
}

#[test]
#[expect(
	clippy::significant_drop_tightening,
	reason = "the lock guards what the assertions read"
)]
fn preparse_overlays_parses_each_requestable_changed_file_once() {
	// Given: overlays for three program files, and a changed file without one.
	let temp = write_preparse_project(&[
		("src/a.ts", "export const a = 1;\n"),
		("src/b.ts", "export const b = 1;\n"),
		("src/c.ts", "export const c = 1;\n"),
		("src/d.ts", "export const d = 1;\n"),
	]);
	let dir = temp.dir();
	large_stack(|| {
		let program = load_preparse_program(&dir, false);
		let case_sensitive = program.use_case_sensitive_file_names();
		let mut files = HashMap::new();
		let mut overlays = HashMap::new();
		for name in ["a", "b", "c"] {
			let file = preparse_source(program, &dir, &format!("src/{name}.ts"));
			files.insert(name, file);
			overlays.insert(
				normalize_overlay_path(file.file_name(), case_sensitive),
				format!("export const {name} = 2;\n"),
			);
		}
		let (a, b, c, d) = (
			files["a"].path().clone(),
			files["b"].path().clone(),
			files["c"].path().clone(),
			preparse_source(program, &dir, "src/d.ts").path().clone(),
		);
		let missing = tspath::to_path(
			&gopath::to_slash(&format!("{dir}/src/missing.ts")),
			program.get_current_directory(),
			case_sensitive,
		);
		let scenarios: [PreparseScenario<'_>; 5] = [
			(
				"duplicate and no overlay before a missing path",
				vec![b.clone(), a.clone(), b.clone(), d, missing.clone(), c.clone()],
				Some(vec!["b", "a"]),
			),
			("all", vec![a.clone(), b.clone(), c], Some(vec!["a", "b", "c"])),
			("one job", vec![a.clone()], None),
			(
				"one job before a missing path",
				vec![a.clone(), missing.clone(), b.clone()],
				None,
			),
			("missing first", vec![missing, a, b], None),
		];
		for (name, changed, want) in scenarios {
			// When: the overlays preparse for the changed paths.
			let preparsed = preparse_overlays(program, &overlays, &changed, case_sensitive);

			// Then: each listed file parses once with its program options and overlay text,
			// nothing else parses, and one job gets no work group.
			let Some(want) = want else {
				assert!(preparsed.is_none(), "{name}: preparsed files, want none");
				continue;
			};
			let preparsed = preparsed.unwrap_or_else(|| panic!("{name}: preparsed files do not match {want:?}"));
			{
				let jobs = lock(&preparsed.files);
				let jobs = jobs.as_ref().expect("preparsed files");
				assert_eq!(jobs.len(), want.len(), "{name}: preparsed files do not match {want:?}");
				for file_name in want {
					let old = files[file_name];
					let text = &overlays[&normalize_overlay_path(old.file_name(), case_sensitive)];
					let job = jobs.get(old.parse_options());
					let file = job
						.filter(|job| job.panic.is_none() && job.text == *text)
						.and_then(|job| job.file)
						.filter(|file| *file != old)
						.unwrap_or_else(|| panic!("{name}: {file_name}.ts was not preparsed from its overlay"));
					assert!(
						file.text() == text
							&& file.parse_options() == old.parse_options()
							&& file.script_kind() == old.script_kind(),
						"{name}: {file_name}.ts preparse lost its text, parse options or script kind"
					);
				}
			}
			preparsed.release();
		}
	});
}

#[test]
fn preparse_take_needs_exact_options_and_text() {
	for case_sensitive in [true, false] {
		// Given: a host with one preparsed overlay and one physical alias.
		let file_name = "/project/src/Source.ts";
		let alias_name = "/physical/src/Source.ts";
		let text = "/** Source. */\nexport const source = 1;\n";
		let force = ExternalModuleIndicatorOptions {
			force: true,
			..ExternalModuleIndicatorOptions::default()
		};
		let options = |file_name: &str, path: &str, flags: ExternalModuleIndicatorOptions| SourceFileParseOptions {
			file_name: file_name.to_owned(),
			path: Path::from(path),
			external_module_indicator_options: flags,
		};
		let opts = options(file_name, file_name, force);
		let base = new_compiler_host(
			"/project",
			Arc::new(vfstest::from_map(std::iter::empty::<(&str, &str)>(), case_sensitive)),
			"",
			None,
			None,
			None,
		);
		let base: &'static Arc<dyn CompilerHost> = Box::leak(Box::new(base));
		let mut host = new_native_overlay_host_with_aliases(
			Arc::clone(base),
			base.fs(),
			HashMap::from([(normalize_overlay_path(file_name, case_sensitive), text.to_owned())]),
			HashMap::from([(normalize_overlay_path(alias_name, case_sensitive), text.to_owned())]),
		);
		let preparse = |host: &mut crate::flamework_program_update::NativeOverlayHost| {
			let mut job = OverlayParse {
				opts: opts.clone(),
				text: text.to_owned(),
				file: None,
				panic: None,
			};
			job.parse();
			let file = job.file;
			host.preparsed = Some(Arc::new(PreparsedOverlays {
				files: Mutex::new(Some(HashMap::from([(opts.clone(), job)]))),
			}));
			file
		};
		let require_fresh = |host: &crate::flamework_program_update::NativeOverlayHost,
		                     request: SourceFileParseOptions,
		                     preparsed: Option<P<SourceFile>>| {
			let got = host.get_source_file(request.clone());
			assert!(
				got.is_some_and(|got| Some(got) != preparsed && got.text() == text && *got.parse_options() == request),
				"caseSensitive={case_sensitive}: request {request:?} did not parse fresh from the overlay"
			);
		};

		// When/Then: only the first exact request takes the parse; the next one parses fresh.
		let preparsed = preparse(&mut host);
		assert!(
			host.get_source_file(opts.clone()) == preparsed,
			"exact request did not take the preparsed file"
		);
		require_fresh(&host, opts.clone(), preparsed);

		// When/Then: a request with another path, other module flags, another case or the alias
		// spelling parses fresh and leaves the parse alone.
		let preparsed = preparse(&mut host);
		let mut variants = vec![
			options(file_name, "/canonical/src/source.ts", force),
			options(file_name, file_name, ExternalModuleIndicatorOptions::default()),
			options(
				file_name,
				file_name,
				ExternalModuleIndicatorOptions { jsx: true, force: true },
			),
			options(alias_name, alias_name, force),
		];
		let lower = options("/project/src/source.ts", "/project/src/source.ts", force);
		if case_sensitive {
			assert!(
				host.get_source_file(lower).is_none(),
				"case-sensitive host read the overlay through another case"
			);
		} else {
			variants.push(lower);
		}
		for variant in variants {
			require_fresh(&host, variant, preparsed);
		}
		assert!(
			host.get_source_file(opts.clone()) == preparsed,
			"variant requests used the exact request's parse"
		);

		// When/Then: a text mismatch drops the parse, and so does release.
		let preparsed = preparse(&mut host);
		let current = host.preparsed.clone().expect("a preparse");
		assert!(
			current.take(&opts, "export const other = 1;\n").is_none(),
			"text mismatch took the preparsed file"
		);
		require_fresh(&host, opts.clone(), preparsed);
		let preparsed = preparse(&mut host);
		host.preparsed.as_ref().expect("a preparse").release();
		require_fresh(&host, opts.clone(), preparsed);
		// Go's nil `*preparsedOverlays` has no Rust spelling; a host without one is `None`.
	}
}

#[test]
fn preparse_take_and_release_are_safe_under_concurrent_requests() {
	// Given: preparsed overlays for several files.
	let new_preparsed = || {
		let mut jobs = Vec::with_capacity(4);
		let mut files = HashMap::with_capacity(4);
		for index in 0..4 {
			let file_name = format!("/project/src/file{index}.ts");
			let mut job = OverlayParse {
				opts: SourceFileParseOptions {
					file_name: file_name.clone(),
					path: Path::from(file_name.as_str()),
					..SourceFileParseOptions::default()
				},
				text: format!("export const value{index} = {index};\n"),
				file: None,
				panic: None,
			};
			job.parse();
			jobs.push((job.opts.clone(), job.text.clone(), job.file));
			files.insert(job.opts.clone(), job);
		}
		(
			Arc::new(PreparsedOverlays {
				files: Mutex::new(Some(files)),
			}),
			jobs,
		)
	};
	let request = |preparsed: &Arc<PreparsedOverlays>,
	               jobs: &[(SourceFileParseOptions, String, Option<P<SourceFile>>)],
	               release: bool| {
		let taken: Vec<AtomicI32> = jobs.iter().map(|_| AtomicI32::new(0)).collect();
		std::thread::scope(|scope| {
			for _ in 0..8 {
				for (index, (opts, text, want)) in jobs.iter().enumerate() {
					let taken = &taken;
					scope.spawn(move || {
						if let Some(file) = preparsed.take(opts, text) {
							assert!(Some(file) == *want, "request for {} took another file", opts.file_name);
							taken[index].fetch_add(1, Ordering::Relaxed);
						}
					});
				}
			}
			if release {
				scope.spawn(|| preparsed.release());
			}
		});
		taken
			.iter()
			.map(|count| count.load(Ordering::Relaxed))
			.collect::<Vec<_>>()
	};

	// When/Then: concurrent requests take each parse exactly once.
	let (preparsed, jobs) = new_preparsed();
	for (index, count) in request(&preparsed, &jobs, false).into_iter().enumerate() {
		assert_eq!(count, 1, "file{index} taken {count} times, want once");
	}

	// When/Then: requests that race with release take each parse at most once, and no request
	// after release takes one.
	let (preparsed, jobs) = new_preparsed();
	for (index, count) in request(&preparsed, &jobs, true).into_iter().enumerate() {
		assert!(count <= 1, "file{index} taken {count} times, want at most once");
	}
	for (opts, text, _) in &jobs {
		assert!(
			preparsed.take(opts, text).is_none(),
			"request after release took a file"
		);
	}
}

/// The message of a recovered panic.
fn panic_message(payload: &(dyn Any + Send)) -> String {
	payload
		.downcast_ref::<&str>()
		.map(|message| (*message).to_owned())
		.or_else(|| payload.downcast_ref::<String>().cloned())
		.unwrap_or_else(|| "<non-string panic>".to_owned())
}

/// `preparseRecover`: the message of the panic `run` raised, if any.
fn preparse_recover(run: impl FnOnce()) -> Option<String> {
	catch_unwind(AssertUnwindSafe(run))
		.err()
		.map(|payload| panic_message(&*payload))
}

#[test]
fn preparse_panic_raises_only_on_request() {
	// Given: an overlay parse that panics, as the parser does for a file name that is not
	// absolute.
	let opts = SourceFileParseOptions {
		file_name: "src/relative.ts".to_owned(),
		path: Path::from("src/relative.ts"),
		..SourceFileParseOptions::default()
	};
	let text = "export const relative = 1;\n";
	let want = preparse_recover(|| {
		rotor_tsrs::parser::parse_source_file(
			opts.clone(),
			text,
			rotor_tsrs::tscore::get_script_kind_from_file_name(&opts.file_name),
		);
	});
	assert!(
		want.is_some(),
		"parser accepted a relative file name; the fixture needs a parse panic"
	);
	let mut job = OverlayParse {
		opts: opts.clone(),
		text: text.to_owned(),
		file: None,
		panic: None,
	};

	// When: the work group runs the parse.
	job.parse();

	// Then: the job keeps the panic, and only a request for it raises the same value on the
	// requesting thread.
	assert!(
		job.file.is_none() && job.panic.as_deref().map(panic_message) == want,
		"job panic = {:?}, want the parser panic {want:?}",
		job.panic.as_deref().map(panic_message)
	);
	let base = new_compiler_host(
		"/project",
		Arc::new(vfstest::from_map(std::iter::empty::<(&str, &str)>(), true)),
		"",
		None,
		None,
		None,
	);
	let base: &'static Arc<dyn CompilerHost> = Box::leak(Box::new(base));
	let mut host = new_native_overlay_host_with_aliases(
		Arc::clone(base),
		base.fs(),
		HashMap::from([(normalize_overlay_path(&opts.file_name, true), text.to_owned())]),
		HashMap::new(),
	);
	host.preparsed = Some(Arc::new(PreparsedOverlays {
		files: Mutex::new(Some(HashMap::from([(opts.clone(), job)]))),
	}));
	let other = SourceFileParseOptions {
		file_name: "/project/src/other.ts".to_owned(),
		path: Path::from("/project/src/other.ts"),
		..SourceFileParseOptions::default()
	};
	let preparsed = host.preparsed.clone().expect("a preparse");
	let got = preparse_recover(|| {
		preparsed.take(&other, text);
	});
	assert!(got.is_none(), "request for another file raised {got:?}");
	let got = preparse_recover(|| {
		host.get_source_file(opts.clone());
	});
	assert_eq!(got, want, "request raised");
	let mut unused_job = OverlayParse {
		opts: opts.clone(),
		text: text.to_owned(),
		file: None,
		panic: None,
	};
	unused_job.parse();
	let unused = PreparsedOverlays {
		files: Mutex::new(Some(HashMap::from([(opts, unused_job)]))),
	};
	let got = preparse_recover(|| unused.release());
	assert!(got.is_none(), "release of an unused panicked parse raised {got:?}");
}

#[test]
fn preparse_compose_traces_matches_serial_composition() {
	// Given: traces that share pointers, absent traces, and a repeated file.
	let temp = write_preparse_project(&[("src/a.ts", "export const a = 1;\n")]);
	let dir = temp.dir();
	large_stack(|| {
		let [control, candidate] = load_preparse_programs(&dir);
		let inner = trace("inner.ts", "inner", &[(0, 3), (1, 5)]);
		let outer = trace("outer.ts", "outer", &[(0, 0), (4, 1)]);
		let last = trace("last.ts", "last", &[(2, 1)]);
		let prefix = trace("prefix.ts", "prefix", &[]);
		let (key_a, key_b, key_c) = (
			normalize_source_file_path("/project/a.ts"),
			normalize_source_file_path("/project/b.ts"),
			normalize_source_file_path("/project/c.ts"),
		);
		let traces = DiagnosticTraces(HashMap::from([
			(key_a.clone(), Arc::clone(&inner)),
			(key_b.clone(), Arc::clone(&inner)),
			("prefix".to_owned(), Arc::clone(&prefix)),
		]));
		let overlays = [
			overlay("/project/a.ts", "", Some(&outer)),
			overlay("/project/b.ts", "", None),
			overlay("/project/c.ts", "", Some(&outer)),
			overlay("/project/a.ts", "", Some(&last)),
		];
		let snapshot = || -> Vec<SourceTraceMap> {
			[&inner, &outer, &last, &prefix]
				.iter()
				.map(|trace| (***trace).clone())
				.collect()
		};
		let before = snapshot();
		// The loop the native update ran before: copy, then compose in order. Go stores a nil
		// composition; the port's traces hold none, so it drops the key.
		let mut want = traces.clone();
		for overlay in &overlays {
			let key = normalize_source_file_path(&overlay.file_name);
			match compose_source_trace_maps(overlay.trace.clone(), traces.0.get(&key).cloned()) {
				Some(trace) => want.0.insert(key, trace),
				None => want.0.remove(&key),
			};
		}

		for program in [control, candidate] {
			// When: the overlays compose in the program's work group.
			let got = compose_native_overlay_traces(program, &traces, &overlays);

			// Then: the values match the serial loop, pass-through traces keep their pointers, the
			// last overlay for a file wins, and the inputs stay as they were.
			assert_eq!(
				got.0,
				want.0,
				"composed traces differ from the serial loop (single-threaded {})",
				program.single_threaded()
			);
			assert!(
				Arc::ptr_eq(&got.0[&key_b], &inner)
					&& Arc::ptr_eq(&got.0[&key_c], &outer)
					&& Arc::ptr_eq(&got.0["prefix"], &prefix)
					&& !Arc::ptr_eq(&got.0[&key_a], &inner)
					&& !Arc::ptr_eq(&got.0[&key_a], &outer)
					&& !Arc::ptr_eq(&got.0[&key_a], &last),
				"composed traces lost pass-through identity or did not compose"
			);
			assert_eq!(
				Some(&got.0[&key_a]),
				compose_source_trace_maps(Some(Arc::clone(&last)), Some(Arc::clone(&inner))).as_ref(),
				"repeated file did not keep its last overlay's trace"
			);
			assert!(
				traces.0.len() == 3
					&& Arc::ptr_eq(&traces.0[&key_a], &inner)
					&& Arc::ptr_eq(&traces.0[&key_b], &inner)
					&& Arc::ptr_eq(&traces.0["prefix"], &prefix)
					&& snapshot() == before,
				"composition changed its inputs"
			);
		}
	});
}

#[test]
fn preparse_aliases_match_serial_aliases() {
	// Given: overlays for existing, missing and relative paths, and for a path through a
	// symlinked parent when the platform permits one.
	let temp = Temp::new();
	let dir = temp.dir();
	for name in ["one.ts", "two.ts", "sub/three.ts", "five.ts"] {
		temp.write(name, "export {};\n");
	}
	let mut overlays: HashMap<String, String> = HashMap::from([
		(format!("{dir}/one.ts"), "one".to_owned()),
		(format!("{dir}/two.ts"), "two".to_owned()),
		(format!("{dir}/sub/three.ts"), "three".to_owned()),
		(format!("{dir}/missing.ts"), "missing".to_owned()),
		(gopath::join(&["relative", "four.ts"]), "four".to_owned()),
	]);
	let link_temp = Temp::new();
	let linked = link_temp.join("linked");
	#[cfg(unix)]
	let symlinked = std::os::unix::fs::symlink(&dir, &linked);
	#[cfg(windows)]
	let symlinked = std::os::windows::fs::symlink_dir(&dir, &linked);
	if symlinked.is_ok() {
		overlays.insert(linked.join("five.ts").to_string_lossy().into_owned(), "five".to_owned());
	}
	let compare = |overlays: &HashMap<String, String>| -> HashMap<String, String> {
		let mut serial = HashMap::new();
		for case_sensitive in [true, false] {
			let normalized: HashMap<String, String> = overlays
				.iter()
				.map(|(path, text)| (normalize_overlay_path(path, case_sensitive), text.clone()))
				.collect();
			let ordered: BTreeMap<String, String> = normalized.clone().into_iter().collect();
			let got = parallel_overlay_aliases(&normalized, case_sensitive);
			let want = overlay_aliases(&ordered, "", case_sensitive);
			assert_eq!(
				got, want,
				"parallel aliases differ from serial aliases (case-sensitive {case_sensitive})"
			);
			if case_sensitive {
				serial = want;
			}
		}
		serial
	};

	// When/Then: the parallel aliases equal the serial ones in both case modes.
	compare(&overlays);

	// When/Then: changing the working directory preserves the serial result. The Windows
	// realpath wrapper makes "." absolute; on other platforms it stays relative, so the serial
	// code cannot make a relative alias from it.
	let previous = std::env::current_dir().expect("the working directory");
	std::env::set_current_dir(&dir).expect("chdir");
	let aliases = std::panic::catch_unwind(AssertUnwindSafe(|| compare(&overlays)));
	std::env::set_current_dir(previous).expect("chdir back");
	let aliases = aliases.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
	let relative = aliases.get(&normalize_overlay_path("one.ts", true));
	assert!(
		relative.is_some() == cfg!(windows) && relative.is_none_or(|text| text == "one"),
		"working-directory relative alias = {relative:?}"
	);

	// And: zero or one overlay gives the serial result.
	for small in [
		HashMap::new(),
		HashMap::from([(format!("{dir}/one.ts"), "one".to_owned())]),
	] {
		let ordered: BTreeMap<String, String> = small.clone().into_iter().collect();
		assert_eq!(
			parallel_overlay_aliases(&small, true),
			overlay_aliases(&ordered, "", true),
			"small overlay sets differ from serial aliases"
		);
	}
}

#[test]
fn preparse_alias_merge_keeps_entry_order() {
	// Given: two spellings of one physical file with different texts.
	let link = normalize_overlay_path("/project/link/a.ts", true);
	let physical = normalize_overlay_path("/project/real/a.ts", true);
	let relative = normalize_overlay_path(&gopath::join(&["real", "a.ts"]), true);
	let entry = |path: &str, text: &str| OverlayAliasEntry {
		path: path.to_owned(),
		text: text.to_owned(),
		physical: Some(physical.clone()),
		panic: None,
	};
	let canonical = gopath::from_slash("/project").into_owned();
	let map = |entries: &[(&str, &str)]| -> HashMap<String, String> {
		entries
			.iter()
			.map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
			.collect()
	};
	let scenarios = [
		(
			"first then second",
			[(link.as_str(), "first"), (physical.as_str(), "second")],
			None,
			map(&[(&link, "first"), (&physical, "second")]),
		),
		(
			"second then first",
			[(physical.as_str(), "second"), (link.as_str(), "first")],
			None,
			map(&[(&link, "first"), (&physical, "first")]),
		),
		(
			"first then second under the config directory",
			[(link.as_str(), "first"), (physical.as_str(), "second")],
			Some(canonical.as_str()),
			map(&[(&link, "first"), (&physical, "second"), (&relative, "second")]),
		),
		(
			"second then first under the config directory",
			[(physical.as_str(), "second"), (link.as_str(), "first")],
			Some(canonical.as_str()),
			map(&[(&link, "first"), (&physical, "first"), (&relative, "first")]),
		),
	];
	for (name, entries, canonical, want) in scenarios {
		// When: the entries merge in this snapshot order.
		let entries = entries.iter().map(|(path, text)| entry(path, text)).collect();
		let got = merge_overlay_aliases(entries, ".", canonical, true);

		// Then: the later entry wins each alias that both write, as the serial loop does when it
		// visits the overlays in this order.
		assert_eq!(got, want, "{name}: aliases");
	}

	// When/Then: a stored lookup panic raises at its entry.
	let failed = OverlayAliasEntry {
		path: normalize_overlay_path("/project/failed.ts", true),
		text: "failed".to_owned(),
		physical: None,
		panic: Some(Box::new("lookup failed")),
	};
	let got = preparse_recover(|| {
		merge_overlay_aliases(
			vec![entry(&link, "first"), failed, entry(&physical, "second")],
			".",
			Some(&canonical),
			true,
		);
	});
	assert_eq!(
		got.as_deref(),
		Some("lookup failed"),
		"merge raised, want the stored lookup panic"
	);
}

#[test]
fn preparse_missing_source_errors_match_serial_control() {
	// Given: overlays that name a source the program does not hold.
	let temp = write_preparse_project(&[
		("src/a.ts", "export const a = 1;\n"),
		("src/b.ts", "export const b = 1;\n"),
	]);
	let dir = temp.dir();
	large_stack(|| {
		let [control, candidate] = load_preparse_programs(&dir);
		let missing = format!("{dir}/src/missing.ts");
		for program in [control, candidate] {
			let a = preparse_source(program, &dir, "src/a.ts");

			// When/Then: both updates fail with the same error.
			let sources = project_source_files(program);
			let error = update_native_flamework_program(NativeProgramUpdate {
				program,
				source_files: &sources,
				traces: &DiagnosticTraces::default(),
				overlays: vec![
					overlay(a.file_name(), "export const a = 2;\n", None),
					overlay(&missing, "export {};\n", None),
				],
			})
			.err()
			.map(|error| error.to_string());
			assert_eq!(
				error,
				Some(format!(
					"compile: native Flamework source missing from Program: {missing}"
				)),
				"native update error"
			);
			let overlays = BTreeMap::from([
				(format!("{dir}/src/a.ts"), "export const a = 2;\n".to_owned()),
				(missing.clone(), "export {};\n".to_owned()),
			]);
			let error = update_program_with_text_overlays(program, &overlays)
				.err()
				.map(|error| error.to_string());
			assert_eq!(
				error,
				Some(format!("compile: overlay source missing from Program: {missing}")),
				"text update error"
			);
		}
	});
}

/// `writePreparseProject`: a project whose src holds the globals stub and `files`, named
/// relative to the project root.
fn write_preparse_project(files: &[(&str, &str)]) -> Temp {
	let temp = write_project("@scope/preparse-update", "");
	std::fs::remove_file(temp.join("src/main.ts")).expect("remove main.ts");
	for (name, text) in files {
		temp.write(name, text);
	}
	temp
}

/// `loadPreparsePrograms`: the serial control and the candidate, as fresh programs from `dir`.
fn load_preparse_programs(dir: &str) -> [&'static Program; 2] {
	[load_preparse_program(dir, true), load_preparse_program(dir, false)]
}

/// `loadPreparseProgram`: a fresh program with its own host and one checker, so the two programs
/// of a test differ only in threading.
fn load_preparse_program(dir: &str, single_threaded: bool) -> &'static Program {
	let (_, program) = new_project_program_with_options(
		dir,
		"",
		&ProjectOptions {
			checkers: Some(1),
			single_threaded: Some(single_threaded),
			..ProjectOptions::default()
		},
	)
	.unwrap_or_else(|failure| {
		panic!(
			"newProjectProgramWithOptions: {} ({:?})",
			failure.error,
			failure.messages()
		)
	});
	assert_eq!(program.single_threaded(), single_threaded, "Program.SingleThreaded()");
	program
}

/// `preparseSource`
fn preparse_source(program: &Program, dir: &str, name: &str) -> P<SourceFile> {
	program
		.get_source_file(&format!("{dir}/{name}"))
		.unwrap_or_else(|| panic!("fixture source missing from Program: {name}"))
}

/// `preparseShape`: an updated program against the program it came from. It queries the
/// diagnostics first, in a fixed order, then lists each file in program order with its parse
/// options, script kind, text, whether the old parse tree stayed, its parse diagnostics and the
/// lazy `JSDoc` of its statements.
fn preparse_shape(old: &'static Program, updated: &'static Program) -> Vec<String> {
	let ctx = Context::default();
	let mut shape = vec![format!("files {}", updated.source_files().len())];
	shape.extend(preparse_diagnostics(
		"semantic",
		&updated.get_semantic_diagnostics(&ctx, None),
	));
	shape.extend(preparse_diagnostics(
		"suggestion",
		&updated.get_suggestion_diagnostics(&ctx, None),
	));
	shape.extend(preparse_diagnostics(
		"syntactic",
		&updated.get_syntactic_diagnostics(&ctx, None),
	));
	shape.extend(preparse_diagnostics("global", &updated.get_global_diagnostics(&ctx)));
	shape.extend(preparse_diagnostics("program", &updated.get_program_diagnostics()));
	for &file in updated.source_files() {
		let kept = old.get_source_file_by_path(file.path()) == Some(file);
		shape.push(format!(
			"file {} opts={:?} kind={:?} declaration={} kept={kept} text={:?}",
			file.file_name(),
			file.parse_options(),
			file.script_kind(),
			file.is_declaration_file(),
			file.text()
		));
		shape.extend(preparse_diagnostics("parse", file.diagnostics()));
		for (index, statement) in file.statements().nodes().iter().enumerate() {
			for jsdoc in statement.jsdoc(Some(file.get())) {
				shape.push(format!(
					"jsdoc {} statement={index} kind={:?} range={}-{}",
					file.file_name(),
					jsdoc.kind(),
					jsdoc.pos(),
					jsdoc.end()
				));
			}
		}
	}
	shape
}

/// `preparseDiagnostics`
fn preparse_diagnostics(label: &str, diagnostics: &[P<Diagnostic>]) -> Vec<String> {
	diagnostics
		.iter()
		.map(|diagnostic| {
			let file_name = diagnostic
				.file()
				.map_or_else(|| "<none>".to_owned(), |file| file.file_name().to_owned());
			format!(
				"{label} {file_name} {:?}+{:?} {:?} TS{} {}",
				diagnostic.pos(),
				diagnostic.len(),
				diagnostic.category(),
				diagnostic.code(),
				diagnostic.localize()
			)
		})
		.collect()
}

/// `requireSameShape`
fn require_same_shape(control: &[String], candidate: &[String]) {
	assert!(
		control == candidate,
		"candidate differs from the serial control\ncontrol:\n{}\ncandidate:\n{}",
		control.join("\n"),
		candidate.join("\n")
	);
}

/// `requireSameRemap`
fn require_same_remap(
	control_program: &Program,
	control: &[P<SourceFile>],
	candidate_program: &Program,
	candidate: &[P<SourceFile>],
) {
	assert_eq!(control.len(), candidate.len(), "remapped sources: control, candidate");
	for (index, (control, candidate)) in control.iter().zip(candidate).enumerate() {
		assert_eq!(
			control.file_name(),
			candidate.file_name(),
			"remapped source[{index}]: control, candidate"
		);
		assert!(
			control_program.get_source_file(control.file_name()) == Some(*control)
				&& candidate_program.get_source_file(candidate.file_name()) == Some(*candidate),
			"remapped source[{index}] is not the updated Program's file"
		);
	}
}

/// `requireSameTraces`: composed traces compared by value; a trace that passes through from the
/// inputs must stay the same pointer.
fn require_same_traces(control: &DiagnosticTraces, candidate: &DiagnosticTraces, inputs: &[&Arc<SourceTraceMap>]) {
	assert_eq!(
		control.0, candidate.0,
		"candidate composed traces differ from the serial control"
	);
	for (key, trace) in &control.0 {
		let passed_through = inputs.iter().any(|input| Arc::ptr_eq(input, trace));
		assert_eq!(
			passed_through,
			candidate
				.0
				.get(key)
				.is_some_and(|candidate| Arc::ptr_eq(candidate, trace)),
			"trace {key}: pass-through identity differs from the serial control"
		);
	}
}

/// `requirePreparseRelease`: the overlay host each updated program keeps. The serial control has
/// no preparse, and the candidate's is released.
fn require_preparse_release(control: Option<&LastPreparse>, candidate: Option<&LastPreparse>) {
	assert!(
		matches!(control, Some(None)),
		"serial control host has a preparse or is not the overlay host"
	);
	assert!(
		matches!(candidate, Some(Some(preparsed)) if lock(&preparsed.files).is_none()),
		"candidate host has no released preparse"
	);
}
