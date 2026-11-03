//! `declaration_prefetch.go`: emits a project's declaration text during the transformer sidecar
//! round trip, which otherwise leaves the build idle while Node runs the plugins.
//!
//! The emit must finish before any transform reuses the original program's nodes; the
//! prefetching transformer host (Stage 5b's sidecar) runs [`DeclarationPrefetch::run`] beside
//! the round trip and waits for it.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use rotor_support::logservice;
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::Program;

use crate::compile::Error;
use crate::output::{DeclarationEmitFile, declaration_emit_source_files, emit_declaration_texts_until};
use crate::project::lock;
use crate::timings::{BuildTimings, Stage};

/// `declarationPrefetch`
pub struct DeclarationPrefetch {
	program: &'static Program,
	files: Vec<P<SourceFile>>,
	timings: Option<Arc<BuildTimings>>,
	once: OnceLock<()>,
	result: Mutex<Option<Result<Vec<DeclarationEmitFile>, Error>>>,
}

impl DeclarationPrefetch {
	/// `newDeclarationPrefetch`: `None` when the project emits no declarations for the selection.
	pub(crate) fn new(
		program: &'static Program,
		selected_paths: Option<&HashSet<String>>,
		timings: Option<Arc<BuildTimings>>,
	) -> Option<Arc<Self>> {
		if !program.options().get_emit_declarations() {
			return None;
		}
		let files = declaration_emit_source_files(program, selected_paths);
		if files.is_empty() {
			return None;
		}
		Some(Arc::new(Self {
			program,
			files,
			timings,
			once: OnceLock::new(),
			result: Mutex::new(None),
		}))
	}

	/// `run`: emits the declarations once. Setting `abandon` stops it between files and leaves no
	/// result, so the build emits after the transform as usual.
	pub fn run(&self, abandon: &AtomicBool) {
		self.once.get_or_init(|| {
			let stop = crate::timings::start_stage(self.timings.as_deref(), Stage::DeclarationEmit);
			let emitted = logservice::benchmark_if_verbose("declaration emit", || {
				emit_declaration_texts_until(self.program, &self.files, Some(abandon))
			});
			stop.stop();
			if abandon.load(Ordering::Relaxed) && emitted.is_err() {
				return;
			}
			*lock(&self.result) = Some(emitted);
		});
	}

	/// `result`: the prefetched text, `None` when no round trip produced it.
	pub(crate) fn result(&self) -> Option<Result<Vec<DeclarationEmitFile>, Error>> {
		lock(&self.result).clone()
	}
}
