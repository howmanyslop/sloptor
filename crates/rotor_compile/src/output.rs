//! `output.go`.
//!
//! The disk-writing build ([`build_project_with_options`]): cleanup, the include copy, the
//! non-compiled file copy, the compile, then the compiled outputs, source maps and declarations,
//! written in parallel, and the incremental manifest.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rotor_rojo::{PathTranslator, RojoResolverCache};
use rotor_support::{fsutil, gopath, gostr, logservice};
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::EmitOnly;
use rotor_tsrs::compiler::Program;
use rotor_tsrs::{Context, P, tspath};

use crate::compile::{DiagnosticInfo, Error, Failure, diagnostic_info_messages, ts_diagnostic_infos};
use crate::copyfilesgate::{
	CopyFilesGateInputs, copy_files_changed_snapshot, is_protected_output_filename, load_copy_files_gate_pre_build,
};
use crate::declaration_prefetch::DeclarationPrefetch;
use crate::declmap::shift_declaration_map_columns;
use crate::declpaths::{DeclarationPathRewriter, TextEdit, apply_text_edits, non_overlapping_text_edits};
use crate::flamework_pipeline::{FlameworkPipeline, prepare_flamework_pipeline, run_compile_pipeline_with_options};
use crate::incremental_flamework::flamework_incremental_inputs;
use crate::incremental_manifest::{
	IncrementalManifest, build_incremental_manifest, encode_incremental_manifest, normalize_source_file_path,
	prune_missing_outputs, read_incremental_manifest, same_incremental_manifest, select_incremental_source_files,
	write_incremental_manifest, write_incremental_manifest_data,
};
use crate::incremental_salt::{incremental_salt_with_flamework, output_manifest_path};
use crate::incremental_signature::{narrow_selection_by_declaration_signature, recovered_input_paths};
use crate::macrodecl::source_uses_macros;
use crate::outputwriter::{OutputWriter, is_local};
use crate::package_layout::prepare_package_layout;
use crate::parallelwrite::{Job, parallelize, write_workers};
use crate::project::{
	ProjectOptions, create_path_translator, get_root_dirs, lock, new_project_context_with_import_context,
	new_project_program_with_options, project_is_package, project_source_files, resolve_include_path,
	resolve_rojo_config_path, run_work_group,
};
use crate::rotortypes::{ROTOR_TYPES_FILE_NAME, write_rotor_types};
use crate::sidecar::project_uses_transformer_plugins;
use crate::source_trace_sourcemap::{rewrite_source_map_paths, rewrite_source_map_with_trace};
use crate::timings::{Stage, start_stage};

/// `BuildResult`: what a build wrote and owns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildResult {
	/// The compiled Luau, by project-relative output path.
	pub outputs: BTreeMap<String, Vec<u8>>,
	/// Every artifact the project owns: outputs, declarations, source maps and copies.
	pub owned_outputs: Vec<String>,
	/// The compiled outputs and declarations written this pass.
	pub emitted_files: Vec<String>,
	/// The output directory.
	pub output_dir: String,
	/// Whether any source references `$env`.
	pub uses_env_macro: bool,
	/// Whether any source references `$nameof`, `$keys` or `$file`.
	pub uses_macros: bool,
	/// Whether `rotor.d.ts` was written this pass.
	pub wrote_rotor_types: bool,
	/// The structured diagnostics; set even when the build fails on them.
	pub diagnostics: Vec<DiagnosticInfo>,
}

/// A failed build: the partial result Go returns beside its diagnostics and error.
#[derive(Debug)]
pub struct BuildFailure {
	/// The result Go returns with the error (its diagnostics), if any.
	pub result: Option<Box<BuildResult>>,
	/// The failure's diagnostics and error.
	pub failure: Failure,
}

impl From<Failure> for BuildFailure {
	fn from(failure: Failure) -> Self {
		Self { result: None, failure }
	}
}

impl From<Error> for BuildFailure {
	fn from(error: Error) -> Self {
		Self {
			result: None,
			failure: Failure::bare(error),
		}
	}
}

impl std::fmt::Display for BuildFailure {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.failure.fmt(formatter)
	}
}

impl std::error::Error for BuildFailure {}

fn io_message(error: &std::io::Error) -> Error {
	Error::message(gostr::io_error_text(error))
}

/// `BuildProjectWithOptions`: the output pipeline of `rotor build`.
///
/// Must run on a large stack ([`rotor_tsrs::with_large_stack`]).
///
/// # Errors
///
/// A [`BuildFailure`] carrying the diagnostics and the error that stopped the build.
pub fn build_project_with_options(project_dir: &str, opts: &ProjectOptions) -> Result<BuildResult, BuildFailure> {
	let result = build_project(project_dir, opts.clone());
	if let Some(timings) = &opts.timings {
		timings.finish();
	}
	result
}

/// `buildProject`: frees the build's programs when it ends, as Go drops them.
#[expect(unsafe_code, reason = "frees the build's programs once nothing borrows them")]
fn build_project(project_dir: &str, opts: ProjectOptions) -> Result<BuildResult, BuildFailure> {
	// SAFETY: declared first, so it drops last; nothing this build derives from its programs
	// leaves the build. The result, the failure and the deferred persistence own their data
	// (the manifest is encoded before it is queued), and every thread that checks or emits is
	// scoped to the build.
	let _programs = unsafe { rotor_tsrs::project::ProgramScope::enter() };
	build_project_in_scope(project_dir, opts)
}

