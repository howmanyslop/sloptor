//! `flamework_pipeline.go`: the transformer stages of one build.
//!
//! Without native Flamework the build runs its transformer plugins, if any, in one sidecar stage.
//! With it, the plugins split around native Flamework at `flamework.after`: a prefix stage, the
//! native transform, and a suffix stage. A stage whose first `before` plugin is a ported jecs
//! build runs jecs natively first (`jecs_native`).

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::Program;

use crate::compile::{Error, Failure};
use crate::flamework_program_update::{
	NativeProgramUpdate, NativeSourceOverlay, print_native_source, update_native_flamework_program,
};
use crate::jecs_native::{apply_native_jecs_transform, native_jecs_stage, uses_subprocess_sidecar};
use crate::project::{Cancellation, ProjectOptions, create_path_translator, lock};
use crate::sidecar::{
	PreparedTransformerProgram, SidecarBuildState, apply_transformer_sidecar_with_plugins_and_host,
	prepare_transformer_program_with_host, remap_program_source_files, transformer_host_for,
};
use crate::sidecar_parallel::{ParallelPrefix, parallel_prefix_context};
use crate::sidecar_shards::warm_transformer_sidecar_shards;
use crate::source_trace::{DiagnosticTraces, compose_source_trace_maps};
use crate::timings::log_stage_named;
use crate::transformer_host::{TRANSFORMER_ORIGINAL_AUTHORED, TransformerHost};
use crate::transformer_order::{TransformerPluginConfig, effective_transformer_plugins, split_transformer_plugins};

/// `flameworkPipeline`: the native Flamework stages of one build.
pub struct FlameworkPipeline {
	/// The active configuration.
	pub config: rotor_config::config::FlameworkConfig,
	/// The project, shared with the persistence the build queues.
	pub(crate) project: Arc<Mutex<rotor_flamework::Project>>,
	/// The program the pipeline was prepared for.
	pub(crate) program: &'static Program,
	/// The effective transformer plugins.
	pub(crate) plugins: Vec<TransformerPluginConfig>,
	/// The plugins before native Flamework.
	pub(crate) prefix: Vec<TransformerPluginConfig>,
	/// The plugins after native Flamework.
	pub(crate) suffix: Vec<TransformerPluginConfig>,
}

/// `compilePipelineResult`
pub(crate) struct CompilePipelineResult {
	pub(crate) prepared: PreparedTransformerProgram,
	/// The native Flamework project whose artifacts the build persists.
	pub(crate) flamework_project: Option<Arc<Mutex<rotor_flamework::Project>>>,
}

/// `dirtyFlameworkIncrementalStateError`
const DIRTY_FLAMEWORK_INCREMENTAL_STATE: &str =
	"Flamework cannot be built in a dirty environment, please delete your tsbuildinfo";

/// `prepareCompilePipeline`
pub(crate) fn prepare_compile_pipeline(
	dir: &str,
	program: &'static Program,
	source_files: &[P<SourceFile>],
	overlays: &BTreeMap<String, String>,
	opts: &ProjectOptions,
) -> Result<CompilePipelineResult, Failure> {
	let pipeline = prepare_flamework_pipeline(dir, program, opts)?;
	run_compile_pipeline_with_options(dir, program, source_files, overlays, pipeline.as_ref(), opts)
}

/// `prepareFlameworkPipeline`: `None` with Flamework off.
pub(crate) fn prepare_flamework_pipeline(
	dir: &str,
	program: &'static Program,
	opts: &ProjectOptions,
) -> Result<Option<FlameworkPipeline>, Failure> {
	let Some(configured) = crate::flamework_config::prepare_flamework_config(dir, &program.command_line())? else {
		return Ok(None);
	};
	if let Err(message) = reject_dirty_flamework_incremental_state(dir, program) {
		return Err(Failure::strings(
			vec![message.to_owned()],
			Error::message(format!("compile: native Flamework incremental state: {message}")),
		));
	}
	let plugins = effective_transformer_plugins(&program.options().config_file_path).map_err(|error| {
		Failure::strings(
			vec![error.to_string()],
			Error::message("compile: resolve effective transformer plugins"),
		)
	})?;
	let (prefix, suffix) = split_transformer_plugins(&plugins, &configured.after).map_err(|error| {
		Failure::strings(
			vec![format!(
				"{}: {error}",
				gopath::join(&[&gopath::from_slash(dir), "rotor.toml"])
			)],
			Error::message("compile: invalid flamework.after"),
		)
	})?;
	// OpenProject takes a single root; Flamework's createPathTranslator resolves that from rootDir
	// or rootDirs (plus commonSourceDirectory).
	let options = program.options();
	let project = rotor_flamework::open_project(&rotor_flamework::ProjectOptions {
		project_dir: gopath::from_slash(dir).into_owned(),
		root_dir: create_path_translator(program, false).root_dir,
		out_dir: gopath::from_slash(&options.out_dir).into_owned(),
		include_directory: opts.include_path.clone(),
		no_include: !opts.emit_include_files,
		rojo_config_path: opts.rojo_config_path.clone(),
		declaration: options.get_emit_declarations(),
		config: configured.clone(),
		rojo_resolvers: opts.rojo_cache.clone(),
	})
	.map_err(|error| {
		Failure::strings(
			vec![error.message().to_owned()],
			Error::message(format!("compile: open native Flamework project: {error}")),
		)
	})?;
	Ok(Some(FlameworkPipeline {
		config: configured,
		project: Arc::new(Mutex::new(project)),
		program,
		plugins,
		prefix,
		suffix,
	}))
}

