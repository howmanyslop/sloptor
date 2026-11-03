//! The one layer through which rotor talks to the vendored tsrs checker.
//!
//! Other crates depend on `rotor_tsrs` only, never on a `tsrs_*` crate. The tsrs crates they need
//! are re-exported as modules ([`ast`], [`compiler`], [`printer`], ...), and rotor's own layer sits
//! on top:
//!
//! - [`project`]: loading a tsconfig the way rotor's build does, derived programs through
//!   `update_program`, and per-file checker access;
//! - [`tsconfig`]: the tsconfig rewrites and rbxtsc's option validation;
//! - [`checker`]: [`checker::TypeChecker`], the checker queries rotor makes under their Go names;
//! - [`types`]: the type predicates of `internal/transformer/types.go`;
//! - [`dump`]: the per-node differential against `tools/tsrs-oracle`;
//! - [`replay`]: the spike's query replay.
//!
//! `API.md` maps every typescript-go API the Go code calls to its home here.

pub mod ast;
pub mod checker;
pub mod dump;
pub mod project;
pub mod replay;
pub mod tsconfig;
pub mod types;

use std::path::Path;

pub use project::{LoadError, Project, ProjectOptions, load_project, load_project_in};
pub use tsrs_compiler as compiler;
pub use tsrs_core as tscore;
pub use tsrs_core::{CompilerOptions, Context, P, Tristate, jsnum, tspath};
pub use tsrs_diagnostics as diagnostics;
pub use tsrs_module as module;
pub use tsrs_parser as parser;
pub use tsrs_printer as printer;
pub use tsrs_scanner as scanner;
pub use tsrs_sourcemap as sourcemap;
pub use tsrs_tsoptions as tsoptions;
pub use tsrs_vfs as vfs;

// Unit tests are handed the dev-dependencies too; only the integration tests use this one.
#[cfg(test)]
use tempfile as _;

/// The checker recurses deeply, and tsrs runs it on 512 MiB stacks.
const CHECKER_STACK_BYTES: usize = 512 << 20;

/// Runs `work` on a thread with the stack size the checker needs.
///
/// # Panics
///
/// Resumes the panic of `work`, if it panics.
pub fn with_large_stack<R: Send>(work: impl FnOnce() -> R + Send) -> R {
	std::thread::scope(|scope| {
		let spawned = std::thread::Builder::new()
			.name("rotor-tsrs".to_owned())
			.stack_size(CHECKER_STACK_BYTES)
			.spawn_scoped(scope, work);
		match spawned {
			Ok(handle) => handle
				.join()
				.unwrap_or_else(|payload| std::panic::resume_unwind(payload)),
			Err(error) => std::panic::resume_unwind(Box::new(error)),
		}
	})
}

/// What loading a tsconfig produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramSummary {
	/// Source files in the program, including declaration files.
	pub source_files: usize,
	/// Program, syntactic, semantic and global diagnostics, of any category.
	pub diagnostics: usize,
}

/// Loads `tsconfig` the way rotor does and returns its source file and diagnostic counts.
///
/// Runs on a large-stack thread, so it is safe to call from a test or any other thread.
///
/// # Errors
///
/// Returns [`LoadError`] if the project cannot be loaded.
pub fn load_program(tsconfig: &Path) -> Result<ProgramSummary, LoadError> {
	with_large_stack(|| {
		let Project { program } = load_project(tsconfig, &ProjectOptions::default())?;
		let context = Context::default();
		let counts: [usize; 4] = [
			program.get_program_diagnostics().len(),
			program.get_syntactic_diagnostics(&context, None).len(),
			program.get_semantic_diagnostics(&context, None).len(),
			program.get_global_diagnostics(&context).len(),
		];
		Ok(ProgramSummary {
			source_files: program.source_files().len(),
			diagnostics: counts.iter().sum(),
		})
	})
}