/// [`build_project`] inside its program scope.
#[expect(clippy::too_many_lines, reason = "one function in Go")]
fn build_project_in_scope(project_dir: &str, mut opts: ProjectOptions) -> Result<BuildResult, BuildFailure> {
	let mut writer = OutputWriter::new();
	if let Some(timings) = &opts.timings {
		timings.set_effective_write_workers(write_workers());
		let config_path = if opts.ts_config_path.is_empty() {
			gopath::join(&[project_dir, "tsconfig.json"])
		} else {
			opts.ts_config_path.clone()
		};
		timings.default_config_path(&config_path);
	}
	let timings = opts.timings.clone();
	let timings = timings.as_deref();

	let stop_initial_program = start_stage(timings, Stage::InitialProgram);
	let program_result = new_project_program_with_options(project_dir, &opts.ts_config_path, &opts);
	stop_initial_program.stop();
	let (dir, mut program) = program_result?;
	// Loaded before the native Flamework project opens, which reads the same Rojo configuration
	// through it, and before the salt, which fingerprints the resolver. Declaration-only builds
	// never resolve Rojo paths.
	let rojo_cache: Option<Arc<RojoResolverCache>> = if opts.emit_declaration_only {
		None
	} else {
		load_rojo_caches_pre_build(&dir, &opts).map(Arc::new)
	};
	opts.rojo_cache.clone_from(&rojo_cache);
	let native_pipeline = prepare_flamework_pipeline(&dir, program, &opts)?;
	let flamework_inputs = flamework_incremental_inputs(native_pipeline.as_ref())
		.map_err(|error| Error::message(format!("compile: collect native Flamework incremental inputs: {error}")))?;
	let path_translator = create_path_translator(program, !opts.lua_extension);
	let source_files = project_source_files(program);
	if let Some(timings) = timings {
		timings.set_source_counts(source_files.len(), source_files.len());
	}

	let stop_manifest = start_stage(timings, Stage::IncrementalManifest);
	let mut manifest_path = path_translator.build_info_output_path.clone();
	if manifest_path.is_empty() {
		manifest_path = output_manifest_path(&gopath::from_slash(&dir), &program.options().config_file_path);
	}
	let previous_manifest = match read_incremental_manifest(&manifest_path, Some(program)) {
		Ok(previous) => previous,
		Err(error) => {
			stop_manifest.stop();
			return Err(error.into());
		}
	};
	if !opts.emit_declaration_only {
		match prepare_package_layout(&dir, &program.options(), &opts) {
			Ok(layout) => opts.package_layout = layout.map(Arc::new),
			Err(error) => {
				stop_manifest.stop();
				return Err(error.into());
			}
		}
	}
	let salt = match incremental_salt_with_flamework(&dir, program, &opts, &manifest_path, flamework_inputs.as_ref()) {
		Ok(salt) => salt,
		Err(error) => {
			stop_manifest.stop();
			return Err(error.into());
		}
	};
	let incremental = program.options().is_incremental() && !path_translator.build_info_output_path.is_empty();
	let mut current_manifest = if incremental {
		let mut current = build_incremental_manifest(program, &source_files, &salt, previous_manifest.as_ref());
		if let Some(previous) = &previous_manifest
			&& previous.salt == current.salt
		{
			current
				.outputs
				.extend(previous.outputs.iter().map(|(path, hash)| (path.clone(), hash.clone())));
		}
		current
	} else {
		IncrementalManifest::empty(salt)
	};
	let previous_outputs: BTreeMap<String, String> = match &previous_manifest {
		Some(previous) if previous.salt == current_manifest.salt => previous.outputs.clone(),
		_ => BTreeMap::new(),
	};
	if let Err(error) = writer.use_hashes(
		&gopath::from_slash(&dir),
		&previous_outputs,
		std::mem::take(&mut current_manifest.outputs),
	) {
		stop_manifest.stop();
		return Err(error.into());
	}
	stop_manifest.stop();
	// rotor-rs: validation lstat'd every previous output a moment ago; Go reads their
	// directories again.
	let previous_presence = writer.validated_presence_index();

	if opts.emit_declaration_only {
		return build_declarations_only(
			&dir,
			program,
			&opts,
			&writer,
			previous_manifest.as_ref(),
			current_manifest,
			&manifest_path,
		);
	}

	let stop_selection = start_stage(timings, Stage::IncrementalSelection);
	let source_output_paths: Vec<String> = source_files
		.iter()
		.map(|file| output_path_relative_to_dir(&dir, &path_translator.get_output_path(file.file_name())))
		.collect();
	if let Err(error) = reject_duplicate_output_paths(&source_output_paths) {
		stop_selection.stop();
		return Err(error.into());
	}
	let mut selected_files = source_files.clone();
	// The declaration text the selection pass already emitted, so the build does not emit it
	// twice.
	let mut signature_declarations: Option<Vec<DeclarationEmitFile>> = None;
	if incremental {
		current_manifest.outputs = writer.current_outputs();
		selected_files = select_incremental_source_files(&source_files, &current_manifest, previous_manifest.as_ref());
		let recovered = recovered_input_paths(&dir, &path_translator, &previous_outputs, &previous_presence);
		match narrow_selection_by_declaration_signature(
			&dir,
			program,
			&source_files,
			&recovered,
			&current_manifest,
			previous_manifest.as_ref(),
			&previous_outputs,
			timings,
		) {
			Some((narrowed, declarations)) => {
				selected_files = narrowed;
				signature_declarations = Some(declarations);
			}
			None if !previous_outputs.is_empty() => {
				let mut selected_paths: HashSet<String> = selected_files
					.iter()
					.map(|file| normalize_source_file_path(file.file_name()))
					.collect();
				selected_paths.extend(recovered);
				selected_files = source_files
					.iter()
					.copied()
					.filter(|file| selected_paths.contains(&normalize_source_file_path(file.file_name())))
					.collect();
			}
			None => {}
		}
	}
	stop_selection.stop();
	if !selected_files.is_empty()
		&& let Some(on_compile_start) = &opts.on_compile_start
	{
		on_compile_start();
	}

	let root_dirs = get_root_dirs(program)?;
	let copy_files_gate = load_copy_files_gate_pre_build(&CopyFilesGateInputs {
		root_dirs: root_dirs.clone(),
		out_dir: path_translator.out_dir.clone(),
		declaration: program.options().declaration.is_true(),
		path_translator: &path_translator,
		snapshot: copy_files_changed_snapshot(program, &selected_files),
	});
	let stop_cleanup = start_stage(timings, Stage::Cleanup);
	if !copy_files_gate.skip_cleanup {
		cleanup_outputs(&path_translator, program.options().source_map.is_true());
	}
	stop_cleanup.stop();

	let stop_include_copy = start_stage(timings, Stage::IncludeCopy);
	let include_outputs = maybe_copy_include(&dir, &opts);
	stop_include_copy.stop();
	let include_outputs = include_outputs?;
	// rotor-rs: the Rojo cache trusts its entries for the build (`for_one_build`); Go checks each
	// one when it loads it, after cleanup and the include copy changed the tree.
	if let Some(cache) = &opts.rojo_cache {
		cache.revalidate();
	}

	// Copying the non-compiled files is disk work no compile stage reads, so it runs beside them;
	// every return waits for it.
	std::thread::scope(|scope| {
		let copy = scope.spawn(|| {
			let stop = start_stage(timings, Stage::NonCompiledCopy);
			let result = if copy_files_gate.skip_copy_files {
				Ok(())
			} else {
				copy_non_compiled_files(&path_translator, &root_dirs)
			};
			stop.stop();
			result
		});
		let mut join_copy = Some(copy);
		let mut join = move || -> Result<(), Error> {
			join_copy.take().map_or(Ok(()), |handle| {
				handle
					.join()
					.unwrap_or_else(|_| Err(Error::message("compile: copying non-compiled files panicked")))
			})
		};
		let result = build_after_copy(
			&dir,
			native_pipeline.as_ref(),
			&mut program,
			&mut opts,
			&writer,
			&path_translator,
			&source_files,
			&selected_files,
			signature_declarations,
			&mut current_manifest,
			previous_manifest.as_ref(),
			&previous_presence,
			&manifest_path,
			rojo_cache.clone(),
			&copy_files_gate,
			&include_outputs,
			&mut join,
		);
		let joined = join();
		match (result, joined) {
			(Err(error), _) => Err(error),
			(Ok(_), Err(error)) => Err(error.into()),
			(Ok(result), Ok(())) => Ok(result),
		}
	})
}