/// `rejectDirtyFlameworkIncrementalState`: an incremental build info without the Flamework
/// build data it was made with.
fn reject_dirty_flamework_incremental_state(dir: &str, program: &Program) -> Result<(), &'static str> {
	let options = program.options();
	if !options.is_incremental() || options.ts_build_info_file.is_empty() {
		return Ok(());
	}
	match std::fs::metadata(gopath::join(&[&gopath::from_slash(dir), "flamework.build"])) {
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
		_ => return Ok(()),
	}
	if std::fs::metadata(create_path_translator(program, true).build_info_output_path).is_ok() {
		return Err(DIRTY_FLAMEWORK_INCREMENTAL_STATE);
	}
	Ok(())
}

/// `runCompilePipelineWithOptions`: the transformer stages for one build. Declarations are not
/// among them: they are emitted natively from the original program, never from the overlaid
/// program a transform produces.
pub(crate) fn run_compile_pipeline_with_options(
	dir: &str,
	program: &'static Program,
	source_files: &[P<SourceFile>],
	overlays: &BTreeMap<String, String>,
	pipeline: Option<&FlameworkPipeline>,
	opts: &ProjectOptions,
) -> Result<CompilePipelineResult, Failure> {
	let cancel = opts.context.as_ref();
	let base = transformer_host_for(opts);
	let prefetching = opts
		.declaration_prefetch
		.as_ref()
		.map(|prefetch| crate::declaration_prefetch::PrefetchingTransformerHost::new(base, prefetch));
	let host: &dyn TransformerHost = prefetching
		.as_ref()
		.map_or(base, |prefetching| prefetching as &dyn TransformerHost);
	// Project setup already rejected unmatched overlays (or left them to the solution-wide
	// check); the transformer and native stages need only this program's exact spellings.
	let overlays = if overlays.is_empty() {
		BTreeMap::new()
	} else {
		crate::overlayfs::rekey_overlays_to_program(program, overlays).0
	};
	let Some(pipeline) = pipeline else {
		let prepared = prepare_transformer_program_with_host(cancel, host, dir, program, source_files, &overlays)?;
		return Ok(CompilePipelineResult {
			prepared,
			flamework_project: None,
		});
	};
	// An incremental build that selected nothing has no source to transform, and every stage
	// below would hand the worker an empty compile list, which still costs it a whole program.
	if source_files.is_empty() {
		return Ok(CompilePipelineResult {
			prepared: PreparedTransformerProgram::plain(program, Vec::new(), Some(pipeline.config.clone())),
			flamework_project: Some(Arc::clone(&pipeline.project)),
		});
	}
	let state = Arc::new(Mutex::new(SidecarBuildState::default()));
	let original = program;
	let mut program = program;
	let mut current_files = source_files.to_vec();
	let mut traces = DiagnosticTraces::default();
	if !pipeline.prefix.is_empty() {
		// This prefix is the only external call of a one-shot project task: the suffix is empty
		// and native Flamework never calls the sidecar. The split attestation is scoped to this
		// call.
		let parallel = parallel_prefix_context(dir, Some(pipeline), opts);
		let prepared = apply_external_transformer_stage(
			cancel,
			host,
			dir,
			program,
			&current_files,
			&overlays,
			&traces,
			&pipeline.prefix,
			Some(&state),
			TRANSFORMER_ORIGINAL_AUTHORED,
			parallel.as_ref(),
		)?;
		program = prepared.program;
		current_files = prepared.source_files;
		traces = prepared.source_traces;
	}
	let mut prepared = apply_native_flamework_transform(program, &current_files, &traces, pipeline)?;
	if !std::ptr::eq(program, original) && !std::ptr::eq(program, prepared.program) {
		release_transformed_checkers(program);
	}
	lock(&state).absorb_source_files(&prepared.source_files);
	if !pipeline.suffix.is_empty() {
		prepared = apply_external_transformer_stage(
			cancel,
			host,
			dir,
			prepared.program,
			&prepared.source_files,
			&overlays,
			&prepared.source_traces,
			&pipeline.suffix,
			Some(&state),
			"",
			None,
		)?;
	}
	prepared.flamework = Some(pipeline.config.clone());
	lock(&state).apply_to(&mut prepared);
	Ok(CompilePipelineResult {
		prepared,
		flamework_project: Some(Arc::clone(&pipeline.project)),
	})
}

