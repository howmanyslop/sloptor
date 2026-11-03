//! `flamework_sparse_update_test.go`

use std::collections::HashMap;
use std::sync::Arc;

use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::Program;

use super::task7_acceptance::task7_flamework_project;
use super::{large_stack, write_project};
use crate::flamework_pipeline::{apply_native_flamework_transform, prepare_flamework_pipeline};
use crate::flamework_program_update::{
	NativeProgramUpdate, NativeSourceOverlay, PROGRAM_UPDATES, update_native_flamework_program,
};
use crate::incremental_manifest::normalize_source_file_path;
use crate::project::{ProjectOptions, new_project_program_with_options, project_source_files};
use crate::source_trace::{DiagnosticTraces, SourcePosition, SourceTraceMap};

fn program_for(dir: &str) -> (String, &'static Program) {
	new_project_program_with_options(dir, "", &ProjectOptions::default())
		.unwrap_or_else(|failure| panic!("newProjectProgram: {} ({:?})", failure.error, failure.messages()))
}

fn prefix_traces() -> DiagnosticTraces {
	DiagnosticTraces(HashMap::from([(
		"unchanged-prefix".to_owned(),
		Arc::new(SourceTraceMap::for_test("unchanged.ts", "prefix", &[])),
	)]))
}

fn overlay(file_name: &str, text: &str) -> NativeSourceOverlay {
	NativeSourceOverlay {
		file_name: file_name.to_owned(),
		text: text.to_owned(),
		trace: None,
		rebase: None,
	}
}

fn source(program: &Program, name: &str) -> P<SourceFile> {
	program
		.get_source_file(name)
		.unwrap_or_else(|| panic!("{name} missing from the program"))
}

fn text(program: &Program, name: &str) -> String {
	source(program, name).text().to_owned()
}

fn program_updates() -> usize {
	PROGRAM_UPDATES.with(std::cell::Cell::get)
}

fn reset_program_updates() {
	PROGRAM_UPDATES.with(|updates| updates.set(0));
}

#[test]
fn native_flamework_sparse_update_reprints_every_source_by_default_when_no_structural_change_occurs() {
	// Given: a native Flamework project whose ordinary sources need no structural transform.
	let temp = task7_flamework_project("[flamework]\n");
	large_stack(|| {
		let (dir, program) = program_for(&temp.dir());
		let pipeline = match prepare_flamework_pipeline(&dir, program, &ProjectOptions::default()) {
			Ok(Some(pipeline)) => pipeline,
			Ok(None) => panic!("prepareFlameworkPipeline returned nil pipeline"),
			Err(failure) => panic!("prepareFlameworkPipeline: {} ({:?})", failure.error, failure.messages()),
		};
		let source_files = project_source_files(program);
		let traces = prefix_traces();

		// When: the native stage runs with the default upstream-parity behavior.
		let prepared =
			apply_native_flamework_transform(program, &source_files, &traces, &pipeline).unwrap_or_else(|failure| {
				panic!(
					"applyNativeFlameworkTransform: {} ({:?})",
					failure.error,
					failure.messages()
				)
			});
		// Then: every source is reprinted and overlaid, so the compiler sees fresh parse trees.
		assert!(
			!std::ptr::eq(prepared.program, program),
			"default native transform reused the incoming Program; want print/overlay/reparse"
		);
		assert_eq!(
			prepared.source_files.len(),
			source_files.len(),
			"default native source count"
		);
		for (index, source_file) in source_files.iter().enumerate() {
			assert!(
				prepared.source_files[index] != *source_file,
				"default native source[{index}], want fresh parse tree"
			);
			assert!(
				prepared
					.source_traces
					.0
					.contains_key(&normalize_source_file_path(source_file.file_name())),
				"default native source[{index}] missing composed trace"
			);
		}
		assert!(
			Arc::ptr_eq(
				&prepared.source_traces.0["unchanged-prefix"],
				&traces.0["unchanged-prefix"]
			),
			"default native replaced the incoming prefix trace"
		);
	});
}

#[test]
fn native_flamework_sparse_update_reuses_incoming_program_and_files_when_skip_unchanged_files_is_enabled() {
	// Given: a native Flamework project opting into identity reuse.
	let temp = task7_flamework_project("[flamework]\nskipUnchangedFiles = true\n");
	large_stack(|| {
		let (dir, program) = program_for(&temp.dir());
		let pipeline = match prepare_flamework_pipeline(&dir, program, &ProjectOptions::default()) {
			Ok(Some(pipeline)) => pipeline,
			Ok(None) => panic!("prepareFlameworkPipeline returned nil pipeline"),
			Err(failure) => panic!("prepareFlameworkPipeline: {} ({:?})", failure.error, failure.messages()),
		};
		let source_files = project_source_files(program);
		let traces = prefix_traces();

		// When: the native stage runs with skipUnchangedFiles enabled.
		let prepared =
			apply_native_flamework_transform(program, &source_files, &traces, &pipeline).unwrap_or_else(|failure| {
				panic!(
					"applyNativeFlameworkTransform: {} ({:?})",
					failure.error,
					failure.messages()
				)
			});
		// Then: the opt-in no-op path preserves the incoming compiler state.
		assert!(
			std::ptr::eq(prepared.program, program),
			"native opt-in Program, want incoming"
		);
		assert_eq!(
			prepared.source_files, source_files,
			"native opt-in sources, want incoming"
		);
		assert!(
			Arc::ptr_eq(
				&prepared.source_traces.0["unchanged-prefix"],
				&traces.0["unchanged-prefix"]
			),
			"native opt-in replaced the incoming prefix trace"
		);
	});
}

