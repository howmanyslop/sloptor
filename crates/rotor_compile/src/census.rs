//! `census.go`: census mode, which classifies every file of a project instead of stopping at the
//! first failure.
//!
//! The stock pipeline is four gates: program option diagnostics, the per-file precheck, the
//! global checker diagnostics and the transform drain. Each returns at its first failure, so one
//! type error hides every transformer diagnostic. Census mode turns the gates into
//! classifications: every file runs and every file gets an outcome.

use std::sync::{Arc, Mutex};

use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;

use crate::compile::{DiagnosticInfo, Error, InternalCompilerError, line_col_in, ts_diagnostic_infos};
use crate::project::{
	CompiledProjectSourceFile, PrecheckedProjectSourceFile, ProjectOptions, compile_project_program, lock,
	new_project_program_with_options,
};
use crate::source_trace::DiagnosticTraces;

/// `FileOutcome`: how one source file fared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOutcome {
	/// `ok`: the file transformed with no diagnostics. It does not mean the Luau is correct: the
	/// transformer reads types, so a type-broken file's output can be silently wrong.
	Ok,
	/// `typeError`: TypeScript rejected the file. It is transformed anyway.
	TypeError,
	/// `transformerDiagnostic`: rotor's transformer reported a diagnostic, or the file uses
	/// comment directives.
	TransformerDiagnostic,
	/// `internalCompilerError`: a ported upstream assert fired.
	InternalCompilerError,
}

impl FileOutcome {
	/// The outcome's spelling.
	#[must_use]
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Ok => "ok",
			Self::TypeError => "typeError",
			Self::TransformerDiagnostic => "transformerDiagnostic",
			Self::InternalCompilerError => "internalCompilerError",
		}
	}
}

/// `FileDiagnostics`: one source file's census entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDiagnostics {
	/// The file's absolute, slash-separated path.
	pub file_name: String,
	/// The most severe outcome: internal compiler error, then transformer diagnostic, then type
	/// error, then ok.
	pub outcome: FileOutcome,
	/// Whether the file reached and completed the transform.
	pub transformed: bool,
	/// Every diagnostic attributed to the file.
	pub diagnostics: Vec<DiagnosticInfo>,
	/// Set only for [`FileOutcome::InternalCompilerError`].
	pub internal_error: Option<InternalCompilerError>,
}

/// `ProjectDiagnostics`: the census of one project.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectDiagnostics {
	/// The directory the project was compiled from.
	pub project_dir: String,
	/// The tsconfig the project was compiled with.
	pub config_path: String,
	/// One entry per compiled source file, in program order.
	pub files: Vec<FileDiagnostics>,
	/// What no single file owns: option diagnostics, global diagnostics, setup failures.
	pub diagnostics: Vec<DiagnosticInfo>,
	/// The entries of `files` that transformed.
	pub transformed: usize,
	/// The overlay keys that named a file of the program.
	pub overlay_matches: usize,
}

/// `censusCollector`: accumulates a census while `compile_project_source_files` runs.
#[derive(Default)]
pub struct CensusCollector {
	state: Mutex<CensusState>,
}

#[derive(Default)]
struct CensusState {
	files: Vec<FileDiagnostics>,
	diagnostics: Vec<DiagnosticInfo>,
	overlay_matches: usize,
	/// The pass's transformer traces, which put a file's positions back into the text on disk.
	traces: DiagnosticTraces,
}

impl CensusCollector {
	/// Records the traces positions remap through.
	pub(crate) fn set_traces(&self, traces: DiagnosticTraces) {
		lock(&self.state).traces = traces;
	}

	/// The project-level diagnostics collected so far.
	#[cfg(test)]
	pub(crate) fn diagnostics(&self) -> Vec<DiagnosticInfo> {
		lock(&self.state).diagnostics.clone()
	}

	/// `addProjectDiagnostics`
	pub(crate) fn add_project_diagnostics(&self, diagnostics: Vec<DiagnosticInfo>) {
		lock(&self.state).diagnostics.extend(diagnostics);
	}

	/// Records how many overlays matched a file of the program.
	pub(crate) fn set_overlay_matches(&self, matched: usize) {
		lock(&self.state).overlay_matches = matched;
	}