/// rotor-rs: frees the checkers of `program`, a transform's input that the transform's output
/// replaced. Go's collector frees them once the program is unreachable; `ProgramScope` would hold
/// them to the end of the build, beside the checkers of the programs after it.
#[expect(unsafe_code, reason = "frees the checkers of a program nothing checks any more")]
pub(crate) fn release_transformed_checkers(program: &'static Program) {
	// SAFETY: the transform dropped its checker handle, and what it kept from the checker is
	// printed text, traces and owned diagnostics. Later stages check the replacing program, and
	// the build reads `program` only for its files.
	unsafe { rotor_tsrs::compiler::release_checkers(program) };
}

/// `applyExternalTransformerStage`: one sidecar stage over `source_files`, its traces composed
/// with the incoming ones. Under the Node sidecar, a ported jecs build at the head of the stage
/// runs natively first.
#[expect(clippy::too_many_arguments, reason = "Go's signature")]
pub(crate) fn apply_external_transformer_stage(
	cancel: Option<&Cancellation>,
	host: &dyn TransformerHost,
	dir: &str,
	program: &'static Program,
	source_files: &[P<SourceFile>],
	overlays: &BTreeMap<String, String>,
	traces: &DiagnosticTraces,
	plugins: &[TransformerPluginConfig],
	state: Option<&Arc<Mutex<SidecarBuildState>>>,
	source_origin: &str,
	parallel: Option<&ParallelPrefix>,
) -> Result<PreparedTransformerProgram, Failure> {
	let mut rebased = false;
	let result = external_transformer_stage(
		cancel,
		host,
		dir,
		program,
		source_files,
		overlays,
		traces,
		plugins,
		state,
		source_origin,
		parallel,
		&mut rebased,
	);
	// Go's deferred reset: the rebases serve this stage's sidecar call alone.
	if rebased && let Some(state) = state {
		lock(state).rebases.clear();
	}
	result
}

#[expect(clippy::too_many_arguments, reason = "Go's signature")]
fn external_transformer_stage(
	cancel: Option<&Cancellation>,
	host: &dyn TransformerHost,
	dir: &str,
	program: &'static Program,
	source_files: &[P<SourceFile>],
	overlays: &BTreeMap<String, String>,
	traces: &DiagnosticTraces,
	plugins: &[TransformerPluginConfig],
	state: Option<&Arc<Mutex<SidecarBuildState>>>,
	source_origin: &str,
	parallel: Option<&ParallelPrefix>,
	rebased: &mut bool,
) -> Result<PreparedTransformerProgram, Failure> {
	let input_traces = traces;
	let mut program = program;
	let mut source_files = source_files.to_vec();
	let mut native_traces: Option<DiagnosticTraces> = None;
	let mut plugins = plugins.to_vec();
	if uses_subprocess_sidecar(host)
		&& let (Some(config), rest) = native_jecs_stage(dir, &plugins)
	{
		// A parallel transformer split admits only a worker no request has used yet, so a warmed
		// worker would turn it away.
		if !rest.is_empty() && parallel.is_none() {
			let mut config_path = program.options().config_file_path.clone();
			if config_path.is_empty() {
				config_path =
					gopath::to_slash(&gopath::join(&[&gopath::from_slash(dir), "tsconfig.json"])).into_owned();
			}
			warm_transformer_sidecar_shards(
				dir,
				&config_path,
				Some(&raw_transformer_plugins(&rest)),
				source_files.len(),
			);
		}
		let native = apply_native_jecs_transform(program, &source_files, traces, &config)
			.map_err(|error| Failure::strings(vec![error.to_string()], error))?;
		if rest.is_empty() {
			return Ok(native);
		}
		program = native.program;
		source_files = native.source_files;
		native_traces = Some(native.source_traces);
		plugins = rest;
		if let Some(state) = state {
			lock(state).rebases = native.rebases;
			*rebased = true;
		}
	}
	let traces = native_traces.as_ref().unwrap_or(traces);
	let raw_plugins = raw_transformer_plugins(&plugins);
	let mut transformed = apply_transformer_sidecar_with_plugins_and_host(
		cancel,
		host,
		dir,
		program,
		&source_files,
		overlays,
		Some(&raw_plugins),
		state,
		source_origin,
		parallel,
	)?;
	let remapped = if std::ptr::eq(transformed.program, program) {
		source_files
	} else {
		remap_program_source_files(transformed.program, &source_files).map_err(Failure::bare)?
	};
	let rewritten = state.map(|state| lock(state).rebases.clone()).unwrap_or_default();
	let mut composed = traces.clone();
	for (key, trace) in std::mem::take(&mut transformed.source_traces.0) {
		// A rebased file came back traced to the text native jecs started from, not to its
		// rewrite.
		let inner = if rewritten.contains_key(&key) {
			input_traces.0.get(&key).cloned()
		} else {
			traces.0.get(&key).cloned()
		};
		match compose_source_trace_maps(Some(trace), inner) {
			Some(trace) => {
				composed.0.insert(key, trace);
			}
			None => {
				composed.0.remove(&key);
			}
		}
	}
	transformed.source_files = remapped;
	transformed.source_traces = composed;
	Ok(transformed)
}

