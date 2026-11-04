//! `compile.go`: the single-file fast path ([`compile_file`]) and the diagnostic plumbing every
//! entry point shares.
//!
//! [`compile_file`] builds the same program and project context as
//! [`crate::project::compile_project`] but transforms only the requested file, so per-fixture
//! tests stay isolated and fast.

use std::fmt;

use rotor_luau::render::render_ast_bytes;
use rotor_transformer::{Diagnostic, MultiState, State, StateOptions, transform_source_file};
use rotor_tsrs::ast::{self, SourceFile};
use rotor_tsrs::compiler::Program;
use rotor_tsrs::diagnostics::Category;
use rotor_tsrs::{Context, P};

use crate::flamework_pipeline::prepare_compile_pipeline;
use crate::project::{ProjectOptions, new_project_context, new_project_program_with_options};
use crate::source_trace::DiagnosticTraces;

/// `DiagnosticInfo`: the structured form of a compile diagnostic.
///
/// Every diagnostic carries a stable `code`: the upstream-style factory name for a rotor
/// transformer diagnostic (`noAny`), and `TS####` for a TypeScript one. It is empty only where
/// there was never one, a project-setup failure reported as a bare message. `file_name`, `offset`
/// and `len` locate the span; `file_name` is empty without a location and `len` is 0 without a
/// usable span. `line` and `col` are 1-based, resolved against the text the compile used, and 0
/// without a location.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticInfo {
	/// The diagnostic code.
	pub code: String,
	/// The message.
	pub message: String,
	/// Whether this is a warning.
	pub warning: bool,
	/// The file the span is in; empty without a location.
	pub file_name: String,
	/// The byte offset of the span into the file's source.
	pub offset: usize,
	/// The span length in bytes; 0 means no usable span.
	pub len: usize,
	/// The 1-based line of `offset`.
	pub line: usize,
	/// The 1-based column of `offset`.
	pub col: usize,
	/// tsc's related information ("'x' is declared here"), for the human renderer only; no
	/// `--json` shape carries it.
	pub related: Vec<RelatedInfo>,
}

/// One related-information entry of a [`DiagnosticInfo`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelatedInfo {
	/// The message.
	pub message: String,
	/// The file the span is in; empty without a location.
	pub file_name: String,
	/// The byte offset of the span.
	pub offset: usize,
	/// The span length in bytes.
	pub len: usize,
}

/// `InternalCompilerError`.
///
/// The recovered form of a transformer panic. The Rust transformer reports its invariant violations
/// as an internal error on the state rather than panicking; either way the file has no text and no
/// diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalCompilerError {
	/// The source file being transformed; empty outside a file's transform.
	pub file_name: String,
	/// The panic value's text.
	pub value: String,
}

impl fmt::Display for InternalCompilerError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(formatter, "internal compiler error: {}", self.value)
	}
}

/// A failed compile's error, whose text is Go's `error.Error()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
	/// An `errors.New` or `fmt.Errorf` error.
	Message(String),
	/// A recovered transformer panic.
	InternalCompiler(InternalCompilerError),
	/// `declarationEmitDiagnosticError`: the declaration emit reported diagnostics.
	DeclarationEmitDiagnostics(Vec<DiagnosticInfo>),
}

impl Error {
	/// An error with Go's message.
	pub fn message(text: impl Into<String>) -> Self {
		Self::Message(text.into())
	}
}

impl fmt::Display for Error {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Message(text) => formatter.write_str(text),
			Self::InternalCompiler(error) => error.fmt(formatter),
			Self::DeclarationEmitDiagnostics(_) => formatter.write_str("compile: declaration emit diagnostics"),
		}
	}
}

impl std::error::Error for Error {}

/// A failed compile: Go's `(diagnostics, error)` pair, the diagnostics returned beside the error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
	/// The diagnostics that explain the failure; may be empty.
	pub diagnostics: Vec<DiagnosticInfo>,
	/// The error.
	pub error: Error,
}

impl Failure {
	/// A failure with message-only diagnostics (`stringDiagnostics`).
	#[must_use]
	pub fn strings(diagnostics: Vec<String>, error: Error) -> Self {
		Self {
			diagnostics: string_diagnostics(diagnostics),
			error,
		}
	}

	/// A failure without diagnostics.
	#[must_use]
	pub const fn bare(error: Error) -> Self {
		Self {
			diagnostics: Vec::new(),
			error,
		}
	}

