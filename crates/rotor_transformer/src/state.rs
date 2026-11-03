//! `state.go`: the per-file [`State`] (upstream `TransformState.ts`) and the per-pass
//! [`MultiState`] (upstream `MultiTransformState.ts`).
//!
//! The state owns the file's Luau [`Ast`] arena, so every transform takes `&mut State` and
//! returns [`NodeId`]s into it. The checker and the pass-wide caches are borrowed for the life of
//! the state: one file is transformed at a time on a checker, as in Go.

use std::collections::{HashMap, HashSet};

use rotor_luau::{Origin, OriginId, Position};
use rotor_rojo::{PathTranslator, RbxPath, RojoResolver};
use rotor_tsrs::ast::{self as tsast, get_source_file_of_node};
use rotor_tsrs::checker::{Checker, TypeChecker};
use rotor_tsrs::compiler::Program;
use rotor_tsrs::types::TypeContext;
use rotor_tsrs::{scanner, tspath};

use crate::diagnostics;
use crate::diagservice::DiagService;
use crate::macromanager::MacroManager;
use crate::nodemodulespath::NodeModulesPathMapping;
use crate::prelude::*;
use crate::selectsymlinkpath::select_symlink_path;
use crate::sourcepositions::SourceColumnCheckpoint;
use crate::{control, luau_types, macros};

/// `ProjectType` (`Shared/constants.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectType {
	/// `game`
	Game,
	/// `model`
	Model,
	/// `package`
	Package,
}

impl ProjectType {
	/// The option's spelling.
	#[must_use]
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::Game => "game",
			Self::Model => "model",
			Self::Package => "package",
		}
	}
}

/// `MultiState`: caches that live for one whole compilation step, shared by every file of the
/// pass on one checker. Go keys these by pointer; the tsrs handles are the same identities.
///
/// Go makes one per checker group (`project.go`), because merged globals and so the
/// `MacroManager` registry are per checker. [`State::new`] records the first checker's id here
/// and reports an internal error for a state built on any other checker.
#[derive(Default)]
pub struct MultiState {
	/// `IsMethodCache`
	pub is_method_cache: HashMap<P<Type>, bool>,
	/// `IsReportedByNoUnstableThisType`
	pub is_reported_by_no_unstable_this_type: HashSet<P<Node>>,
	/// `IsDefinedAsLetCache`
	pub is_defined_as_let_cache: HashMap<P<Symbol>, bool>,
	/// `IsReportedByNoAnyCache`
	pub is_reported_by_no_any_cache: HashSet<P<Symbol>>,
	/// `IsReportedByMultipleDefinitionsCache`
	pub is_reported_by_multiple_definitions_cache: HashSet<P<Symbol>>,
	/// `GetModuleExportsCache`
	pub get_module_exports_cache: HashMap<P<Symbol>, Vec<P<Symbol>>>,
	/// `GetModuleExportsAliasMapCache`
	pub get_module_exports_alias_map_cache: HashMap<P<Symbol>, HashMap<P<Symbol>, String>>,
	/// The pass's one `MacroManager` (upstream `services.macroManager`), built by the first
	/// [`State::new`].
	pub(crate) macro_manager: Option<MacroManager>,
	/// The id of the checker every state of this pass must use.
	pub(crate) checker_id: Option<u32>,
}

impl MultiState {
	/// An empty compilation-step cache (`NewMultiState`).
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	/// The registration failures of the pass's `MacroManager` (`MacroManager.Missing`), sorted;
	/// empty before the first [`State::new`].
	#[must_use]
	pub fn macro_registration_failures(&self) -> Vec<String> {
		self.macro_manager.as_ref().map_or_else(Vec::new, MacroManager::missing)
	}
}

/// Per-file state of the transformer modules that keep their own.
#[derive(Default)]
pub struct Slices {
	pub(crate) luau_types: luau_types::SliceState,
}

/// `RojoContext`: the project-level Rojo data upstream's `TransformState` receives as
/// constructor arguments, computed once per pass and shared by every file.
#[derive(Clone, Debug)]
pub struct RojoContext {
	/// The resolver over the project's Rojo config.
	pub resolver: RojoResolver,
	/// The input to output path translator.
	pub path_translator: PathTranslator,
	/// The runtime library's Roblox path; `None` for Package projects (upstream `undefined`).
	pub runtime_lib_rbx_path: Option<RbxPath>,
	/// Upstream `data.projectPath`, the tsconfig directory.
	pub project_path: String,
	/// One package import resolver per type root.
	pub pkg_rojo_resolvers: Vec<RojoResolver>,
	/// Types entry to shipped main, per type root package.
	pub node_modules_path_mapping: NodeModulesPathMapping,
	/// Referenced sources and emitted declarations to their Luau import paths, keyed by
	/// `rojo.CanonicalFileName`.
	pub project_reference_paths: HashMap<String, String>,
	/// Upstream `data.nodeModulesPath`: `<package.json dir>/node_modules`.
	pub node_modules_path: String,
	/// `compilerOptions.typeRoots`, absolute and slash-separated.
	pub type_roots: Vec<String>,
	/// Feeds the canonical file name lookups.
	pub use_case_sensitive_file_names: bool,
}