#[expect(
	clippy::too_many_arguments,
	clippy::too_many_lines,
	reason = "the rest of Go's BuildProjectWithOptions"
)]
fn build_after_copy(
	dir: &str,
	native_pipeline: Option<&FlameworkPipeline>,
	program: &mut &'static Program,
	opts: &mut ProjectOptions,
	writer: &OutputWriter,
	path_translator: &PathTranslator,
	source_files: &[P<SourceFile>],
	selected_files: &[P<SourceFile>],
	signature_declarations: Option<Vec<DeclarationEmitFile>>,
	current_manifest: &mut IncrementalManifest,
	previous_manifest: Option<&IncrementalManifest>,
	previous_presence: &crate::outputwriter_manifest::OutputPresenceIndex,
	manifest_path: &str,
	rojo_cache: Option<Arc<RojoResolverCache>>,
	copy_files_gate: &crate::copyfilesgate::CopyFilesGateOutputs,
	include_outputs: &[String],
	join_copy: &mut dyn FnMut() -> Result<(), Error>,
) -> Result<BuildResult, BuildFailure> {
	let timings_arc = opts.timings.clone();
	let timings = timings_arc.as_deref();
	if let Some(timings) = timings {
		timings.set_source_counts(source_files.len(), selected_files.len());
	}

	// rotor extension: $env and macro usage, by a substring scan of the loaded sources. Files that
	// import rbxts-transform-env use that plugin's `$env` export instead.
	let mut uses_env_macro = false;
	let mut uses_macros = false;
	for file in source_files {
		let text = file.text();
		if text.contains("$env") && !text.contains("rbxts-transform-env") {
			uses_env_macro = true;
		}
		if source_uses_macros(text) {
			uses_macros = true;
		}
	}

	// Declarations describe the source the user wrote, not the source transformers produced.
	let original_program = *program;
	let selected_paths: HashSet<String> = selected_files
		.iter()
		.map(|file| normalize_source_file_path(file.file_name()))
		.collect();
	let mut pipeline_opts = opts.clone();
	if signature_declarations.is_none() {
		pipeline_opts.declaration_prefetch =
			DeclarationPrefetch::new(original_program, Some(&selected_paths), opts.timings.clone());
	}
	let pipeline = run_compile_pipeline_with_options(
		dir,
		program,
		selected_files,
		&opts.overlays,
		native_pipeline,
		&pipeline_opts,
	)?;
	let flamework_project = pipeline.flamework_project;
	let prepared = pipeline.prepared;
	if let Some(timings) = timings {
		timings.record_prepared_transformer_program(&prepared);
	}
	*program = prepared.program;
	let selected_files = prepared.source_files.clone();
	if prepared
		.flamework
		.as_ref()
		.is_some_and(|flamework| flamework.no_semantic_diagnostics)
	{
		opts.skip_semantic_diagnostics = true;
	}
	if let Some(native_pipeline) = native_pipeline {
		if native_pipeline.config.no_semantic_diagnostics {
			opts.skip_semantic_diagnostics = true;
		}
		// rotor-rs: with nothing selected the pipeline transformed nothing, so the inputs and the
		// salt are the ones the build started with; Go collects them again.
		if !selected_files.is_empty() {
			let final_inputs = flamework_incremental_inputs(Some(native_pipeline)).map_err(|error| {
				Error::message(format!("compile: refresh native Flamework incremental inputs: {error}"))
			})?;
			current_manifest.salt =
				incremental_salt_with_flamework(dir, program, opts, manifest_path, final_inputs.as_ref())?;
		}
	}
	if let Some(timings) = timings {
		timings.set_source_counts(source_files.len(), selected_files.len());
	}
	// rotor-rs: the original program is past its last check once a transform replaced it and its
	// declarations are emitted, so its checkers go before the replacing program's fill up.
	let declarations_ready = !original_program.options().get_emit_declarations()
		|| signature_declarations.is_some()
		|| pipeline_opts
			.declaration_prefetch
			.as_ref()
			.is_some_and(|prefetch| prefetch.has_result());
	if declarations_ready && !std::ptr::eq(*program, original_program) {
		crate::flamework_pipeline::release_transformed_checkers(original_program);
	}

	let stop_project_context = start_stage(timings, Stage::ProjectContext);
	let pctx = new_project_context_with_import_context(dir, program, opts, !selected_files.is_empty());
	stop_project_context.stop();
	let mut pctx = pctx?;
	pctx.source_traces = prepared.source_traces.clone();
	let (outputs, source_maps) =
		match crate::project::compile_project_source_files(dir, program, &pctx, &selected_files, opts) {
			Ok(compiled) => compiled,
			Err(failure) => {
				return Err(BuildFailure {
					result: Some(Box::new(BuildResult {
						diagnostics: failure.diagnostics.clone(),
						..BuildResult::default()
					})),
					failure,
				});
			}
		};

	join_copy()?;
	let prefetched = pipeline_opts
		.declaration_prefetch
		.as_ref()
		.and_then(|prefetch| prefetch.result());
	let declarations = match prefetched {
		Some(prefetched) => Some(prefetched.map_err(declaration_build_failure)?),
		None => signature_declarations,
	};

	// Declarations and compiled outputs are disjoint files, so they write together; a compiled
	// output failure still reports first.
	let (write_result, declaration_result) = std::thread::scope(|scope| {
		let declarations_written = scope.spawn(|| {
			let stop = start_stage(timings, Stage::DeclarationEmitWrites);
			let files = emit_declarations(
				original_program,
				Some(&selected_paths),
				declarations,
				opts.write_only_changed,
				writer,
				timings,
			);
			stop.stop();
			files
		});
		let write_result = write_compiled_outputs(
			dir,
			path_translator,
			&selected_files,
			&outputs,
			&source_maps,
			&prepared.source_traces,
			writer,
			timings,
		);
		let declaration_result = declarations_written
			.join()
			.unwrap_or_else(|_| Err(Error::message("compile: declaration writes panicked")));
		(write_result, declaration_result)
	});
	let mut emitted_files = write_result?;
	if let Some(timings) = timings {
		timings.set_emitted_entries(emitted_files.len());
	}
	emitted_files.extend(declaration_result.map_err(declaration_build_failure)?);
	if let Some(timings) = timings {
		timings.set_hash_skips(writer.hash_skip_count());
	}

	let stop_persistence = start_stage(timings, Stage::Persistence);
	if let Some(project) = flamework_project {
		if let Some(pending) = &opts.pending_solution_dependency_persists {
			lock(pending).push(Box::new(move || {
				lock(&project)
					.persist()
					.map_err(|error| Error::message(error.to_string()))
			}));
		} else {
			let persisted = lock(&project).persist();
			if let Err(error) = persisted {
				stop_persistence.stop();
				return Err(Error::message(format!("compile: persist native Flamework artifacts: {error}")).into());
			}
		}
	}
	let mut current_outputs = writer.current_outputs();
	if copy_files_gate.skip_cleanup {
		// No cleanup ran and nothing was selected: the pre-build index is still valid.
		prune_missing_outputs(previous_presence, &mut current_outputs);
	} else {
		let current_presence = writer.new_output_presence_index(current_outputs.keys());
		prune_missing_outputs(&current_presence, &mut current_outputs);
	}
	writer.set_current_outputs(current_outputs.clone());
	writer.persist_stamps();
	// rotor-rs: a build that wrote outputs refreshes, when it persists (after every project of a
	// solution wrote), the Rojo cache entries those writes invalidated, so the next build loads
	// them from disk (`RojoResolverCache::refresh`).
	let refresh_rojo_cache =
		!(selected_files.is_empty() && copy_files_gate.skip_cleanup && copy_files_gate.skip_copy_files);
	current_manifest.outputs = current_outputs;
	// rotor extension: keep the rotor.d.ts editor companion fresh for macro users.
	let mut wrote_rotor_types = false;
	if uses_env_macro || uses_macros {
		match write_rotor_types(&gopath::from_slash(dir)) {
			Ok(wrote) => wrote_rotor_types = wrote,
			Err(error) => {
				stop_persistence.stop();
				return Err(Error::message(format!(
					"compile: writing {ROTOR_TYPES_FILE_NAME}: {}",
					gostr::io_error_text(&error)
				))
				.into());
			}
		}
	}

	let manifest = current_manifest.clone();
	let manifest_changed = !same_incremental_manifest(previous_manifest, Some(&manifest));
	let gate = crate::copyfilesgate::CopyFilesGateOutputs {
		persist: copy_files_gate.persist.clone(),
		..crate::copyfilesgate::CopyFilesGateOutputs::default()
	};
	let manifest_path_owned = manifest_path.to_owned();
	// Encoded now: a deferred write must not keep the program, which the build frees.
	// The graph describes original inputs: a transformer can replace their imports.
	let manifest_data = manifest_changed.then(|| encode_incremental_manifest(&manifest, original_program));
	let mut persist: crate::project::PendingPersist = Box::new(move || {
		if let Some(cache) = &rojo_cache {
			if refresh_rojo_cache {
				cache.refresh();
			}
			cache.persist();
		}
		gate.persist()?;
		if let Some(data) = &manifest_data {
			write_incremental_manifest_data(&manifest_path_owned, data)?;
		}
		Ok(())
	});
	match &opts.pending_solution_persists {
		Some(pending) => lock(pending).push(persist),
		None => {
			if let Err(error) = persist() {
				stop_persistence.stop();
				return Err(error.into());
			}
		}
	}
	stop_persistence.stop();
	if let Some(timings) = timings {
		timings.set_emitted_entries(emitted_files.len());
	}

	Ok(BuildResult {
		owned_outputs: collect_owned_output_paths(
			dir,
			&current_manifest.outputs,
			&[&copy_files_gate.owned_outputs, include_outputs],
		),
		outputs,
		emitted_files,
		output_dir: path_translator.out_dir.clone(),
		uses_env_macro,
		uses_macros,
		wrote_rotor_types,
		diagnostics: Vec::new(),
	})
}