	/// Go's diagnostics-as-strings view (`diagnosticInfoMessages`).
	#[must_use]
	pub fn messages(&self) -> Vec<String> {
		diagnostic_info_messages(&self.diagnostics)
	}
}

impl fmt::Display for Failure {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.error.fmt(formatter)
	}
}

impl std::error::Error for Failure {}

/// What a file's transform produced: the rendered bytes and the diagnostics. The text is empty
/// when the diagnostics hold an error (upstream bails before rendering).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileOutput {
	/// The rendered Luau, as bytes: a `$file` string literal keeps a non-UTF-8 file's bytes.
	pub text: Vec<u8>,
	/// The transformer's diagnostics.
	pub diagnostics: Vec<DiagnosticInfo>,
}

/// `CompileFile`: compiles `project_dir/rel_path` to Luau.
///
/// # Errors
///
/// A [`Failure`] when the project cannot be set up, the file has TypeScript or comment directive
/// diagnostics, or the transform fails internally. Transformer diagnostics are not an error: they
/// come back with empty text.
pub fn compile_file(project_dir: &str, rel_path: &str) -> Result<(Vec<u8>, Vec<String>), Failure> {
	compile_file_with_options(project_dir, rel_path, &ProjectOptions::default())
}

/// `CompileFileDetailed`: [`compile_file`] keeping the structured diagnostics.
///
/// # Errors
///
/// See [`compile_file`].
pub fn compile_file_detailed(project_dir: &str, rel_path: &str) -> Result<FileOutput, Failure> {
	compile_file_detailed_with_options(project_dir, rel_path, &ProjectOptions::default())
}

/// `CompileFileWithOptions`: [`compile_file`] with project options.
///
/// # Errors
///
/// See [`compile_file`].
pub fn compile_file_with_options(
	project_dir: &str,
	rel_path: &str,
	opts: &ProjectOptions,
) -> Result<(Vec<u8>, Vec<String>), Failure> {
	let output = compile_file_detailed_with_options(project_dir, rel_path, opts)?;
	Ok((output.text, diagnostic_info_messages(&output.diagnostics)))
}

/// `CompileFileDetailedWithOptions`: the options-aware single-file fast path.
///
/// Must run on a large stack ([`rotor_tsrs::with_large_stack`]).
///
/// # Errors
///
/// See [`compile_file`].
#[expect(
	clippy::significant_drop_tightening,
	reason = "the state borrows the checker until the file is rendered"
)]
pub fn compile_file_detailed_with_options(
	project_dir: &str,
	rel_path: &str,
	opts: &ProjectOptions,
) -> Result<FileOutput, Failure> {
	let (dir, program) = new_project_program_with_options(project_dir, &opts.ts_config_path, opts)?;

	let file_path = format!("{dir}/{}", rotor_support::gopath::to_slash(rel_path));
	let Some(source_file) = program.get_source_file(&file_path) else {
		return Err(Failure::bare(Error::message(format!(
			"compile: source file not in program: {file_path}"
		))));
	};
	let pipeline = prepare_compile_pipeline(&dir, program, &[source_file], &opts.overlays, opts)?;
	let program = pipeline.prepared.program;
	let source_file = pipeline.prepared.source_files[0];

	let mut pctx = new_project_context(&dir, program, opts)?;
	pctx.source_traces = pipeline.prepared.source_traces;
	let context = Context::default();

	// Program-level option diagnostics plus this file's pre-emit diagnostics (syntactic, semantic
	// and checker globals); any of them fails the compile before transforming, mirroring
	// upstream's pre-emit bail (compileFiles.ts:151-158).
	let mut ts_diags = program.get_program_diagnostics();
	ts_diags.extend(pre_emit_project_file_diagnostics_with_options(
		&context,
		program,
		source_file,
		opts,
	));
	ts_diags.extend(program.get_global_diagnostics(&context));
	if !ts_diags.is_empty() {
		return Err(Failure {
			diagnostics: ts_diagnostic_infos(&ts_diags, &pctx.source_traces),
			error: Error::message("compile: TypeScript diagnostics"),
		});
	}
	if !opts.allow_comment_directives {
		let diags = comment_directive_diagnostics(source_file);
		if !diags.is_empty() {
			return Err(Failure::strings(
				diags,
				Error::message("compile: comment directive diagnostics"),
			));
		}
	}

	let mut checker = program.get_type_checker_for_file(&context, source_file);
	let mut multi = MultiState::new();
	let options = StateOptions {
		rojo: Some(&pctx.rojo_context),
		project_type: Some(pctx.project_type),
		env: Some(&pctx.env),
		files: Some(&*pctx.files),
		optimized_array_appends: opts.optimized_array_appends,
		native_types: pctx.native_types,
		luau_types: pctx.luau_types,
		skip_semantic_diagnostics: opts.skip_semantic_diagnostics,
		..StateOptions::default()
	};
	let mut state = State::new(program, &mut checker, source_file, &mut multi, options);
	// Macro registration audit (digest §6): upstream's MacroManager constructor throws
	// ProjectError before any emit when a registration name fails to resolve.
	let missing = state.multi.macro_registration_failures();
	if !missing.is_empty() {
		return Err(Failure::strings(
			missing,
			Error::message("compile: macro registration failure"),
		));
	}
	let (text, _, diagnostics) = transform_and_render_source_map_detailed(&mut state, None)?;
	Ok(FileOutput { text, diagnostics })
}