/// `arrayAppendTarget` (`arrayappendoptimization.go`), read by core and by the
/// macros, so it lives here.
#[derive(Clone, Copy, Debug)]
pub struct ArrayAppendTarget {
	pub(crate) length_id: NodeId,
}

/// `arrayAppendPlan` (`arrayappendoptimization.go`).
#[derive(Clone, Debug)]
pub struct ArrayAppendPlan {
	pub(crate) calls: HashMap<P<Node>, ArrayAppendTarget>,
	pub(crate) target: ArrayAppendTarget,
}

/// `TryUses` (`types.ts`): which flow-control kinds escape a try block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TryUses {
	/// A `return` escapes.
	pub uses_return: bool,
	/// A `break` escapes.
	pub uses_break: bool,
	/// A `continue` escapes.
	pub uses_continue: bool,
}

/// The label flag values: the `LoopLabel` enum of roblox-ts PR #2928.
pub const LABEL_NONE: &str = "none";
pub const LABEL_BREAK: &str = "break";
pub const LABEL_CONTINUE: &str = "continue";

/// `labelEntry`: one live TS label. `depth` is the break depth of the construct the label is
/// attached to.
#[derive(Clone, Debug)]
pub struct LabelEntry {
	pub(crate) id: NodeId,
	pub(crate) name: String,
	pub(crate) depth: usize,
	pub(crate) is_loop: bool,
	pub(crate) ever_broken: bool,
	pub(crate) ever_continued: bool,
}

impl LabelEntry {
	/// Whether the flag variable is read anywhere, so it needs declaring.
	pub(crate) const fn used(&self) -> bool {
		self.ever_broken || self.ever_continued
	}
}

#[expect(clippy::struct_excessive_bools, reason = "Go's State option fields")]
/// What a project hands every file's state (the `State` fields Go's compile package assigns
/// after `NewState`).
#[derive(Clone, Copy, Default)]
pub struct StateOptions<'a> {
	/// `SetRojoContext`'s context.
	pub rojo: Option<&'a RojoContext>,
	/// `SetRojoContext`'s project type.
	pub project_type: Option<ProjectType>,
	/// `Env`, read by the `$env` macro.
	pub env: Option<&'a rotor_config::dotenv::Env>,
	/// `Files`, read by the `$file` macro.
	pub files: Option<&'a dyn macros::FileResolver>,
	/// `LogTruthyChanges`
	pub log_truthy_changes: bool,
	/// `OptimizedArrayAppends`
	pub optimized_array_appends: bool,
	/// `NativeTypes`
	pub native_types: bool,
	/// `LuauTypes`
	pub luau_types: bool,
	/// `!OptimizedLoops` (Go defaults the option on).
	pub no_optimized_loops: bool,
	/// `SkipSemanticDiagnostics`
	pub skip_semantic_diagnostics: bool,
}

#[expect(clippy::struct_excessive_bools, reason = "Go's State fields")]
/// `State` (`TransformState.ts`): the per-file transform state. It owns the prereq statement
/// stack, hoisting bookkeeping, runtime library use, the type cache and the module export ids.
pub struct State<'a> {
	/// The program.
	pub program: &'static Program,
	/// The checker that checked this file.
	pub checker: &'a mut Checker,
	/// The file being transformed.
	pub source_file: P<SourceFile>,
	/// The file's diagnostics.
	pub diags: DiagService,
	/// The pass-wide caches.
	pub multi: &'a mut MultiState,
	/// The file's Luau arena.
	pub ast: Ast,

	/// The project-level Rojo context.
	pub rojo: Option<&'a RojoContext>,
	/// The project type; `None` outside a project (Go's zero value).
	pub project_type: Option<ProjectType>,
	/// Whether the file's output lands under `ReplicatedFirst`.
	pub is_in_replicated_first: bool,
	/// The full source text (upstream `sourceFile.getFullText()`).
	pub source_file_text: &'static str,
	/// Set only by [`State::runtime_lib`]; gates the runtime library import.
	pub uses_runtime_lib: bool,
	/// The `logTruthyChanges` option.
	pub log_truthy_changes: bool,
	/// The `$env` macro's environment.
	pub env: Option<&'a rotor_config::dotenv::Env>,
	/// The `$file` macro's reader.
	pub files: Option<&'a dyn macros::FileResolver>,
	/// The `optimizedLoops` option (default on).
	pub optimized_loops: bool,
	/// The `optimizedArrayAppends` option (default off).
	pub optimized_array_appends: bool,
	/// The `nativeTypes` option.
	pub native_types: bool,
	/// The `luauTypes` option.
	pub luau_types: bool,
	/// The file has `//!native` and `nativeTypes` is on.
	pub is_native: bool,
	/// The `luauTypes` option is on.
	pub is_luau_typed: bool,
	/// Honors `[flamework] noSemanticDiagnostics`.
	pub skip_semantic_diagnostics: bool,
	/// `export =` was seen.
	pub has_export_equals: bool,
	/// `export ... from` was seen.
	pub has_export_from: bool,

	prereq_stack: Vec<Statements>,
	try_uses_stack: Vec<TryUses>,
	label_stack: Vec<LabelEntry>,
	break_depth: usize,
	pub(crate) array_append_plans: Vec<ArrayAppendPlan>,
	get_type_cache: HashMap<P<Node>, P<Type>>,

	/// `HoistsByStatement`: the TS identifiers needing a `local x` before each statement.
	pub hoists_by_statement: HashMap<P<Node>, Vec<P<Node>>>,
	/// `IsHoisted`: per-symbol hoist decisions, never reconsidered once made.
	pub is_hoisted: HashMap<P<Symbol>, bool>,
	/// `SymbolToID`: a symbol's replacement temporary.
	pub symbol_to_id: HashMap<P<Symbol>, NodeId>,
	/// Gates expression origins and occurrence-specific clones.
	pub source_map_enabled: bool,
	pub(crate) source_map_column_indexes: HashMap<P<SourceFile>, Vec<SourceColumnCheckpoint>>,
	/// `ClassIdentifierMap`: a class-like declaration's internal identifier.
	pub class_identifier_map: HashMap<P<Node>, NodeId>,
	class_element_to_object_key_map: HashMap<P<Node>, NodeId>,
	/// The decorated object keys, as a dense table over the arena.
	source_map_object_keys: Vec<bool>,
	module_id_by_symbol: HashMap<P<Symbol>, NodeId>,
	declaration_to_optimizable_var_args: HashMap<P<Node>, control::VarArgsData>,

	/// The first internal invariant violation (a Go `panic`), surfaced by the driver as an
	/// internal compiler error.
	pub(crate) internal_error: Option<String>,
	/// Per-file state of the `luau_types` module.
	pub(crate) slices: Slices,
}