/// The compiled output and source map writes, fanned out across the write workers.
#[expect(
	clippy::too_many_arguments,
	reason = "the write half of Go's BuildProjectWithOptions"
)]
fn write_compiled_outputs(
	dir: &str,
	path_translator: &PathTranslator,
	selected_files: &[P<SourceFile>],
	outputs: &BTreeMap<String, Vec<u8>>,
	source_maps: &BTreeMap<String, String>,
	source_traces: &crate::source_trace::DiagnosticTraces,
	writer: &OutputWriter,
	timings: Option<&crate::timings::BuildTimings>,
) -> Result<Vec<String>, Error> {
	let stop = start_stage(timings, Stage::CompiledOutputWrites);
	let result = (|| {
		let mut rel_outs: Vec<(String, P<SourceFile>)> = Vec::with_capacity(outputs.len());
		for &file in selected_files {
			let rel_out = output_path_relative_to_dir(dir, &path_translator.get_output_path(file.file_name()));
			if outputs.contains_key(&rel_out) {
				rel_outs.push((rel_out, file));
			}
		}
		let mut write_paths = Vec::with_capacity(rel_outs.len() * 2);
		let mut planned: Vec<(String, String, Option<String>)> = Vec::with_capacity(rel_outs.len());
		for (rel_out, file) in &rel_outs {
			let abs_out = gopath::join(&[&gopath::from_slash(dir), &gopath::from_slash(rel_out)]);
			let mut source_map = source_maps.get(&format!("{rel_out}.map")).cloned();
			if let Some(map) = &mut source_map {
				if let Some(timings) = timings {
					timings.add_scheduled_source_map_write();
				}
				if let Some(trace) = source_traces.get(file.file_name()) {
					*map = rewrite_source_map_with_trace(map, trace).map_err(Error::Message)?;
				}
				*map = rewrite_source_map_paths(map, &abs_out).map_err(Error::Message)?;
			}
			write_paths.push(abs_out.clone());
			if source_map.is_some() {
				write_paths.push(format!("{abs_out}.map"));
			}
			planned.push((rel_out.clone(), abs_out, source_map));
		}
		writer.prepare(&write_paths)?;
		let wrote: Vec<AtomicBool> = planned.iter().map(|_| AtomicBool::new(false)).collect();
		let jobs: Vec<Job<'_>> = planned
			.iter()
			.zip(&wrote)
			.map(|((rel_out, abs_out, source_map), wrote)| -> Job<'_> {
				Box::new(move || {
					// Defense in depth: refuse any output path escaping the project directory.
					assert_local_output_path(rel_out)?;
					let text = outputs.get(rel_out).map_or(&[][..], Vec::as_slice);
					let written = writer.write(abs_out, text)?;
					wrote.store(written, Ordering::Relaxed);
					if let Some(timings) = timings {
						timings.record_output_write(abs_out, written);
					}
					let Some(source_map) = source_map else {
						return Ok(());
					};
					let map_path = format!("{abs_out}.map");
					let map_written = writer.write(&map_path, source_map.as_bytes())?;
					if let Some(timings) = timings {
						timings.record_output_write(&map_path, map_written);
					}
					Ok(())
				})
			})
			.collect();
		parallelize(write_workers(), jobs)?;
		Ok(planned
			.iter()
			.zip(&wrote)
			.filter(|(_, wrote)| wrote.load(Ordering::Relaxed))
			.map(|((rel_out, _, _), _)| gopath::join(&[&gopath::from_slash(dir), &gopath::from_slash(rel_out)]))
			.collect())
	})();
	stop.stop();
	result
}

