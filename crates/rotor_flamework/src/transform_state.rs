//! `transform_state.go`: the transform's input, per-invocation state and results.

use std::any::Any;
use std::collections::{HashMap, HashSet};

use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::{NodeFactory, SourceFile};
use rotor_tsrs::checker::{Checker, TypeChecker};
use rotor_tsrs::compiler::Program;
use rotor_tsrs::printer::{EmitContext, new_emit_context};

use crate::plan::{FileAnalysis, FilePlan};
use crate::project::Project;
use crate::transform::initialize_flamework_expression_surfaces;
use crate::transform_addpaths::AddPathsSourceIndex;
use crate::transform_diagnostics::{Diagnostic, diagnostic_file_name};
use crate::transform_expressions::default_flamework_macro_runtime;
use crate::transform_macro_types::MacroRuntime;
use crate::transform_parallel::TransformRecording;
use crate::{Error, ErrorKind};

/// What `TransformInput.Print` returns for a file.
pub type Printed = Box<dyn Any + Send>;

/// `TransformInput.Print`: runs for every file while its emit context is current. The parallel
/// loop calls it from its workers at once, as Go does, so it is `Sync`.
pub type PrintFn<'a> = Box<dyn Fn(&SourceMetadata) -> Result<Option<Printed>, Error> + Send + Sync + 'a>;

/// `TransformInput`: the native compiler boundary for Flamework.
pub struct TransformInput<'a> {
	/// The program.
	pub program: &'static Program,
	/// The checker.
	pub checker: &'a mut Checker,
	/// The files to transform, in caller order.
	pub files: Vec<P<SourceFile>>,
	/// The project.
	pub project: &'a mut Project,
	/// Analyses supplied by the caller.
	pub analyses: Vec<FileAnalysis>,
	/// A macro runtime overriding the default's parts.
	pub macro_runtime: Option<MacroRuntime>,
	/// Prints each file; its result is [`SourceMetadata::printed`].
	pub print: Option<PrintFn<'a>>,
}

/// `SourceMetadata`: the original source identity beside its replacement.
pub struct SourceMetadata {
	pub(crate) file_name: String,
	pub(crate) original: P<SourceFile>,
	pub(crate) transformed: P<SourceFile>,
	pub(crate) trace: SourceTrace,
	pub(crate) emit_context: P<EmitContext>,
	pub(crate) printed: Option<Printed>,
}

impl SourceMetadata {
	pub(crate) fn new(source_file: P<SourceFile>, transformed: P<SourceFile>, emit_context: P<EmitContext>) -> Self {
		Self {
			file_name: source_file.file_name().to_owned(),
			original: source_file,
			transformed,
			trace: SourceTrace {
				file_name: source_file.file_name().to_owned(),
				text: source_file.text(),
			},
			emit_context,
			printed: None,
		}
	}

	/// `FileName`
	#[must_use]
	pub fn file_name(&self) -> &str {
		&self.file_name
	}

	/// `Original`
	#[must_use]
	pub const fn original(&self) -> P<SourceFile> {
		self.original
	}

	/// `Transformed`
	#[must_use]
	pub const fn transformed(&self) -> P<SourceFile> {
		self.transformed
	}

	/// `Changed`
	#[must_use]
	pub fn changed(&self) -> bool {
		self.original != self.transformed
	}

	/// `Trace`
	#[must_use]
	pub const fn trace(&self) -> &SourceTrace {
		&self.trace
	}

	/// `EmitContext`
	#[must_use]
	pub const fn emit_context(&self) -> P<EmitContext> {
		self.emit_context
	}

	/// `Printed`: the print callback's result, if any.
	#[must_use]
	pub fn printed(&self) -> Option<&Printed> {
		self.printed.as_ref()
	}

	/// Takes the print callback's result.
	pub fn take_printed(&mut self) -> Option<Printed> {
		self.printed.take()
	}
}

/// `printSource`: runs the print callback for one file's metadata.
pub(crate) fn print_source(print: Option<&PrintFn<'_>>, mut metadata: SourceMetadata) -> Result<SourceMetadata, Error> {
	let Some(print) = print else {
		return Ok(metadata);
	};
	metadata.printed = print(&metadata)?;
	Ok(metadata)
}