/// `rawTransformerPlugins`
pub(crate) fn raw_transformer_plugins(plugins: &[TransformerPluginConfig]) -> Vec<String> {
	plugins.iter().map(TransformerPluginConfig::marshal_json).collect()
}

/// `printFlameworkSource`
pub(crate) fn print_flamework_source(
	source_file: P<SourceFile>,
	metadata: &rotor_flamework::SourceMetadata,
) -> Result<NativeSourceOverlay, Error> {
	let (text, trace) = print_native_source(
		source_file,
		metadata.original(),
		metadata.emit_context(),
		metadata.trace().original_file_name(),
		metadata.trace().original_text(),
	)?;
	Ok(NativeSourceOverlay {
		file_name: metadata.file_name().to_owned(),
		text,
		trace: Some(Arc::new(trace)),
		rebase: None,
	})
}

/// `applyNativeFlameworkTransform`
pub(crate) fn apply_native_flamework_transform(
	program: &'static Program,
	source_files: &[P<SourceFile>],
	traces: &DiagnosticTraces,
	pipeline: &FlameworkPipeline,
) -> Result<PreparedTransformerProgram, Failure> {
	let config_path = program.options().config_file_path.clone();
	let stop = log_stage_named(&config_path, "native Flamework transform");
	let result = native_flamework_transform(&config_path, program, source_files, traces, pipeline);
	stop();
	result
}

fn native_flamework_transform(
	config_path: &str,
	program: &'static Program,
	source_files: &[P<SourceFile>],
	traces: &DiagnosticTraces,
	pipeline: &FlameworkPipeline,
) -> Result<PreparedTransformerProgram, Failure> {
	let stop_checker = log_stage_named(config_path, "native Flamework checker");
	let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
	stop_checker();
	// Upstream always prints a fresh source-file update, so by default every source is overlaid
	// back into the program. Only flamework.skipUnchangedFiles opts back into reusing unchanged
	// files. Printing happens in the transform's own workers, each holding its file's emit
	// context.
	let skip_unchanged = pipeline.config.skip_unchanged_files;
	let stop_visit = log_stage_named(config_path, "native Flamework visit and print");
	let mut project = lock(&pipeline.project);
	let result = rotor_flamework::transform(rotor_flamework::TransformInput {
		program,
		checker: &mut checker,
		files: source_files.to_vec(),
		project: &mut project,
		analyses: Vec::new(),
		macro_runtime: None,
		print: Some(Box::new(move |metadata: &rotor_flamework::SourceMetadata| {
			if skip_unchanged && !metadata.changed() {
				return Ok(None);
			}
			let overlay = print_flamework_source(metadata.transformed(), metadata)
				.map_err(|error| rotor_flamework::Error::msg(error.to_string()))?;
			Ok(Some(Box::new(overlay) as Box<dyn Any + Send>))
		})),
	});
	drop(project);
	// Held for the transform alone: the program update below builds a new program.
	drop(checker);
	stop_visit();
	let mut result = result
		.map_err(|error| Failure::bare(Error::message(format!("compile: native Flamework transform: {error}"))))?;
	if !result.diagnostics.is_empty() {
		let messages = result
			.diagnostics
			.iter()
			.map(|diagnostic| diagnostic.message.clone())
			.collect();
		return Err(Failure::strings(
			messages,
			Error::message("compile: native Flamework diagnostics"),
		));
	}

	let mut changed = Vec::with_capacity(result.sources.len());
	for metadata in &mut result.sources {
		if let Some(printed) = metadata.take_printed()
			&& let Ok(overlay) = printed.downcast::<NativeSourceOverlay>()
		{
			changed.push(*overlay);
		}
	}
	drop(result);
	let stop_update = log_stage_named(config_path, "native Flamework program update");
	let updated = update_native_flamework_program(NativeProgramUpdate {
		program,
		source_files,
		traces,
		overlays: changed,
	});
	stop_update();
	let (transformed_program, remapped, composed) = updated.map_err(Failure::bare)?;
	let mut prepared = PreparedTransformerProgram::plain(transformed_program, remapped, None);
	prepared.source_traces = composed;
	Ok(prepared)
}