/// The `emitDeclarationOnly` route of `BuildProjectWithOptions`.
fn build_declarations_only(
	dir: &str,
	program: &'static Program,
	opts: &ProjectOptions,
	writer: &OutputWriter,
	previous_manifest: Option<&IncrementalManifest>,
	mut current_manifest: IncrementalManifest,
	manifest_path: &str,
) -> Result<BuildResult, BuildFailure> {
	let timings = opts.timings();
	if !program.options().get_emit_declarations() {
		let message = "Option 'emitDeclarationOnly' cannot be specified without specifying option 'declaration' or option 'composite'.";
		return Err(Failure::strings(
			vec![message.to_owned()],
			Error::message("compile: TypeScript diagnostics"),
		)
		.into());
	}
	// Declarations come from the original program. Configured transformer modules are validated
	// without running their factories: transformed text cannot affect declarations.
	if project_uses_transformer_plugins(Some(&program.command_line())) {
		crate::sidecar::validate_transformer_host(
			opts.context.as_ref(),
			crate::sidecar::transformer_host_for(opts),
			dir,
			program,
		)?;
	}
	let stop = start_stage(timings, Stage::DeclarationEmitWrites);
	let emitted = emit_declarations(program, None, None, opts.write_only_changed, writer, timings);
	stop.stop();
	let emitted = emitted.map_err(declaration_build_failure)?;
	if let Some(timings) = timings {
		timings.set_hash_skips(writer.hash_skip_count());
	}
	let stop_persistence = start_stage(timings, Stage::Persistence);
	let mut outputs = writer.current_outputs();
	let presence = writer.new_output_presence_index(outputs.keys());
	prune_missing_outputs(&presence, &mut outputs);
	writer.set_current_outputs(outputs.clone());
	writer.persist_stamps();
	current_manifest.outputs = outputs;
	if !same_incremental_manifest(previous_manifest, Some(&current_manifest))
		&& let Err(error) = write_incremental_manifest(manifest_path, &current_manifest, program)
	{
		stop_persistence.stop();
		return Err(error.into());
	}
	stop_persistence.stop();
	if let Some(timings) = timings {
		timings.set_emitted_entries(emitted.len());
	}
	Ok(BuildResult {
		owned_outputs: collect_owned_output_paths(dir, &current_manifest.outputs, &[]),
		emitted_files: emitted,
		..BuildResult::default()
	})
}

/// `loadRojoCachesPreBuild`
fn load_rojo_caches_pre_build(dir: &str, opts: &ProjectOptions) -> Option<RojoResolverCache> {
	let (Some(rojo_config_path), _) = resolve_rojo_config_path(dir, &opts.rojo_config_path).ok()? else {
		return None;
	};
	let cache = RojoResolverCache::with_deferred_persist(
		&gopath::join(&[&gopath::from_slash(dir), ".rotor", "cache", "rojo"]),
		// Go keys this cache by tsgo's `core.Version()`; rotor-rs keys it by its own compiler
		// version, so Go-written and Rust-written caches never pass for each other.
		&crate::copyfilesgate::compiler_version(),
		opts.defer_rojo_cache_persist,
	)
	.for_one_build();
	let _ = cache.load(&rojo_config_path);
	Some(cache)
}

/// `declarationEmitFile`: one file the declaration emit produced (`.d.ts` or `.d.ts.map`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarationEmitFile {
	/// The output path.
	pub file_name: String,
	/// The text, with rotor's rewrites applied.
	pub text: String,
}

/// `declarationBuildFailure`: a declaration diagnostics error keeps its diagnostics on the result.
fn declaration_build_failure(error: Error) -> BuildFailure {
	if let Error::DeclarationEmitDiagnostics(diagnostics) = &error {
		let diagnostics = diagnostics.clone();
		return BuildFailure {
			result: Some(Box::new(BuildResult {
				diagnostics: diagnostics.clone(),
				..BuildResult::default()
			})),
			failure: Failure { diagnostics, error },
		};
	}
	error.into()
}

/// `emitDeclarationTexts`: the declaration seam. One entry per emitted artifact with rotor's
/// rewrites applied, files in the order given. `program` must be the original program.
pub(crate) fn emit_declaration_texts(
	program: &'static Program,
	files: &[P<SourceFile>],
) -> Result<Vec<DeclarationEmitFile>, Error> {
	emit_declaration_texts_until(program, files, None)
}

/// `errDeclarationEmitAbandoned`
pub(crate) const DECLARATION_EMIT_ABANDONED: &str = "compile: declaration emit abandoned";

fn abandoned(abandon: Option<&AtomicBool>) -> bool {
	abandon.is_some_and(|abandon| abandon.load(Ordering::Relaxed))
}