	/// `record`: classifies one file from its precheck and transform results.
	pub(crate) fn record(
		&self,
		source_file: P<SourceFile>,
		precheck: &PrecheckedProjectSourceFile,
		result: &CompiledProjectSourceFile,
	) {
		let mut state = lock(&self.state);
		let mut entry = FileDiagnostics {
			file_name: source_file.file_name().to_owned(),
			outcome: FileOutcome::Ok,
			transformed: result.transformed,
			diagnostics: Vec::new(),
			internal_error: None,
		};
		if !precheck.ts_diags.is_empty() {
			entry.outcome = FileOutcome::TypeError;
			entry
				.diagnostics
				.extend(ts_diagnostic_infos(&precheck.ts_diags, &state.traces));
		}
		if !precheck.comment_diags.is_empty() {
			entry.outcome = FileOutcome::TransformerDiagnostic;
			let infos = comment_directive_infos(source_file, &precheck.comment_diags);
			entry
				.diagnostics
				.extend(state.traces.remap_all(infos, source_file.text()));
		}
		if !result.diags.is_empty() {
			entry.outcome = FileOutcome::TransformerDiagnostic;
			entry.diagnostics.extend(result.diags.iter().cloned());
		}
		if let Some(error) = &result.err {
			if let Error::InternalCompiler(ice) = error {
				entry.outcome = FileOutcome::InternalCompilerError;
				entry.internal_error = Some(ice.clone());
			} else {
				// Anything else the transform failed with is rotor rejecting the file, the same
				// class as a transformer diagnostic.
				entry.outcome = FileOutcome::TransformerDiagnostic;
				if result.diags.is_empty() {
					entry.diagnostics.push(DiagnosticInfo {
						message: error.to_string(),
						file_name: entry.file_name.clone(),
						..DiagnosticInfo::default()
					});
				}
			}
		}
		state.files.push(entry);
	}
}

/// `absSlashPath`: an absolute, slash-separated path; empty stays empty.
pub(crate) fn abs_slash_path(path: &str) -> String {
	if path.is_empty() {
		return String::new();
	}
	gopath::abs(path).map_or_else(
		|_| gopath::to_slash(path).into_owned(),
		|absolute| gopath::to_slash(&absolute).into_owned(),
	)
}

/// `commentDirectiveInfos`: the file and position of each comment directive message. The
/// `@ts-nocheck` message has no directive range, so it carries the file only.
pub(crate) fn comment_directive_infos(source_file: P<SourceFile>, messages: &[String]) -> Vec<DiagnosticInfo> {
	let file_name = source_file.file_name();
	let directives = source_file.comment_directives.get();
	messages
		.iter()
		.enumerate()
		.map(|(index, message)| {
			let mut info = DiagnosticInfo {
				message: message.clone(),
				file_name: file_name.to_owned(),
				..DiagnosticInfo::default()
			};
			if let Some(directive) = directives.get(index) {
				info.offset = directive.loc.pos().source_offset().map_or(0, |offset| offset as usize);
				info.len = directive.loc.len().map_or(0, |len| len as usize);
				(info.line, info.col) = line_col_in(source_file.text(), info.offset);
			}
			info
		})
		.collect()
}

/// `CompileProjectDiagnostics`: compiles every file of the project and reports what happened to
/// each, writing nothing.
///
/// A returned error means the project could not be set up far enough to census; the diagnostics
/// collected before that point are in the result.
#[must_use]
pub fn compile_project_diagnostics(project_dir: &str, opts: &ProjectOptions) -> (ProjectDiagnostics, Option<Error>) {
	let collector = Arc::new(CensusCollector::default());
	let mut opts = opts.clone();
	opts.census = Some(Arc::clone(&collector));

	// Seeded in the form the program uses, so a project that fails before it has a program
	// attributes its diagnostics like every other project of a solution census.
	let mut census = ProjectDiagnostics {
		project_dir: abs_slash_path(project_dir),
		config_path: abs_slash_path(&opts.ts_config_path),
		..ProjectDiagnostics::default()
	};
	let (dir, program) = match new_project_program_with_options(project_dir, &opts.ts_config_path, &opts) {
		Ok(loaded) => loaded,
		Err(failure) => {
			census.diagnostics.extend(failure.diagnostics);
			return (census, Some(failure.error));
		}
	};
	census.project_dir.clone_from(&dir);
	let config_file_path = program.options().config_file_path.clone();
	if !config_file_path.is_empty() {
		census.config_path = gopath::to_slash(&config_file_path).into_owned();
	}

	let compiled = compile_project_program(&dir, program, &opts);
	let state = std::mem::take(&mut *lock(&collector.state));
	census.files = state.files;
	census.overlay_matches = state.overlay_matches;
	census.diagnostics.extend(state.diagnostics);
	census.transformed = census.files.iter().filter(|file| file.transformed).count();
	match compiled {
		Ok(_) => (census, None),
		Err(failure) => {
			// Setup failures never reach the per-file stage, so their diagnostics exist only here.
			if census.files.is_empty() {
				census.diagnostics.extend(failure.diagnostics);
			}
			(census, Some(failure.error))
		}
	}
}