impl<'a> State<'a> {
	/// `NewState`, then the field assignments Go's compile package makes before transforming.
	pub fn new(
		program: &'static Program,
		checker: &'a mut Checker,
		source_file: P<SourceFile>,
		multi: &'a mut MultiState,
		options: StateOptions<'a>,
	) -> Self {
		// One MacroManager per compilation pass and checker; the first state builds it.
		let checker_id = checker.id;
		let foreign_checker = multi.checker_id.is_some_and(|id| id != checker_id);
		multi.checker_id.get_or_insert(checker_id);
		if multi.macro_manager.is_none() {
			multi.macro_manager = Some(MacroManager::new(checker));
		}
		let source_map_enabled = program.options().source_map.is_true();
		let mut state = Self {
			program,
			checker,
			source_file,
			diags: DiagService::new(),
			multi,
			ast: Ast::new(),
			rojo: None,
			project_type: None,
			is_in_replicated_first: false,
			source_file_text: source_file.text(),
			uses_runtime_lib: false,
			log_truthy_changes: options.log_truthy_changes,
			env: options.env,
			files: options.files,
			optimized_loops: !options.no_optimized_loops,
			optimized_array_appends: options.optimized_array_appends,
			native_types: options.native_types,
			luau_types: options.luau_types,
			is_native: false,
			is_luau_typed: false,
			skip_semantic_diagnostics: options.skip_semantic_diagnostics,
			has_export_equals: false,
			has_export_from: false,
			prereq_stack: Vec::new(),
			try_uses_stack: Vec::new(),
			label_stack: Vec::new(),
			break_depth: 0,
			array_append_plans: Vec::new(),
			get_type_cache: HashMap::new(),
			hoists_by_statement: HashMap::new(),
			is_hoisted: HashMap::new(),
			symbol_to_id: HashMap::new(),
			source_map_enabled,
			source_map_column_indexes: HashMap::new(),
			class_identifier_map: HashMap::new(),
			class_element_to_object_key_map: HashMap::new(),
			source_map_object_keys: Vec::new(),
			module_id_by_symbol: HashMap::new(),
			declaration_to_optimizable_var_args: HashMap::new(),
			internal_error: None,
			slices: Slices::default(),
		};
		if foreign_checker {
			state.ice("transformer: MultiState is shared across checkers; make one per checker group");
		}
		if let Some(rojo) = options.rojo {
			state.set_rojo_context(rojo, options.project_type);
		} else {
			state.project_type = options.project_type;
		}
		state
	}