/// `emitDeclarationTextsUntil`: setting `abandon` stops it between files.
///
/// Checker state one file's emit leaves shapes the next file's text, so each checker emits its
/// files serially in program order; checkers share no state, so the groups run concurrently.
pub(crate) fn emit_declaration_texts_until(
	program: &'static Program,
	files: &[P<SourceFile>],
	abandon: Option<&AtomicBool>,
) -> Result<Vec<DeclarationEmitFile>, Error> {
	if !program.options().get_emit_declarations() || files.is_empty() {
		return Ok(Vec::new());
	}
	let rewriter = DeclarationPathRewriter::new(program);
	let order = declaration_emit_order(program, files);
	let ordered: Vec<P<SourceFile>> = order.iter().map(|&index| files[index]).collect();
	let context = Context::default();
	let groups = group_by_checker(&context, program, &ordered);
	let single_threaded = program.single_threaded() || groups.len() <= 1;
	let per_group = run_work_group(single_threaded, groups.len(), |group_index| {
		let mut emitted: Vec<(usize, FileEmit)> = Vec::new();
		for &ordered_index in &groups[group_index] {
			if abandoned(abandon) {
				break;
			}
			let index = order[ordered_index];
			emitted.push((index, emit_file(program, rewriter.as_ref(), files[index])));
		}
		emitted
	});
	if abandoned(abandon) {
		return Err(Error::message(DECLARATION_EMIT_ABANDONED));
	}
	let mut per_file: Vec<Option<FileEmit>> = vec![None; files.len()];
	for (index, result) in per_group.into_iter().flatten() {
		per_file[index] = Some(result);
	}
	let diagnostics: Vec<DiagnosticInfo> = per_file
		.iter()
		.flatten()
		.filter_map(|result| result.as_ref().err())
		.flatten()
		.cloned()
		.collect();
	if !diagnostics.is_empty() {
		return Err(Error::DeclarationEmitDiagnostics(diagnostics));
	}
	Ok(per_file.into_iter().flatten().flatten().flatten().collect())
}

/// One file's declaration emit with the rewrites applied, or its emit diagnostics.
type FileEmit = Result<Vec<DeclarationEmitFile>, Vec<DiagnosticInfo>>;

/// One file's [`FileEmit`].
fn emit_file(
	program: &'static Program,
	rewriter: Option<&DeclarationPathRewriter>,
	source_file: P<SourceFile>,
) -> FileEmit {
	let pending: Mutex<Vec<DeclarationEmitFile>> = Mutex::new(Vec::new());
	let write_file = |file_name: &str, text: &str, _data: &mut rotor_tsrs::compiler::WriteFileData| {
		lock(&pending).push(DeclarationEmitFile {
			file_name: file_name.to_owned(),
			text: text.to_owned(),
		});
		Ok(())
	};
	let result = program.emit(
		&Context::default(),
		&rotor_tsrs::compiler::EmitOptions {
			target_source_files: Some(vec![source_file]),
			emit_only: EmitOnly::EmitOnlyDts,
			force_emit: false,
			write_file: Some(&write_file),
		},
	);
	if !result.diagnostics.is_empty() {
		return Err(ts_diagnostic_infos(
			&result.diagnostics,
			&crate::source_trace::DiagnosticTraces::default(),
		));
	}
	let mut pending = pending.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
	rewrite_declaration_emit(rewriter, source_file, &mut pending);
	Ok(pending)
}

/// The groups of indices into `files` that one checker checks, groups in order of their first
/// file (`groupSourceFilesByChecker`).
fn group_by_checker(context: &Context, program: &'static Program, files: &[P<SourceFile>]) -> Vec<Vec<usize>> {
	let mut groups_by_checker: HashMap<u32, usize> = HashMap::new();
	let mut groups: Vec<Vec<usize>> = Vec::new();
	for (index, &file) in files.iter().enumerate() {
		let key = program.get_type_checker_for_file_exclusive(context, file).id;
		let group = *groups_by_checker.entry(key).or_insert_with(|| {
			groups.push(Vec::new());
			groups.len() - 1
		});
		groups[group].push(index);
	}
	groups
}

/// `declarationEmitOrder`: indices of `files` sorted stably by their position in the program.
fn declaration_emit_order(program: &Program, files: &[P<SourceFile>]) -> Vec<usize> {
	let position: HashMap<P<SourceFile>, usize> = program
		.source_files()
		.iter()
		.enumerate()
		.map(|(index, &file)| (file, index))
		.collect();
	let mut order: Vec<usize> = (0..files.len()).collect();
	order.sort_by_key(|&index| position.get(&files[index]).copied().unwrap_or(0));
	order
}

/// `rewriteDeclarationEmit`: the `@rbxts/types` reference rescope and the `paths` specifier
/// rewrite, with the matching `.d.ts.map` column fixup.
fn rewrite_declaration_emit(
	rewriter: Option<&DeclarationPathRewriter>,
	source_file: P<SourceFile>,
	emitted: &mut [DeclarationEmitFile],
) {
	for entry in emitted.iter_mut() {
		if entry.file_name.ends_with(".d.ts.map") {
			entry.text = crate::declmap::restore_empty_source_root(&entry.text);
		}
	}
	for index in 0..emitted.len() {
		if !emitted[index].file_name.ends_with(tspath::EXTENSION_DTS) {
			continue;
		}
		let original = emitted[index].text.clone();
		let mut edits = type_reference_edits(&original);
		if let Some(rewriter) = rewriter {
			edits.extend(rewriter.specifier_edits(source_file, &emitted[index].file_name, &original));
		}
		let edits = non_overlapping_text_edits(edits);
		if edits.is_empty() {
			continue;
		}
		emitted[index].text = apply_text_edits(&original, &edits);
		let map_name = format!("{}.map", emitted[index].file_name);
		for entry in emitted.iter_mut() {
			if entry.file_name == map_name {
				entry.text = shift_declaration_map_columns(&entry.text, &original, &edits);
			}
		}
	}
}

/// `emitsDeclarationOutput`
pub(crate) fn emits_declaration_output(source_file: P<SourceFile>) -> bool {
	!source_file.is_declaration_file() && is_compilable_file(source_file.file_name())
}

/// `declarationEmitSourceFiles`: the compilable sources, narrowed to `selected_paths`.
pub(crate) fn declaration_emit_source_files(
	program: &Program,
	selected_paths: Option<&HashSet<String>>,
) -> Vec<P<SourceFile>> {
	program
		.source_files()
		.iter()
		.copied()
		.filter(|&file| emits_declaration_output(file))
		.filter(|file| selected_paths.is_none_or(|paths| paths.contains(&normalize_source_file_path(file.file_name()))))
		.collect()
}