/// `SourceTrace`: the original source identity later trace maps compose with.
#[derive(Clone, Debug)]
pub struct SourceTrace {
	file_name: String,
	text: &'static str,
}

impl SourceTrace {
	/// `OriginalFileName`
	#[must_use]
	pub fn original_file_name(&self) -> &str {
		&self.file_name
	}

	/// `OriginalText`
	#[must_use]
	pub const fn original_text(&self) -> &'static str {
		self.text
	}
}

/// `TransformResult`
pub struct TransformResult {
	/// The transformed files, in input order.
	pub files: Vec<P<SourceFile>>,
	/// The plans.
	pub plans: Vec<FilePlan>,
	/// The diagnostics, ordered.
	pub diagnostics: Vec<Diagnostic>,
	/// Each file's metadata.
	pub sources: Vec<SourceMetadata>,
}

/// `TransformState`
pub struct TransformState<'a> {
	pub(crate) program: &'static Program,
	pub(crate) checker: &'a mut Checker,
	pub(crate) project: &'a mut Project,
	pub(crate) factory: NodeFactory,
	pub(crate) emit_context: P<EmitContext>,
	pub(crate) macro_runtime: MacroRuntime,
	pub(crate) plans: Vec<FilePlan>,
	/// `plans` by normalized file ID, built on the first `plan_for_file`; reset whenever
	/// `plans` is replaced.
	pub(crate) plan_index: std::cell::OnceCell<HashMap<String, Vec<usize>>>,
	pub(crate) diagnostics: Vec<Diagnostic>,
	pub(crate) generated_names: HashMap<String, HashMap<String, usize>>,
	pub(crate) guard_library: String,
	pub(crate) surfaces: HashMap<P<SourceFile>, bool>,
	pub(crate) expression_surfaces: HashMap<P<SourceFile>, bool>,
	/// `flameworkExpressionSurfaces`: the files that reach a direct expression surface.
	pub(crate) flamework_expression_surfaces: Option<HashSet<String>>,
	pub(crate) add_paths_sources: Option<std::sync::Arc<AddPathsSourceIndex>>,
	/// A parallel worker's build-info writes (Go's `Project.recording`).
	pub(crate) recording: Option<TransformRecording>,
}

/// `newTransformState`
pub(crate) fn new_transform_state<'a>(
	program: &'static Program,
	checker: &'a mut Checker,
	project: &'a mut Project,
	macro_runtime: Option<&MacroRuntime>,
	plans: &[FilePlan],
) -> TransformState<'a> {
	let mut state = new_transform_state_base(program, checker, project, macro_runtime, plans);
	initialize_flamework_expression_surfaces(&mut state);
	state
}

/// `newTransformStateBase`: class discovery's state, without the reachability index.
pub(crate) fn new_transform_state_base<'a>(
	program: &'static Program,
	checker: &'a mut Checker,
	project: &'a mut Project,
	macro_runtime: Option<&MacroRuntime>,
	plans: &[FilePlan],
) -> TransformState<'a> {
	let mut runtime = default_flamework_macro_runtime();
	if let Some(custom) = macro_runtime {
		if custom.uuid.is_some() {
			runtime.uuid.clone_from(&custom.uuid);
		}
		if custom.random_index.is_some() {
			runtime.random_index.clone_from(&custom.random_index);
		}
		if custom.build_guard.is_some() {
			runtime.build_guard = custom.build_guard;
		}
	}
	let emit_context = new_emit_context();
	let factory = emit_context.factory.as_node_factory().clone();
	TransformState {
		program,
		checker,
		project,
		factory,
		emit_context,
		macro_runtime: runtime,
		plans: plans.to_vec(),
		plan_index: std::cell::OnceCell::new(),
		diagnostics: Vec::new(),
		generated_names: HashMap::new(),
		guard_library: String::new(),
		surfaces: HashMap::new(),
		expression_surfaces: HashMap::new(),
		flamework_expression_surfaces: None,
		add_paths_sources: None,
		recording: None,
	}
}