/// `transformAndRenderSourceMapDetailed`: transforms and renders one file, with its source map
/// when `source_file` is given.
///
/// An internal error (Go's recovered panic) drops the file's text and diagnostics and fails with
/// an [`InternalCompilerError`]; a render failure is one too, since Go's renderer panics.
pub(crate) fn transform_and_render_source_map_detailed(
	state: &mut State<'_>,
	source_file: Option<P<SourceFile>>,
) -> Result<(Vec<u8>, String, Vec<DiagnosticInfo>), Failure> {
	let statements = transform_source_file(state);
	if let Some(value) = state.internal_error() {
		return Err(Failure::bare(Error::InternalCompiler(InternalCompilerError {
			file_name: state.source_file.file_name().to_owned(),
			value: value.to_owned(),
		})));
	}

	let has_errors = state.diags.has_errors();
	let diagnostics: Vec<DiagnosticInfo> = state.diags.flush().iter().map(info_from_node_diag).collect();
	if has_errors {
		// Upstream bails before rendering when the transformer reported errors
		// (compileFiles.ts:176-178).
		return Ok((Vec::new(), String::new(), diagnostics));
	}

	let rendered = match source_file {
		Some(source_file) => state.render_with_source_map(statements, source_file),
		None => render_ast_bytes(&state.ast, statements).map(|text| (text, String::new())),
	};
	match rendered {
		Ok((text, source_map)) => Ok((text, source_map, diagnostics)),
		Err(error) => Err(Failure::bare(Error::InternalCompiler(InternalCompilerError {
			file_name: state.source_file.file_name().to_owned(),
			value: error.message().to_owned(),
		}))),
	}
}

/// `preEmitProjectFileDiagnosticsWithOptions`: the per-file half of `ts.getPreEmitDiagnostics`,
/// syntactic first (upstream order), then semantic unless `skip_semantic_diagnostics`.
pub(crate) fn pre_emit_project_file_diagnostics_with_options(
	context: &Context,
	program: &'static Program,
	source_file: P<SourceFile>,
	opts: &ProjectOptions,
) -> Vec<P<ast::Diagnostic>> {
	let mut diagnostics = program.get_syntactic_diagnostics(context, Some(source_file));
	if opts.skip_semantic_diagnostics {
		// GetType already uses CheckModeTypeOnly on the nodes emit queries; a full file check
		// here would put the precheck walk back on the clock.
		return diagnostics;
	}
	diagnostics.extend(program.get_semantic_diagnostics(context, Some(source_file)));
	diagnostics
}

/// `diagnosticStrings`
#[must_use]
pub fn diagnostic_strings(diagnostics: &[P<ast::Diagnostic>]) -> Vec<String> {
	diagnostics.iter().map(|diagnostic| diagnostic.localize()).collect()
}

/// `diagnosticInfoMessages`
#[must_use]
pub fn diagnostic_info_messages(diagnostics: &[DiagnosticInfo]) -> Vec<String> {
	diagnostics
		.iter()
		.map(|diagnostic| diagnostic.message.clone())
		.collect()
}

/// `stringDiagnostics`
#[must_use]
pub fn string_diagnostics(diagnostics: Vec<String>) -> Vec<DiagnosticInfo> {
	diagnostics
		.into_iter()
		.map(|message| DiagnosticInfo {
			message,
			..DiagnosticInfo::default()
		})
		.collect()
}

