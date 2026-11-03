//! `flamework_pipeline.go`.
//!
//! The transformer stages of one build. Stage 5a ports the route without native Flamework (Go's
//! `pipeline == nil`); the native Flamework stages are Stage 6 and the external transformer stages
//! Stage 5b, and both fail with a not-yet-ported diagnostic.

use std::collections::BTreeMap;

use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::Program;

use crate::compile::Failure;
use crate::project::ProjectOptions;
use crate::sidecar::{
	PreparedTransformerProgram, not_yet_ported, prepare_transformer_program_with_host, transformer_host_for,
};

/// `flameworkPipeline`: the native Flamework stages (Stage 6).
pub struct FlameworkPipeline {
	/// The active configuration.
	pub config: rotor_config::config::FlameworkConfig,
}

/// `compilePipelineResult`
pub(crate) struct CompilePipelineResult {
	pub(crate) prepared: PreparedTransformerProgram,
}

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

/// `prepareFlameworkPipeline`: `None` with Flamework off. With Flamework on, native Flamework is
/// Stage 6.
pub(crate) fn prepare_flamework_pipeline(
	dir: &str,
	program: &'static Program,
	_opts: &ProjectOptions,
) -> Result<Option<FlameworkPipeline>, Failure> {
	let Some(config) = crate::flamework_config::prepare_flamework_config(dir, &program.command_line())? else {
		return Ok(None);
	};
	let _ = FlameworkPipeline { config };
	Err(not_yet_ported("native Flamework"))
}

/// `runCompilePipelineWithOptions`: without a Flamework pipeline, the transformer program
/// (the original program when the project has no transformer plugins).
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
	let host: &dyn crate::transformer_host::TransformerHost = prefetching.as_ref().map_or(base, |prefetching| {
		prefetching as &dyn crate::transformer_host::TransformerHost
	});
	// Project setup already rejected unmatched overlays (or left them to the solution-wide
	// check); the transformer stages need only this program's exact spellings.
	let overlays = if overlays.is_empty() {
		BTreeMap::new()
	} else {
		crate::overlayfs::rekey_overlays_to_program(program, overlays).0
	};
	if pipeline.is_some() {
		return Err(not_yet_ported("native Flamework"));
	}
	let prepared = prepare_transformer_program_with_host(cancel, host, dir, program, source_files, &overlays)?;
	Ok(CompilePipelineResult { prepared })
}