/// `sparseNativeProgram`: the project directory, the program and its source files. Go writes the
/// sources in map order; their order does not reach the program, which sorts its root files.
fn sparse_native_program(sources: &[(&str, &str)]) -> (super::Temp, String, &'static Program, Vec<P<SourceFile>>) {
	let temp = write_project("@scope/sparse-native-update", "");
	std::fs::remove_file(temp.join("src/main.ts")).expect("remove main.ts");
	for (name, text) in sources {
		temp.write(&format!("src/{name}"), text);
	}
	let (_, program) = program_for(&temp.dir());
	let dir = temp.dir();
	(temp, dir, program, project_source_files(program))
}

/// `remappedSource`
fn remapped_source(source_files: &[P<SourceFile>], file_name: &str) -> Option<P<SourceFile>> {
	source_files.iter().copied().find(|file| file.file_name() == file_name)
}

#[test]
fn native_flamework_sparse_update_updates_only_changed_sources_in_stable_source_order() {
	large_stack(|| {
		// Given: two changed overlays arrive in reverse order around one unchanged source.
		let (_temp, dir, program, source_files) = sparse_native_program(&[
			("a.ts", "export const a = 1;\n"),
			("b.ts", "export const b = 1;\n"),
			("c.ts", "export const c = 1;\n"),
		]);
		let a = source(program, &format!("{dir}/src/a.ts"));
		let b = source(program, &format!("{dir}/src/b.ts"));
		let c = source(program, &format!("{dir}/src/c.ts"));
		let authored = Arc::new(SourceTraceMap::for_test(a.file_name(), "authored-a", &[(0, 0, 4, 0)]));
		let native = Arc::new(SourceTraceMap::for_test(a.file_name(), a.text(), &[(0, 0, 0, 0)]));
		let traces = DiagnosticTraces(HashMap::from([
			(normalize_source_file_path(a.file_name()), authored),
			(
				normalize_source_file_path(b.file_name()),
				Arc::new(SourceTraceMap::for_test(b.file_name(), "unchanged-prefix", &[])),
			),
		]));

		// When: the sparse updater applies reverse-ordered overlays.
		let (updated, remapped, composed) = update_native_flamework_program(NativeProgramUpdate {
			program,
			source_files: &source_files,
			traces: &traces,
			overlays: vec![
				overlay(c.file_name(), "export const c = 2;\n"),
				NativeSourceOverlay {
					trace: Some(native),
					..overlay(a.file_name(), "export const a = 2;\n")
				},
			],
		})
		.unwrap_or_else(|error| panic!("{error}"));
		// Then: changed text is reparsed, while the middle file and its prefix trace retain
		// identity.
		assert!(
			!std::ptr::eq(updated, program),
			"sparse changed update reused the incoming Program"
		);
		assert!(
			source(updated, a.file_name()) != a && text(updated, a.file_name()).contains("a = 2"),
			"a.ts did not receive its native overlay"
		);
		assert!(
			source(updated, c.file_name()) != c && text(updated, c.file_name()).contains("c = 2"),
			"c.ts did not receive its native overlay"
		);
		assert!(
			source(updated, b.file_name()) == b,
			"unchanged b.ts parse tree was not reused"
		);
		assert!(
			remapped_source(&remapped, b.file_name()) == Some(b),
			"unchanged b.ts was not retained in the compile source list"
		);
		let b_key = normalize_source_file_path(b.file_name());
		assert!(
			Arc::ptr_eq(&composed.0[&b_key], &traces.0[&b_key]),
			"unchanged b.ts prefix trace was replaced"
		);
		let position = composed.0[&normalize_source_file_path(a.file_name())]
			.original_position(SourcePosition { line: 0, column: 0 });
		assert!(
			position.is_some_and(|position| position.line == 4),
			"changed a.ts trace position = {position:?}, want authored line 4"
		);
	});
}