	/// The checker under rotor's Go-named API.
	pub const fn ck(&mut self) -> TypeChecker<'_> {
		TypeChecker::new(self.checker)
	}

	/// Records an internal invariant violation: Go panics here, and the compile boundary turns
	/// the panic into an internal compiler error. Transforms keep going with a placeholder node;
	/// the driver discards the output.
	pub(crate) fn ice(&mut self, message: impl Into<String>) {
		if self.internal_error.is_none() {
			self.internal_error = Some(message.into());
		}
	}

	/// The first internal invariant violation, if any.
	#[must_use]
	pub fn internal_error(&self) -> Option<&str> {
		self.internal_error.as_deref()
	}

	// -----------------------------------------------------------------------------------------
	// Source positions
	// -----------------------------------------------------------------------------------------

	pub(crate) fn original_source_position(&mut self, node: Option<P<Node>>, use_end: bool) -> Option<Position> {
		let node = node?;
		let source_file = get_source_file_of_node(node).unwrap_or(self.source_file);
		let mut position = i64::from(scanner::get_token_pos_of_node(node, source_file, false));
		if use_end {
			let pos = crate::util::node_pos(node);
			position = crate::util::node_end(node) - 1;
			if position < pos {
				position = pos;
			}
		}
		Some(self.source_position_at(source_file, position))
	}

	pub(crate) fn set_source_position(&mut self, node: NodeId, position: Option<Position>) {
		if let Some(start) = position {
			let origin = self.ast.add_origin(Origin {
				start,
				..Origin::default()
			});
			self.ast.set_origin(node, origin);
		}
	}

	/// Go mutates the shared origin in place; the arena merges a new origin instead, which only
	/// differs for clones made before this call (sourcemap output only).
	pub(crate) fn set_source_end_position(&mut self, node: NodeId, position: Option<Position>) {
		let Some(origin) = self.ast.origin(node) else {
			return;
		};
		if self.ast.origin_data(origin).closing.is_none()
			&& let Some(position) = position
		{
			let closing = self.ast.add_origin(Origin {
				closing: Some(position),
				start_unset: true,
				..Origin::default()
			});
			self.ast.set_origin(node, closing);
		}
	}

	pub(crate) fn source_origin(&mut self, source_node: P<Node>) -> Option<Origin> {
		let start = self.original_source_position(Some(source_node), false)?;
		let closing = self.original_source_position(Some(source_node), true);
		Some(Origin {
			start,
			closing,
			..Origin::default()
		})
	}

	pub(crate) fn set_node_source_position(&mut self, node: NodeId, source_node: P<Node>) {
		let needs = self.ast.origin(node).is_none_or(|origin| {
			let data = self.ast.origin_data(origin);
			data.start_unset || data.closing.is_none()
		});
		if needs && let Some(origin) = self.source_origin(source_node) {
			let origin = self.ast.add_origin(origin);
			self.ast.set_origin(node, origin);
		}
	}

	pub(crate) fn set_statement_list_source_positions(&mut self, source_node: P<Node>, lists: &[Statements]) {
		let mut origin: Option<OriginId> = None;
		for &statements in lists {
			for value in self.ast.list_to_vec(statements) {
				if let Some(existing) = self.ast.origin(value) {
					let data = self.ast.origin_data(existing);
					if !data.start_unset && data.closing.is_some() {
						continue;
					}
				}
				let origin_id = if let Some(origin_id) = origin {
					origin_id
				} else {
					let Some(source) = self.source_origin(source_node) else {
						return;
					};
					let origin_id = self.ast.add_origin(source);
					origin = Some(origin_id);
					origin_id
				};
				self.ast.set_origin(value, origin_id);
			}
		}
	}

	pub(crate) fn set_operator_source_position(&mut self, node: NodeId, token: P<Node>) {
		if self.source_map_enabled {
			let operator = self.original_source_position(Some(token), false);
			let origin = self.ast.add_origin(Origin {
				start_unset: true,
				operator,
				..Origin::default()
			});
			self.ast.set_origin(node, origin);
		}
	}

	pub(crate) fn set_update_operator_source_position(&mut self, node: NodeId, update: P<Node>) {
		if !self.source_map_enabled {
			return;
		}
		let start = self.original_source_position(Some(update), false);
		let position = if tsast::is_postfix_unary_expression(update) {
			// ++ and -- are the last two ASCII bytes, including when comments separate them from
			// their operand.
			let file = get_source_file_of_node(update).unwrap_or(self.source_file);
			Some(self.source_position_at(file, crate::util::node_end(update) - 2))
		} else {
			start
		};
		let origin = self.ast.add_origin(Origin {
			start_unset: true,
			operator: position,
			..Origin::default()
		});
		self.ast.set_origin(node, origin);
	}

	/// `SetRojoContext`: installs the project's Rojo context and type, then computes
	/// `IsInReplicatedFirst` as the upstream constructor does (`TransformState.ts:62-65`).
	pub fn set_rojo_context(&mut self, rojo: &'a RojoContext, project_type: Option<ProjectType>) {
		self.rojo = Some(rojo);
		self.project_type = project_type;
		let source_out_path = rojo.path_translator.get_output_path(self.source_file.file_name());
		let rbx_path = rojo.resolver.get_rbx_path_from_file_path(&source_out_path);
		self.is_in_replicated_first = rbx_path
			.as_ref()
			.and_then(|path| path.first())
			.is_some_and(|first| first == "ReplicatedFirst");
	}

	// -----------------------------------------------------------------------------------------
	// Prereq statement stack (upstream lines 99-178)
	// -----------------------------------------------------------------------------------------

	/// `Prereq`: appends a prerequisite statement to the top of the stack. Go panics on an empty
	/// stack (upstream's JS crash); every statement transform runs inside a capture.
	pub fn prereq(&mut self, statement: NodeId) {
		if let Some(&top) = self.prereq_stack.last() {
			self.ast.list_push(top, statement);
		} else {
			self.ice("transformer: Prereq on an empty prereq stack");
		}
	}

	/// `PrereqList`: splices `list` onto the top of the stack; `list` must not be reused.
	pub fn prereq_list(&mut self, list: Statements) {
		if let Some(&top) = self.prereq_stack.last() {
			self.ast.list_push_list(top, list);
		} else {
			self.ice("transformer: PrereqList on an empty prereq stack");
		}
	}

	/// `CaptureStatements`: runs `callback` with a fresh prereq list on the stack and returns
	/// what it produced. Restoring the entry depth is Go's recover-hygiene divergence; a no-op
	/// on the normal path.
	pub fn capture_statements(&mut self, callback: impl FnOnce(&mut Self)) -> Statements {
		let depth = self.prereq_stack.len();
		let list = self.ast.new_list();
		self.prereq_stack.push(list);
		callback(self);
		if self.prereq_stack.len() > depth + 1 {
			self.ice("transformer: prereq stack unbalanced");
		}
		self.prereq_stack.truncate(depth);
		list
	}

	/// `Capture`: the expression `callback` produces, with its prerequisite statements.
	pub fn capture(&mut self, callback: impl FnOnce(&mut Self) -> NodeId) -> (NodeId, Statements) {
		let mut value = None;
		let prereqs = self.capture_statements(|state| value = Some(callback(state)));
		let value = value.unwrap_or_else(|| self.ast.none());
		(value, prereqs)
	}

	/// `NoPrereqs`: runs `callback` and asserts it created no prerequisite statements.
	pub fn no_prereqs(&mut self, callback: impl FnOnce(&mut Self) -> NodeId) -> NodeId {
		let (expression, statements) = self.capture(callback);
		if self.ast.list_is_non_empty(statements) {
			self.ice("transformer: NoPrereqs callback created prerequisite statements");
		}
		expression
	}

	// -----------------------------------------------------------------------------------------
	// Try uses stack (upstream lines 68-97)
	// -----------------------------------------------------------------------------------------

	/// `PushTryUsesStack`: pushes a fresh, all-false entry. Go hands the caller the pointer;
	/// [`State::pop_try_uses_stack`] returns the entry's final flags instead.
	pub fn push_try_uses_stack(&mut self) {
		self.try_uses_stack.push(TryUses::default());
	}

	/// `PopTryUsesStack`, returning what the popped entry recorded.
	pub fn pop_try_uses_stack(&mut self) -> TryUses {
		self.try_uses_stack.pop().unwrap_or_else(|| {
			self.ice("transformer: PopTryUsesStack on empty stack");
			TryUses::default()
		})
	}

	/// `markTryUses`: a no-op with an empty stack, which makes flow control outside any try
	/// free.
	fn mark_try_uses(&mut self, mark: impl FnOnce(&mut TryUses)) {
		if let Some(top) = self.try_uses_stack.last_mut() {
			mark(top);
		}
	}

	/// `MarkTryUsesReturn`
	pub fn mark_try_uses_return(&mut self) {
		self.mark_try_uses(|uses| uses.uses_return = true);
	}

	/// `MarkTryUsesBreak`
	pub fn mark_try_uses_break(&mut self) {
		self.mark_try_uses(|uses| uses.uses_break = true);
	}

	/// `MarkTryUsesContinue`
	pub fn mark_try_uses_continue(&mut self) {
		self.mark_try_uses(|uses| uses.uses_continue = true);
	}

	// -----------------------------------------------------------------------------------------
	// Loop labels (a rotor extension)
	// -----------------------------------------------------------------------------------------

	/// `PushLabel`: opens a label scope for the construct about to be transformed and returns
	/// the entry's index.
	pub(crate) fn push_label(&mut self, name: &str, is_loop: bool) -> usize {
		let id = self.ast.temp_id(name);
		self.label_stack.push(LabelEntry {
			id,
			name: name.to_owned(),
			depth: self.break_depth + 1,
			is_loop,
			ever_broken: false,
			ever_continued: false,
		});
		self.label_stack.len() - 1
	}

	/// `PopLabel`: closes the innermost label scope and returns it.
	pub(crate) fn pop_label(&mut self) -> Option<LabelEntry> {
		let entry = self.label_stack.pop();
		if entry.is_none() {
			self.ice("transformer: PopLabel on empty stack");
		}
		entry
	}

	/// `FindLabel`: the innermost label named `name`.
	pub(crate) fn find_label(&self, name: &str) -> Option<usize> {
		self.label_stack.iter().rposition(|entry| entry.name == name)
	}

	/// The label entry at `index`.
	pub(crate) fn label_mut(&mut self, index: usize) -> &mut LabelEntry {
		&mut self.label_stack[index]
	}

	/// The label entry at `index`.
	pub(crate) fn label(&self, index: usize) -> &LabelEntry {
		&self.label_stack[index]
	}

	/// `EnterBreakScope`: every loop, switch and labeled non-loop statement pairs it with
	/// [`State::exit_break_scope`].
	pub const fn enter_break_scope(&mut self) {
		self.break_depth += 1;
	}

	/// `ExitBreakScope`
	pub fn exit_break_scope(&mut self) {
		if self.break_depth == 0 {
			self.ice("transformer: ExitBreakScope below zero");
			return;
		}
		self.break_depth -= 1;
	}

	/// `BreakDepth`
	#[must_use]
	pub const fn break_depth(&self) -> usize {
		self.break_depth
	}

	/// `LabelResets`: the `<id> = "none"` statements to unshift into the body of the loop being
	/// transformed. Only continued labels need one.
	pub fn label_resets(&mut self) -> Statements {
		let result = self.ast.new_list();
		let resets: Vec<NodeId> = self
			.label_stack
			.iter()
			.filter(|entry| entry.depth == self.break_depth && entry.ever_continued)
			.map(|entry| entry.id)
			.collect();
		for id in resets {
			let none = self.ast.str(LABEL_NONE);
			let assignment = self.ast.assignment(id, AssignmentOperator::Assign, none);
			self.ast.list_push(result, assignment);
		}
		result
	}

	/// `LabelChecks`: the statements to emit right after the break boundary at the current
	/// depth ends.
	///
	/// ```text
	/// depth-1, loop, continued -> `if _a == "continue" then continue end`
	/// anything else, still set -> `if _a == "break" then break end`
	/// ```
	pub fn label_checks(&mut self) -> Statements {
		let result = self.ast.new_list();
		let depth = self.break_depth;
		let mut continue_condition = None;
		let mut break_condition = None;
		let entries: Vec<LabelEntry> = self.label_stack.clone();
		for entry in entries {
			if entry.depth >= depth {
				continue;
			}
			let targets_enclosing = entry.depth + 1 == depth;
			if entry.ever_continued {
				let check = self.is_label_flag(entry.id, LABEL_CONTINUE);
				if targets_enclosing && entry.is_loop {
					continue_condition = Some(self.or_condition(continue_condition, check));
				} else {
					break_condition = Some(self.or_condition(break_condition, check));
				}
			}
			if entry.ever_broken {
				let check = self.is_label_flag(entry.id, LABEL_BREAK);
				break_condition = Some(self.or_condition(break_condition, check));
			}
		}
		if let Some(condition) = continue_condition {
			let continue_statement = self.ast.continue_statement();
			let body = self.ast.list_from([continue_statement]);
			let statement = self.ast.if_statement(condition, body, None);
			self.ast.list_push(result, statement);
		}
		if let Some(condition) = break_condition {
			let break_statement = self.ast.break_statement();
			let body = self.ast.list_from([break_statement]);
			let statement = self.ast.if_statement(condition, body, None);
			self.ast.list_push(result, statement);
		}
		result
	}

	fn or_condition(&mut self, into: Option<NodeId>, term: NodeId) -> NodeId {
		match into {
			Some(into) => self.ast.binary(into, BinaryOperator::Or, term),
			None => term,
		}
	}

	fn is_label_flag(&mut self, id: NodeId, value: &str) -> NodeId {
		let value = self.ast.str(value);
		self.ast.binary(id, BinaryOperator::Equal, value)
	}

	/// `WithoutLabels`: runs `callback` with an empty label stack and a zeroed break depth, the
	/// function barrier of label lowering.
	pub fn without_labels<R>(&mut self, callback: impl FnOnce(&mut Self) -> R) -> R {
		let saved_stack = std::mem::take(&mut self.label_stack);
		let saved_depth = std::mem::replace(&mut self.break_depth, 0);
		let result = callback(self);
		self.label_stack = saved_stack;
		self.break_depth = saved_depth;
		result
	}

	// -----------------------------------------------------------------------------------------
	// pushToVar family (upstream lines 267-306)
	// -----------------------------------------------------------------------------------------

	/// `PushToVar`: emits `local <temp> = <expression>` and returns the temp. `None` declares
	/// the temp without a value. The name is `name_hint` when non-empty, else derived from the
	/// expression (upstream `name || (expression && valueToIdStr(expression))`).
	pub fn push_to_var(&mut self, expression: Option<NodeId>, name_hint: &str) -> NodeId {
		let name = match expression {
			Some(expression) if name_hint.is_empty() => crate::valuetoidstr::value_to_id_str(&self.ast, expression),
			_ => name_hint.to_owned(),
		};
		let temp = self.ast.temp_id(name);
		let declaration = self.ast.variable_declaration(temp, expression.map(NodeOrList::from));
		self.prereq(declaration);
		temp
	}

	/// `PushToVarIfComplex`: simple expressions stay inline.
	pub fn push_to_var_if_complex(&mut self, expression: NodeId, name_hint: &str) -> NodeId {
		if self.ast.kind(expression).is_simple() {
			return expression;
		}
		self.push_to_var(Some(expression), name_hint)
	}

	/// `PushToVarIfNonID`: identifiers stay inline.
	pub fn push_to_var_if_non_id(&mut self, expression: NodeId, name_hint: &str) -> NodeId {
		if self.ast.kind(expression).is_any_identifier() {
			return expression;
		}
		self.push_to_var(Some(expression), name_hint)
	}

	// -----------------------------------------------------------------------------------------
	// Class element object keys (upstream lines 390-399)
	// -----------------------------------------------------------------------------------------

	/// `SetClassElementObjectKey`: records the pinned key of a decorated class element.
	pub fn set_class_element_object_key(&mut self, class_element: P<Node>, identifier: NodeId) {
		if self.class_element_to_object_key_map.contains_key(&class_element) {
			self.ice("transformer: SetClassElementObjectKey: key already set");
			return;
		}
		self.class_element_to_object_key_map.insert(class_element, identifier);
		if self.source_map_enabled {
			let index = identifier.index();
			if self.source_map_object_keys.len() <= index {
				self.source_map_object_keys.resize(index + 1, false);
			}
			self.source_map_object_keys[index] = true;
		}
	}

	/// `GetClassElementObjectKey`
	#[must_use]
	pub fn get_class_element_object_key(&self, class_element: P<Node>) -> Option<NodeId> {
		self.class_element_to_object_key_map.get(&class_element).copied()
	}

	pub(crate) fn is_source_map_object_key(&self, expression: NodeId) -> bool {
		self.source_map_object_keys
			.get(expression.index())
			.copied()
			.unwrap_or(false)
	}

	// -----------------------------------------------------------------------------------------
	// getType (upstream lines 183-186)
	// -----------------------------------------------------------------------------------------

	/// `GetType`: the type at [`rotor_tsrs::ast::skip_upwards`] of `node`, memoized by the
	/// original node.
	pub fn get_type(&mut self, node: P<Node>) -> P<Type> {
		if let Some(&cached) = self.get_type_cache.get(&node) {
			return cached;
		}
		let t = self.ck().get_type_at_location(tsast::skip_upwards(node));
		self.get_type_cache.insert(node, t);
		t
	}

	/// Overwrites a cached type, as Go's memoization test pokes `getTypeCache`.
	#[cfg(test)]
	pub(crate) fn poison_type_cache(&mut self, node: P<Node>, t: P<Type>) {
		self.get_type_cache.insert(node, t);
	}

	// -----------------------------------------------------------------------------------------
	// RuntimeLib (upstream TS(), lines 188-197)
	// -----------------------------------------------------------------------------------------

	/// `RuntimeLib` (upstream `TS(node, name)`): `TS.<name>`, flipping
	/// [`State::uses_runtime_lib`]. `node` only locates the `ReplicatedFirst` warning.
	pub fn runtime_lib(&mut self, node: Option<P<Node>>, name: &str) -> NodeId {
		self.uses_runtime_lib = true;
		if self.project_type == Some(ProjectType::Game) && self.is_in_replicated_first {
			// Emitted once per call, not deduped (upstream).
			self.diags.add(diagnostics::runtime_lib_used_in_replicated_first(node));
		}
		self.ast.global_property("TS", name)
	}

	// -----------------------------------------------------------------------------------------
	// guessVirtualPath (upstream lines 365-384)
	// -----------------------------------------------------------------------------------------

	/// `GuessVirtualPath`: maps a realpathed `node_modules` file back onto its symlink-side path
	/// through the symlink cache's realpath to symlink map, innermost ancestor first. `None`
	/// when no ancestor is known (upstream `undefined`).
	#[must_use]
	pub fn guess_virtual_path(&self, fs_path: &str) -> Option<String> {
		let by_realpath = self.program.get_symlink_cache();
		let by_realpath = by_realpath.directories_by_realpath();
		let original = fs_path;
		let mut fs_path = fs_path.to_owned();
		loop {
			let parent = tspath::ensure_trailing_directory_separator(&tspath::get_directory_path(&fs_path));
			if fs_path == parent {
				break;
			}
			fs_path = parent;
			let key = tspath::to_path(
				&fs_path,
				self.program.get_current_directory(),
				self.program.use_case_sensitive_file_names(),
			);
			if let Some(set) = by_realpath.load(&key) {
				let node_modules_path = self.rojo.map_or("", |rojo| rojo.node_modules_path.as_str());
				let symlink = select_symlink_path(&set.to_slice(), node_modules_path);
				if !symlink.is_empty() {
					let rest = original.strip_prefix(fs_path.as_str()).unwrap_or(original);
					return Some(tspath::combine_paths(&symlink, &[rest]));
				}
			}
		}
		None
	}

	// -----------------------------------------------------------------------------------------
	// Emit resolver (upstream state.resolver)
	// -----------------------------------------------------------------------------------------

	/// `EmitResolver().IsReferencedAliasDeclaration`: whether an import or export alias is
	/// referenced as a value. The marks live on the checker that checked the file, which is
	/// this state's checker.
	pub fn is_referenced_alias_declaration(&mut self, node: P<Node>) -> bool {
		self.ck().is_referenced_alias_declaration(node)
	}

	// -----------------------------------------------------------------------------------------
	// MacroManager access (upstream services.macroManager)
	// -----------------------------------------------------------------------------------------

	/// `getArrayAppendTarget`: the innermost array append plan's target for `call`.
	#[must_use]
	pub fn get_array_append_target(&self, call: P<Node>) -> Option<ArrayAppendTarget> {
		self.array_append_plans
			.iter()
			.rev()
			.find_map(|plan| plan.calls.get(&call).copied())
	}

	/// `Macros`: the pass-wide `MacroManager`.
	pub(crate) fn macros(&mut self) -> &mut MacroManager {
		self.multi
			.macro_manager
			.get_or_insert_with(|| MacroManager::new(self.checker))
	}

	/// `AmbientSymbol`: a global ambient symbol by name, through the `MacroManager` registry.
	pub fn ambient_symbol(&mut self, name: &str) -> Option<P<Symbol>> {
		let manager = self
			.multi
			.macro_manager
			.get_or_insert_with(|| MacroManager::new(self.checker));
		manager.symbol(self.checker, name)
	}

	/// `LuaTupleNominalSymbol`
	pub fn lua_tuple_nominal_symbol(&mut self) -> Option<P<Symbol>> {
		let manager = self
			.multi
			.macro_manager
			.get_or_insert_with(|| MacroManager::new(self.checker));
		manager.lua_tuple_nominal_symbol(self.checker)
	}

	// -----------------------------------------------------------------------------------------
	// Module ids (upstream lines 339-354)
	// -----------------------------------------------------------------------------------------

	/// `SetModuleIDBySymbol`
	pub fn set_module_id_by_symbol(&mut self, module_symbol: P<Symbol>, module_id: NodeId) {
		self.module_id_by_symbol.insert(module_symbol, module_id);
	}

	/// `GetModuleIDFromSymbol`: Go asserts the id was recorded.
	pub fn get_module_id_from_symbol(&mut self, module_symbol: P<Symbol>) -> NodeId {
		if let Some(&id) = self.module_id_by_symbol.get(&module_symbol) {
			return id;
		}
		self.ice("transformer: GetModuleIDFromSymbol: no module id recorded for symbol");
		self.ast.none()
	}

	/// `GetModuleIDFromNode`: the module id of the nearest module ancestor.
	pub fn get_module_id_from_node(&mut self, node: P<Node>) -> NodeId {
		match self.get_module_symbol_from_node(node) {
			Some(symbol) => self.get_module_id_from_symbol(symbol),
			None => self.ast.none(),
		}
	}

	pub(crate) fn register_optimizable_var_args(&mut self, declaration: P<Node>, data: control::VarArgsData) {
		self.declaration_to_optimizable_var_args.insert(declaration, data);
	}

	pub(crate) fn unregister_optimizable_var_args(&mut self, declaration: P<Node>) {
		self.declaration_to_optimizable_var_args.remove(&declaration);
	}

	/// `getOptimizableVarArgsData`: the vararg data of the parameter `node` names.
	pub(crate) fn get_optimizable_var_args_data(&mut self, node: P<Node>) -> Option<control::VarArgsData> {
		let expression = tsast::skip_downwards(node);
		if !crate::util::is_identifier(expression) {
			return None;
		}
		let symbol = self.ck().get_symbol_at_location(expression)?;
		let declaration = symbol.value_declaration()?;
		self.declaration_to_optimizable_var_args.get(&declaration).cloned()
	}

	/// `getModuleSymbolFromNode`: the export symbol of the nearest source file or module
	/// declaration ancestor.
	pub(crate) fn get_module_symbol_from_node(&mut self, node: P<Node>) -> Option<P<Symbol>> {
		let Some(module_ancestor) = tsast::find_ancestor(node, |candidate| {
			tsast::is_source_file(candidate) || tsast::is_module_declaration(candidate)
		}) else {
			self.ice("transformer: getModuleSymbolFromNode: no module ancestor");
			return None;
		};
		let location = if tsast::is_source_file(module_ancestor) {
			Some(module_ancestor)
		} else {
			module_ancestor.name()
		};
		let symbol = location.and_then(|location| self.ck().get_symbol_at_location(location));
		if symbol.is_none() {
			self.ice("transformer: getModuleSymbolFromNode: no module symbol");
		}
		symbol
	}

	/// `GetModuleIDPropertyAccess`: `exports.<name>` (or the namespace container's) when
	/// `id_symbol` is exported from its module.
	pub fn get_module_id_property_access(&mut self, id_symbol: P<Symbol>) -> Option<NodeId> {
		let value_declaration = id_symbol.value_declaration()?;
		let module_symbol = self.get_module_symbol_from_node(value_declaration)?;
		let alias = self
			.get_module_exports_alias_map(module_symbol)
			.get(&id_symbol)
			.cloned()?;
		let module_id = self.get_module_id_from_symbol(module_symbol);
		Some(self.ast.property_access(module_id, alias))
	}
}

impl TypeContext for State<'_> {
	fn checker(&mut self) -> &mut Checker {
		self.checker
	}

	fn ambient_symbol(&mut self, name: &str) -> Option<P<Symbol>> {
		Self::ambient_symbol(self, name)
	}

	fn lua_tuple_nominal_symbol(&mut self) -> Option<P<Symbol>> {
		Self::lua_tuple_nominal_symbol(self)
	}
}
