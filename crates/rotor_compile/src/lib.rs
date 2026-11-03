//! Compile orchestration, emit and the incremental cache: the port of Go's `internal/compile`.
//!
//! Stage 5a ports the single-project pipeline: [`compile::compile_file`] (one file),
//! [`project::compile_project`] (every file, in memory) and [`output::build_project_with_options`]
//! (the build that writes outputs, declarations and the incremental manifest). `COMPILE.md` maps
//! every Go file to its module, and to the stage that ports it. Stage 5b adds the solution
//! builder ([`solution_drain::build_solution_with_options`]), census mode and the Node
//! transformer-plugin sidecar.

pub mod afterdeclarations;
pub mod census;
pub mod census_solution;
pub mod compile;
pub mod compiler_options;
pub mod copyfilesgate;
pub mod crossproject;
pub mod declaration_prefetch;
pub mod declmap;
pub mod declpaths;
pub mod envdecl;
pub mod filemacro;
pub mod flamework_config;
pub mod flamework_pipeline;
pub mod flamework_program_update;
pub mod gojson;
pub mod incremental_flamework;
pub mod incremental_manifest;
pub mod incremental_paths;
pub mod incremental_plugin_salt;
pub mod incremental_salt;
pub mod incremental_signature;
pub mod jecs_native;
pub mod jecs_rebase;
pub mod luau_types;
pub mod macro_inputs;
pub mod macrodecl;
pub mod native_types;
pub mod output;
pub mod output_stamps;
pub mod outputwriter;
pub mod outputwriter_manifest;
pub mod overlayfs;
pub mod package_layout;
pub mod package_layout_watch;
pub mod packageexports;
pub mod packageexports_parse;
pub mod parallelwrite;
pub mod project;
pub mod project_config;
pub mod project_references;
pub mod rbxts_options;
pub mod rbxts_schema;
pub mod reference_validation;
pub mod rotortypes;
pub mod sanitize;
pub mod sidecar;
pub mod sidecar_install;
pub mod sidecar_parallel;
pub mod sidecar_roots;
pub mod sidecar_shards;
pub mod solution;
pub mod solution_cache;
pub mod solution_drain;
pub mod solution_satisfied;
pub mod solution_scheduler;
pub mod solution_watch;
pub mod source_trace;
pub mod source_trace_sourcemap;
pub mod timings;
pub mod transformer_host;
pub mod transformer_host_sidecar;
pub mod transformer_order;

#[cfg(test)]
mod tests;

pub use compile::{DiagnosticInfo, Error, Failure, FileOutput, InternalCompilerError};
pub use output::{BuildFailure, BuildResult, build_project_with_options};
pub use project::{ProjectOptions, compile_project, compile_project_with_options};
pub use solution::{SolutionCoordinator, SolutionGraph, SolutionProject, build_solution_graph};
pub use solution_drain::{build_solution_with_options, effective_solution_builders};