#[test]
fn native_flamework_sparse_update_batches_compatible_overlays_into_one_program_reconstruction() {
	large_stack(|| {
		// Given: three shape-compatible text overlays that would previously each rebuild a
		// Program.
		let (_temp, dir, program, source_files) = sparse_native_program(&[
			("a.ts", "export const a = 1;\n"),
			("b.ts", "export const b = 1;\n"),
			("c.ts", "export const c = 1;\n"),
		]);
		reset_program_updates();
		let a = source(program, &format!("{dir}/src/a.ts"));
		let b = source(program, &format!("{dir}/src/b.ts"));
		let c = source(program, &format!("{dir}/src/c.ts"));

		// When: three reverse-ordered compatible overlays apply together.
		let (updated, _, _) = update_native_flamework_program(NativeProgramUpdate {
			program,
			source_files: &source_files,
			traces: &DiagnosticTraces::default(),
			overlays: vec![
				overlay(c.file_name(), "export const c = 2;\n"),
				overlay(b.file_name(), "export const b = 2;\n"),
				overlay(a.file_name(), "export const a = 2;\n"),
			],
		})
		.unwrap_or_else(|error| panic!("{error}"));
		// Then: one Program reconstruction covers every overlay.
		let creations = program_updates();
		assert_eq!(
			creations, 1,
			"compatible multi-overlay update created {creations} Programs, want one batched reconstruction"
		);
		assert!(
			source(updated, a.file_name()) != a
				&& source(updated, b.file_name()) != b
				&& source(updated, c.file_name()) != c,
			"batched compatible update retained an incoming changed parse tree"
		);
		assert!(
			text(updated, a.file_name()).contains("a = 2")
				&& text(updated, b.file_name()).contains("b = 2")
				&& text(updated, c.file_name()).contains("c = 2"),
			"batched compatible update did not expose every native overlay"
		);
	});
}

#[test]
fn native_flamework_sparse_update_reuses_unchanged_files_when_import_targets_already_loaded() {
	large_stack(|| {
		// Given: Flamework-like output that adds an import of a module already in the Program.
		let (_temp, dir, program, source_files) = sparse_native_program(&[
			("a.ts", "export const a = 1;\n"),
			("b.ts", "export const b = 1;\n"),
			("dep.ts", "export const value = 1;\n"),
		]);
		reset_program_updates();
		let a = source(program, &format!("{dir}/src/a.ts"));
		let b = source(program, &format!("{dir}/src/b.ts"));
		let dep = source(program, &format!("{dir}/src/dep.ts"));

		// When: a.ts gains an import of already-loaded dep and b.ts only changes text.
		let (updated, _, _) = update_native_flamework_program(NativeProgramUpdate {
			program,
			source_files: &source_files,
			traces: &DiagnosticTraces::default(),
			overlays: vec![
				overlay(b.file_name(), "export const b = 2;\n"),
				overlay(
					a.file_name(),
					"import { value } from \"./dep\"; export const a = value;\n",
				),
			],
		})
		.unwrap_or_else(|error| panic!("{error}"));
		// Then: one sparse reconstruction reuses untouched parse trees instead of full rebuild.
		let creations = program_updates();
		assert_eq!(
			creations, 1,
			"already-loaded import update created {creations} Programs, want one sparse reconstruction"
		);
		assert!(
			source(updated, dep.file_name()) == dep,
			"already-loaded import update reparsed untouched dep.ts"
		);
		assert!(
			source(updated, a.file_name()) != a && text(updated, a.file_name()).contains("from \"./dep\""),
			"a.ts did not receive its import-bearing overlay"
		);
		assert!(
			source(updated, b.file_name()) != b && text(updated, b.file_name()).contains("b = 2"),
			"b.ts did not receive its text overlay"
		);
	});
}

#[test]
fn native_flamework_sparse_update_falls_back_once_with_every_overlay_when_new_module_is_required() {
	large_stack(|| {
		// Given: a new on-disk module is not part of the current Program file set.
		let (temp, dir, program, source_files) =
			sparse_native_program(&[("a.ts", "export const a = 1;\n"), ("b.ts", "export const b = 1;\n")]);
		// Go writes it before rebuilding the program to hook its pool, from the config parsed
		// without it; the port's program is that program.
		temp.write("src/extra.ts", "export const extra = 1;\n");
		reset_program_updates();
		let a = source(program, &format!("{dir}/src/a.ts"));
		let b = source(program, &format!("{dir}/src/b.ts"));
		assert!(
			program.get_source_file(&format!("{dir}/src/extra.ts")).is_none(),
			"extra.ts was unexpectedly already part of the Program"
		);

		// When: a.ts imports the unloaded module and b.ts also has overlay text.
		let (updated, _, _) = update_native_flamework_program(NativeProgramUpdate {
			program,
			source_files: &source_files,
			traces: &DiagnosticTraces::default(),
			overlays: vec![
				overlay(b.file_name(), "export const b = 2;\n"),
				overlay(
					a.file_name(),
					"import { extra } from \"./extra\"; export const a = extra;\n",
				),
			],
		})
		.unwrap_or_else(|error| panic!("{error}"));
		// Then: one full rebuild includes every overlay and loads the new module graph.
		let creations = program_updates();
		assert_eq!(
			creations, 1,
			"new-module update created {creations} Programs, want one full fallback"
		);
		assert!(
			source(updated, a.file_name()) != a && source(updated, b.file_name()) != b,
			"new-module fallback retained an incoming changed parse tree"
		);
		assert!(
			text(updated, a.file_name()).contains("from \"./extra\"") && text(updated, b.file_name()).contains("b = 2"),
			"new-module fallback did not expose every native overlay"
		);
		assert!(
			updated.get_source_file(&format!("{dir}/src/extra.ts")).is_some(),
			"new-module fallback did not load extra.ts into the Program"
		);
	});
}