/// `emitDeclarations`: produces and writes the build's declarations. `pre_emitted` is text the
/// signature selection or a prefetch already produced for exactly `selected_paths`.
pub(crate) fn emit_declarations(
	program: &'static Program,
	selected_paths: Option<&HashSet<String>>,
	pre_emitted: Option<Vec<DeclarationEmitFile>>,
	write_only_changed: bool,
	writer: &OutputWriter,
	timings: Option<&crate::timings::BuildTimings>,
) -> Result<Vec<String>, Error> {
	// Checked before the stage timer so a project without declarations reports no stage.
	if !program.options().get_emit_declarations() {
		return Ok(Vec::new());
	}
	let emitted = if let Some(emitted) = pre_emitted {
		emitted
	} else {
		let files = declaration_emit_source_files(program, selected_paths);
		if files.is_empty() {
			return Ok(Vec::new());
		}
		let stop = start_stage(timings, Stage::DeclarationEmit);
		let emitted = logservice::benchmark_if_verbose("declaration emit", || emit_declaration_texts(program, &files));
		stop.stop();
		emitted?
	};
	write_declaration_files(&emitted, write_only_changed, writer, timings)
}

/// `writeDeclarationFiles`: the write half of the declaration emit. tsrs's write callback data
/// lives only for the emit, so a skipped write is not flagged on it (Go's `SkippedDtsWrite`,
/// which nothing reads after the emit).
fn write_declaration_files(
	emitted: &[DeclarationEmitFile],
	_write_only_changed: bool,
	writer: &OutputWriter,
	timings: Option<&crate::timings::BuildTimings>,
) -> Result<Vec<String>, Error> {
	if emitted.is_empty() {
		return Ok(Vec::new());
	}
	let paths: Vec<String> = emitted
		.iter()
		.map(|file| gopath::from_slash(&file.file_name).into_owned())
		.collect();
	reject_duplicate_output_paths(&paths)?;
	if let Some(timings) = timings {
		timings.add_scheduled_declaration_writes(emitted.len());
	}
	writer.prepare(&paths)?;
	let wrote: Vec<AtomicBool> = emitted.iter().map(|_| AtomicBool::new(false)).collect();
	let jobs: Vec<Job<'_>> = emitted
		.iter()
		.zip(&paths)
		.zip(&wrote)
		.map(|((file, path), wrote)| -> Job<'_> {
			Box::new(move || {
				let written = writer.write(path, file.text.as_bytes())?;
				wrote.store(written, Ordering::Relaxed);
				if let Some(timings) = timings {
					timings.record_output_write(path, written);
				}
				Ok(())
			})
		})
		.collect();
	parallelize(write_workers(), jobs)?;
	Ok(paths
		.into_iter()
		.zip(&wrote)
		.filter(|(_, wrote)| wrote.load(Ordering::Relaxed))
		.map(|(path, _)| path)
		.collect())
}

/// `typeReferenceEdits`: rescopes the bare `types` triple-slash reference to `@rbxts/types`.
pub(crate) fn type_reference_edits(text: &str) -> Vec<TextEdit> {
	const BARE: &str = "types=\"types\"";
	const SCOPED: &str = "types=\"@rbxts/types\"";
	text.match_indices(BARE)
		.map(|(index, _)| TextEdit {
			start: index,
			end: index + BARE.len(),
			text: SCOPED.to_owned(),
		})
		.collect()
}

/// `assertLocalOutputPath`: rejects absolute or escaping project-relative output paths.
pub(crate) fn assert_local_output_path(rel_out: &str) -> Result<(), Error> {
	if !is_local(&gopath::from_slash(rel_out)) {
		return Err(Error::message(format!(
			"compile: refusing to write output outside the project directory: {}",
			gostr::quote(rel_out)
		)));
	}
	Ok(())
}

/// `outputPathRelativeToDir`
pub(crate) fn output_path_relative_to_dir(dir: &str, path: &str) -> String {
	let native = gopath::from_slash(path);
	let relative = gopath::rel(&gopath::from_slash(dir), &native).unwrap_or_else(|_| native.into_owned());
	gopath::to_slash(&relative).into_owned()
}

/// `rejectDuplicateOutputPaths`: two inputs that would write the same output.
pub(crate) fn reject_duplicate_output_paths(paths: &[String]) -> Result<(), Error> {
	let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	let mut seen: HashMap<String, &String> = HashMap::with_capacity(paths.len());
	for path in paths {
		let absolute = gopath::abs(&gopath::from_slash(path)).map_err(|error| {
			Error::message(format!(
				"compile: resolving output path {}: {error}",
				gostr::quote(path)
			))
		})?;
		let mut key = gopath::clean(&absolute);
		if !case_sensitive {
			key = key.to_lowercase();
		}
		if let Some(previous) = seen.get(&key) {
			return Err(Error::message(format!(
				"compile: duplicate output path detected: {previous} and {path}"
			)));
		}
		seen.insert(key, path);
	}
	Ok(())
}

/// `maybeCopyInclude` (`copyInclude.ts`): writes the runtime library unless disabled or a
/// package.
pub(crate) fn maybe_copy_include(dir: &str, opts: &ProjectOptions) -> Result<Vec<String>, Error> {
	use rotor_transformer::ProjectType;
	if !opts.emit_include_files || opts.project_type == Some(ProjectType::Package) {
		return Ok(Vec::new());
	}
	let (_, is_package) = project_is_package(dir)?;
	if opts.project_type.is_none() && is_package {
		return Ok(Vec::new());
	}
	let include_path = resolve_include_path(dir, &opts.include_path)?;
	logservice::benchmark_if_verbose("copy include files", || rotor_config::includefiles::copy(&include_path))
		.map_err(|error| io_message(&error))?;
	Ok(rotor_config::includefiles::names()
		.into_iter()
		.map(|name| gopath::join(&[&include_path, name]))
		.collect())
}

/// `collectOwnedOutputPaths`: the manifest outputs and the given absolute outputs, project
/// relative, sorted and unique.
pub(crate) fn collect_owned_output_paths(
	project_dir: &str,
	manifest_outputs: &BTreeMap<String, String>,
	absolute_outputs: &[&[String]],
) -> Vec<String> {
	let mut owned: BTreeSet<String> = manifest_outputs
		.keys()
		.map(|path| gopath::to_slash(&gopath::clean(&gopath::from_slash(path))).into_owned())
		.collect();
	for outputs in absolute_outputs {
		for path in *outputs {
			let native = gopath::from_slash(path);
			let relative =
				gopath::rel(&gopath::from_slash(project_dir), &native).unwrap_or_else(|_| native.into_owned());
			owned.insert(gopath::to_slash(&relative).into_owned());
		}
	}
	owned.into_iter().collect()
}