/// `tsDiagnosticInfos`
#[must_use]
pub fn ts_diagnostic_infos(diagnostics: &[P<ast::Diagnostic>], traces: &DiagnosticTraces) -> Vec<DiagnosticInfo> {
	diagnostics
		.iter()
		.map(|&diagnostic| info_from_ts_diag(diagnostic, traces))
		.collect()
}

/// `infoFromNodeDiag`: a located [`DiagnosticInfo`] from a transformer diagnostic.
#[must_use]
pub fn info_from_node_diag(diagnostic: &Diagnostic) -> DiagnosticInfo {
	let mut info = DiagnosticInfo {
		code: diagnostic.code.to_owned(),
		message: diagnostic.message.clone(),
		warning: diagnostic.warning,
		..DiagnosticInfo::default()
	};
	if let Some(node) = diagnostic.node
		&& let Some(file) = ast::get_source_file_of_node(node)
		&& node.pos().source_offset().is_some()
	{
		let start = rotor_tsrs::scanner::get_token_pos_of_node(node, file, false) as usize;
		file.file_name().clone_into(&mut info.file_name);
		info.offset = start;
		(info.line, info.col) = line_col_in(file.text(), start);
		if let Some(end) = node.end().source_offset()
			&& end as usize > start
		{
			info.len = end as usize - start;
		}
	}
	info
}

/// `TypeScriptDiagnosticCode`: a TypeScript diagnostic number as `TS####`.
#[must_use]
pub fn type_script_diagnostic_code(code: i32) -> String {
	format!("TS{code}")
}

/// `DiagnosticCode`. tsrs has no rotor string codes (`StringCode`), so every code is numeric.
#[must_use]
pub fn diagnostic_code(diagnostic: &ast::Diagnostic) -> String {
	type_script_diagnostic_code(diagnostic.code())
}

/// `infoFromTSDiag`
#[must_use]
pub fn info_from_ts_diag(diagnostic: P<ast::Diagnostic>, traces: &DiagnosticTraces) -> DiagnosticInfo {
	let mut info = DiagnosticInfo {
		code: diagnostic_code(&diagnostic),
		message: diagnostic.localize(),
		warning: diagnostic.category() != Category::Error,
		..DiagnosticInfo::default()
	};
	if let Some(file) = diagnostic.file() {
		file.file_name().clone_into(&mut info.file_name);
		info.offset = diagnostic.pos().unwrap_or(0) as usize;
		info.len = diagnostic.len().unwrap_or(0) as usize;
		(info.line, info.col) = line_col_in(file.text(), info.offset);
		info = traces.remap(info, file.text());
	}
	info.related = diagnostic
		.related_information()
		.iter()
		.map(|related| {
			let mut entry = RelatedInfo {
				message: related.localize(),
				..RelatedInfo::default()
			};
			if let Some(file) = related.file() {
				file.file_name().clone_into(&mut entry.file_name);
				entry.offset = related.pos().unwrap_or(0) as usize;
				entry.len = related.len().unwrap_or(0) as usize;
			}
			entry
		})
		.collect();
	info
}

/// `lineColIn`: the 1-based line and byte column of `offset` in `source`, the text the compile
/// used; `(0, 0)` for an offset outside it.
#[must_use]
pub fn line_col_in(source: &str, offset: usize) -> (usize, usize) {
	if offset > source.len() {
		return (0, 0);
	}
	let (mut line, mut col) = (1, 1);
	for &byte in &source.as_bytes()[..offset] {
		if byte == b'\n' {
			line += 1;
			col = 1;
		} else {
			col += 1;
		}
	}
	(line, col)
}

/// `commentDirectiveDiagnostics`: one `noCommentDirectives` message per `@ts-ignore` /
/// `@ts-expect-error` directive, plus one for `@ts-nocheck`.
#[must_use]
pub fn comment_directive_diagnostics(source_file: P<SourceFile>) -> Vec<String> {
	let mut count = source_file.comment_directives.get().len();
	if ast::get_pragma_from_source_file(Some(source_file), "ts-nocheck").is_some() {
		count += 1;
	}
	if count == 0 {
		return Vec::new();
	}
	let message = rotor_transformer::diagnostics::no_comment_directives(source_file.as_node()).message;
	vec![message; count]
}