impl TransformState<'_> {
	/// The checker under rotor's Go-named API.
	pub(crate) const fn ck(&mut self) -> TypeChecker<'_> {
		TypeChecker::new(self.checker)
	}

	/// `nextGeneratedName`: `preferred`, then `preferred_1`, ... per file.
	pub(crate) fn next_generated_name(&mut self, file_name: &str, preferred: &str) -> String {
		let names = self.generated_names.entry(file_name.to_owned()).or_default();
		let suffix = names.entry(preferred.to_owned()).or_insert(0);
		let current = *suffix;
		*suffix += 1;
		if current == 0 {
			return preferred.to_owned();
		}
		format!("{preferred}_{current}")
	}

	pub(crate) fn flamework_surface(&self, source_file: P<SourceFile>) -> Option<bool> {
		self.surfaces.get(&source_file).copied()
	}

	pub(crate) fn set_flamework_surface(&mut self, source_file: P<SourceFile>, declares: bool) {
		self.surfaces.insert(source_file, declares);
	}

	pub(crate) fn flamework_expression_surface(&self, source_file: P<SourceFile>) -> Option<bool> {
		self.expression_surfaces.get(&source_file).copied()
	}

	pub(crate) fn set_flamework_expression_surface(&mut self, source_file: P<SourceFile>, declares: bool) {
		self.expression_surfaces.insert(source_file, declares);
	}

	/// `Factory`
	#[must_use]
	pub const fn factory(&self) -> &NodeFactory {
		&self.factory
	}

	/// `EmitContext`
	#[must_use]
	pub const fn emit_context(&self) -> P<EmitContext> {
		self.emit_context
	}

	/// `MacroRuntime`
	#[must_use]
	pub fn macro_runtime(&self) -> MacroRuntime {
		self.macro_runtime.clone()
	}

	/// `AddDiagnostic`
	pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
		self.diagnostics.push(diagnostic);
	}

	/// `planForFile`: the plan whose file ID names `source_file`.
	///
	/// Go compares the file's name with every plan's cleaned file ID: equal, or ending in `/`
	/// and the ID. The IDs are cleaned once into an index, and the name's suffixes after each
	/// `/` are looked up in it, so a project's files do not each clean every plan's ID.
	pub(crate) fn plan_for_file(&self, source_file: P<SourceFile>) -> Result<Option<FilePlan>, Error> {
		let file_name = gopath::to_slash(&gopath::clean(source_file.file_name())).into_owned();
		let index = self.plan_index.get_or_init(|| {
			let mut index: HashMap<String, Vec<usize>> = HashMap::with_capacity(self.plans.len());
			for (position, plan) in self.plans.iter().enumerate() {
				let file_id = gopath::to_slash(&gopath::clean(&gopath::from_slash(plan.file_id()))).into_owned();
				index.entry(file_id).or_default().push(position);
			}
			index
		});
		let mut last: Option<usize> = None;
		let mut match_count = 0;
		let suffixes = std::iter::once(file_name.as_str())
			.chain(file_name.match_indices('/').map(|(slash, _)| &file_name[slash + 1..]));
		for suffix in suffixes {
			if let Some(positions) = index.get(suffix) {
				match_count += positions.len();
				last = last.max(positions.last().copied());
			}
		}
		let matched = last.map(|position| &self.plans[position]);
		if match_count > 1 {
			return Err(Error::kinded(
				ErrorKind::AmbiguousFilePlan,
				&format!(": {}", source_file.file_name()),
			));
		}
		Ok(matched.cloned())
	}

	/// `orderedDiagnostics`: by file, position, code, then text.
	pub(crate) fn ordered_diagnostics(&self) -> Vec<Diagnostic> {
		let mut diagnostics = self.diagnostics.clone();
		diagnostics.sort_by(|left, right| {
			diagnostic_file_name(left)
				.cmp(&diagnostic_file_name(right))
				.then_with(|| left.pos().cmp(&right.pos()))
				.then_with(|| left.code().cmp(&right.code()))
				.then_with(|| left.string().cmp(right.string()))
		});
		diagnostics
	}
}