/// `cleanupOutputs`: removes the orphaned outputs under `outDir`.
pub(crate) fn cleanup_outputs(path_translator: &PathTranslator, source_maps_enabled: bool) {
	cleanup_dir_recursively(path_translator, &path_translator.out_dir, source_maps_enabled);
}

/// `cleanupDirRecursively`
fn cleanup_dir_recursively(path_translator: &PathTranslator, dir: &str, source_maps_enabled: bool) {
	let Ok(mut entries) = fs::read_dir(dir).map(|entries| entries.filter_map(Result::ok).collect::<Vec<_>>()) else {
		return;
	};
	entries.sort_by_key(fs::DirEntry::file_name);
	for entry in entries {
		let name = entry.file_name().to_string_lossy().into_owned();
		let item_path = gopath::join(&[dir, &name]);
		if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
			if name == ".git" {
				continue;
			}
			cleanup_dir_recursively(path_translator, &item_path, source_maps_enabled);
		}
		try_remove_output(path_translator, &item_path, source_maps_enabled);
	}
}

/// `tryRemoveOutput`
pub(crate) fn try_remove_output(path_translator: &PathTranslator, out_path: &str, source_maps_enabled: bool) {
	if !is_output_file_orphaned_with_source_maps(path_translator, out_path, source_maps_enabled) {
		return;
	}
	let removed = match fs::symlink_metadata(out_path) {
		Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(out_path),
		Ok(_) => fs::remove_file(out_path),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
		Err(error) => Err(error),
	};
	if removed.is_ok() {
		logservice::write_line_if_verbose(&[&format!("remove {out_path}")]);
	}
}

/// `isOutputFileOrphaned`
#[must_use]
pub fn is_output_file_orphaned(path_translator: &PathTranslator, out_path: &str) -> bool {
	is_output_file_orphaned_with_source_maps(path_translator, out_path, false)
}

fn any_input_exists(path_translator: &PathTranslator, out_path: &str) -> bool {
	path_translator
		.get_input_paths(out_path)
		.iter()
		.any(|input_path| fs::metadata(input_path).is_ok())
}

/// `isOutputFileOrphanedWithSourceMaps`: an output whose inputs are gone.
pub(crate) fn is_output_file_orphaned_with_source_maps(
	path_translator: &PathTranslator,
	out_path: &str,
	source_maps_enabled: bool,
) -> bool {
	if is_protected_output_filename(&gopath::base(out_path)) {
		return false;
	}
	if let Some(output) = out_path.strip_suffix(".luau.map") {
		if !source_maps_enabled {
			return true;
		}
		return !any_input_exists(path_translator, &format!("{output}.luau"));
	}
	if out_path.ends_with(".d.ts") && !path_translator.declaration {
		return true;
	}
	if any_input_exists(path_translator, out_path) {
		return false;
	}
	if let Some(output) = out_path.strip_suffix(".map")
		&& any_input_exists(path_translator, output)
	{
		return false;
	}
	path_translator.build_info_output_path != out_path
}

/// `copyNonCompiledFiles`: copies the root dirs' non-compiled files into `outDir`.
pub(crate) fn copy_non_compiled_files(path_translator: &PathTranslator, root_dirs: &[String]) -> Result<(), Error> {
	for root_dir in root_dirs {
		let root_dir = gopath::from_slash(root_dir);
		if fs::metadata(root_dir.as_ref()).is_err() {
			continue;
		}
		copy_item(path_translator, &root_dir)?;
	}
	Ok(())
}

/// `copyItem`
pub(crate) fn copy_item(path_translator: &PathTranslator, item_path: &str) -> Result<(), Error> {
	let info = fs::metadata(item_path).map_err(|error| Error::message(gostr::path_error("stat", item_path, &error)))?;
	if info.is_dir() {
		let entries = crate::packageexports::read_dir_sorted(item_path)
			.map_err(|error| Error::message(gostr::path_error("open", item_path, &error)))?;
		if path_contains(item_path, &path_translator.out_dir) {
			for name in entries {
				if name == "node_modules" {
					continue;
				}
				let child_path = gopath::join(&[item_path, &name]);
				if gopath::clean(&child_path) == gopath::clean(&path_translator.out_dir) {
					continue;
				}
				copy_item(path_translator, &child_path)?;
			}
			return Ok(());
		}
		let out_dir = path_translator.get_output_path(item_path);
		fsutil::mkdir_all(&out_dir, 0o755).map_err(|error| io_message(&error))?;
		for name in entries {
			copy_item(path_translator, &gopath::join(&[item_path, &name]))?;
		}
		return Ok(());
	}

	if item_path.ends_with(".d.ts") {
		if !path_translator.declaration {
			return Ok(());
		}
	} else if is_compilable_file(item_path) {
		return Ok(());
	}

	let dest = path_translator.get_output_path(item_path);
	let data = fs::read(item_path).map_err(|error| Error::message(gostr::path_error("open", item_path, &error)))?;
	fsutil::mkdir_all(gopath::dir(&dest), 0o755).map_err(|error| io_message(&error))?;
	fsutil::write_file_if_changed(&dest, &data, 0o644).map_err(|error| io_message(&error))?;
	Ok(())
}

/// `pathContains`
fn path_contains(parent: &str, child: &str) -> bool {
	let Ok(rel) = gopath::rel(&gopath::clean(parent), &gopath::clean(child)) else {
		return false;
	};
	rel == "." || (rel != ".." && !rel.starts_with(&format!("..{}", gopath::NATIVE.separator())))
}

/// `isCompilableFile`: an existing `.ts`/`.tsx` file that is not a declaration.
#[expect(
	clippy::case_sensitive_file_extension_comparisons,
	reason = "Go's isCompilableFile matches the extensions case-sensitively"
)]
pub(crate) fn is_compilable_file(path: &str) -> bool {
	if !fs::metadata(path).is_ok_and(|info| !info.is_dir()) {
		return false;
	}
	if path.ends_with(".d.ts") {
		return false;
	}
	path.ends_with(".ts") || path.ends_with(".tsx")
}

/// Go's diagnostics-as-strings view of a failed build.
#[must_use]
pub fn build_failure_messages(failure: &BuildFailure) -> Vec<String> {
	diagnostic_info_messages(&failure.failure.diagnostics)
}
