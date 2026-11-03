//! `census.go`.
//!
//! Census mode (`CompileProjectDiagnostics`) is Stage 5b. The single-project pipeline only reaches
//! the collector when census mode is on, which nothing in Stage 5a turns on; this is the
//! collector's shape, with Go's no-census behavior.

use std::sync::Mutex;

use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;

use crate::compile::DiagnosticInfo;
use crate::project::{CompiledProjectSourceFile, PrecheckedProjectSourceFile, lock};
use crate::source_trace::DiagnosticTraces;

/// `censusCollector`: records every file's outcome instead of stopping at the first failure.
#[derive(Default)]
pub struct CensusCollector {
	state: Mutex<CensusState>,
}

#[derive(Default)]
struct CensusState {
	traces: DiagnosticTraces,
	project_diagnostics: Vec<DiagnosticInfo>,
	overlay_matches: usize,
	recorded: usize,
}

impl CensusCollector {
	/// Records the traces positions remap through.
	pub(crate) fn set_traces(&self, traces: DiagnosticTraces) {
		lock(&self.state).traces = traces;
	}

	/// `addProjectDiagnostics`
	pub(crate) fn add_project_diagnostics(&self, diagnostics: Vec<DiagnosticInfo>) {
		lock(&self.state).project_diagnostics.extend(diagnostics);
	}

	/// Records how many overlays matched a file of the program.
	pub(crate) fn set_overlay_matches(&self, matched: usize) {
		lock(&self.state).overlay_matches = matched;
	}

	/// `record`: Stage 5b classifies the outcome; this counts it.
	pub(crate) fn record(
		&self,
		_source_file: P<SourceFile>,
		_precheck: &PrecheckedProjectSourceFile,
		_result: &CompiledProjectSourceFile,
	) {
		lock(&self.state).recorded += 1;
	}
}
