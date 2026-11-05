//! `transform_expressions.go`: the expression visitor, macro imports and the default runtime.
//!
//! Go's visitors close over mutable state, which tsrs's `NodeVisitor` (`'static` `Fn` callbacks)
//! cannot hold. [`map_children`] keeps Go's `VisitEachChild` contract instead: it lists the
//! children `visit_each_child` visits, maps them in that order with an ordinary closure, and
//! replays the results through `visit_each_child`, which rebuilds the node exactly as Go does.

use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use rotor_tsrs::P;
use rotor_tsrs::ast::{
	Kind, Node, NodeFactory, SourceFile, TokenFlags, get_source_file_of_node, is_decorator, is_expression,
	is_identifier, is_statement, new_node_visitor,
};

use crate::guard::build_flamework_guard_for_macro;
use crate::id::new_uuid_v4;
use crate::transform_access::transform_flamework_access_expression;
use crate::transform_addpaths::expand_add_paths;
use crate::transform_binary::transform_flamework_binary_expression;
use crate::transform_call::transform_flamework_call_expression;
use crate::transform_component::{expression_uses_attribute_setter, new_attribute_setter_import};
use crate::transform_delete::transform_flamework_delete_expression;
use crate::transform_diagnostics::capture_flamework_diagnostic;
use crate::transform_guard_library::flamework_guard_library;
use crate::transform_macro_types::{MacroImport, MacroRuntime, MacroTransformResult, alloc};
use crate::transform_new::transform_flamework_new_expression;
use crate::transform_state::TransformState;
use crate::transform_unary::transform_flamework_unary_expression;
use crate::{Error, ErrorKind};

/// `expressionTransformResult`
#[derive(Clone, Debug)]
pub(crate) struct ExpressionTransformResult {
	pub(crate) expression: P<Node>,
	pub(crate) prerequisites: Vec<P<Node>>,
	pub(crate) imports: Vec<MacroImport>,
}

impl ExpressionTransformResult {
	pub(crate) const fn empty(expression: P<Node>) -> Self {
		Self {
			expression,
			prerequisites: Vec::new(),
			imports: Vec::new(),
		}
	}
}

/// Go's `visitor.VisitEachChild(node)` with `visit` as the visitor's callback.
pub(crate) fn map_children(
	factory: &NodeFactory,
	node: P<Node>,
	mut visit: impl FnMut(P<Node>) -> Option<P<Node>>,
) -> P<Node> {
	let seen: Rc<RefCell<Vec<P<Node>>>> = Rc::new(RefCell::new(Vec::new()));
	let sink = Rc::clone(&seen);
	let mut recorder = new_node_visitor(
		Some(Rc::new(move |_: &mut rotor_tsrs::ast::NodeVisitor, child: P<Node>| {
			sink.borrow_mut().push(child);
			Some(child)
		})),
		Some(factory.clone()),
		rotor_tsrs::ast::NodeVisitorHooks::default(),
	);
	recorder.visit_each_child(Some(node));
	let children = std::mem::take(&mut *seen.borrow_mut());
	if children.is_empty() {
		return node;
	}
	let replacements: VecDeque<Option<P<Node>>> = children.iter().map(|child| visit(*child)).collect();
	if replacements
		.iter()
		.zip(&children)
		.all(|(replacement, child)| *replacement == Some(*child))
	{
		return node;
	}
	let queue = Rc::new(RefCell::new(replacements));
	let mut replayer = new_node_visitor(
		Some(Rc::new(move |_: &mut rotor_tsrs::ast::NodeVisitor, _: P<Node>| {
			queue.borrow_mut().pop_front().flatten()
		})),
		Some(factory.clone()),
		rotor_tsrs::ast::NodeVisitorHooks::default(),
	);
	replayer.visit_each_child(Some(node)).unwrap_or(node)
}

/// `transformFlameworkExpression`: the node transformed with the default runtime. Only tests
/// call it, as in Go.
#[cfg(test)]
pub(crate) fn transform_flamework_expression(state: &mut TransformState<'_>, node: P<Node>) -> Result<P<Node>, Error> {
	transform_flamework_expression_with_runtime(state, node, &default_flamework_macro_runtime())
		.map(|result| result.expression)
}

/// `transformFlameworkExpressionsInSourceFile`: the file transformed with the default runtime.
/// Only tests call it, as in Go.
#[cfg(test)]
pub(crate) fn transform_flamework_expressions_in_source_file(
	state: &mut TransformState<'_>,
	source_file: P<SourceFile>,
) -> Result<P<SourceFile>, Error> {
	transform_flamework_expressions_in_source_file_with_runtime(state, source_file, &default_flamework_macro_runtime())
}

/// `transformFlameworkExpressionWithRuntime`
pub(crate) fn transform_flamework_expression_with_runtime(
	state: &mut TransformState<'_>,
	node: P<Node>,
	runtime: &MacroRuntime,
) -> Result<ExpressionTransformResult, Error> {
	match node.kind() {
		Kind::CallExpression => transform_flamework_call_expression(state, node, runtime),
		Kind::NewExpression => transform_flamework_new_expression(state, node, runtime),
		Kind::PrefixUnaryExpression | Kind::PostfixUnaryExpression => {
			transform_flamework_unary_expression(state, node, runtime)
		}
		Kind::BinaryExpression => transform_flamework_binary_expression(state, node, runtime),
		Kind::ElementAccessExpression | Kind::PropertyAccessExpression => {
			transform_flamework_access_expression(state, node, runtime)
		}
		Kind::DeleteExpression => transform_flamework_delete_expression(state, node, runtime),
		_ => transform_flamework_expression_children_with_runtime(state, node, runtime),
	}
}

/// `transformFlameworkExpressionChildrenWithRuntime`: each child transformed, with their
/// prerequisites and imports in order. A child whose error becomes a diagnostic is kept.
pub(crate) fn transform_flamework_expression_children_with_runtime(
	state: &mut TransformState<'_>,
	node: P<Node>,
	runtime: &MacroRuntime,
) -> Result<ExpressionTransformResult, Error> {
	let mut failure: Option<Error> = None;
	let mut prerequisites = Vec::new();
	let mut imports = Vec::new();
	let factory = state.factory.clone();
	let transformed = map_children(&factory, node, |child| {
		if failure.is_some() {
			return Some(child);
		}
		match transform_flamework_expression_with_runtime(state, child, runtime) {
			Ok(result) => {
				prerequisites.extend(result.prerequisites);
				imports.extend(result.imports);
				Some(result.expression)
			}
			Err(error) => {
				if !capture_flamework_diagnostic(state, &error) {
					failure = Some(error);
				}
				Some(child)
			}
		}
	});
	if let Some(error) = failure {
		return Err(error);
	}
	Ok(ExpressionTransformResult {
		expression: transformed,
		prerequisites,
		imports,
	})
}

/// `transformFlameworkExpressionsInSourceFileWithRuntime`: every expression in the file,
/// macro prerequisites before their statements, and the imports the macros need.
pub(crate) fn transform_flamework_expressions_in_source_file_with_runtime(
	state: &mut TransformState<'_>,
	source_file: P<SourceFile>,
	runtime: &MacroRuntime,
) -> Result<P<SourceFile>, Error> {
	let source_file = expand_add_paths(state, source_file)?;
	let mut visitor = FileExpressionVisitor {
		failure: None,
		attribute_setter_required: false,
		imports: Vec::new(),
		prerequisite_stack: Vec::new(),
		runtime,
	};
	let transformed = visitor.visit(state, source_file.as_node());
	if let Some(error) = visitor.failure {
		return Err(error);
	}
	let transformed = transformed.as_source_file_p();
	if !visitor.attribute_setter_required && visitor.imports.is_empty() {
		return Ok(transformed);
	}
	let mut statements = macro_import_statements(&state.factory, &visitor.imports);
	if visitor.attribute_setter_required {
		statements.push(new_attribute_setter_import(&state.factory));
	}
	statements.extend_from_slice(transformed.statements().nodes());
	let updated = state.factory.update_source_file(
		transformed.as_node(),
		state.factory.new_node_list(statements),
		transformed.end_of_file_token,
	);
	Ok(updated.as_source_file_p())
}

struct FileExpressionVisitor<'r> {
	failure: Option<Error>,
	attribute_setter_required: bool,
	imports: Vec<MacroImport>,
	prerequisite_stack: Vec<Vec<P<Node>>>,
	runtime: &'r MacroRuntime,
}

impl FileExpressionVisitor<'_> {
	fn visit(&mut self, state: &mut TransformState<'_>, node: P<Node>) -> P<Node> {
		if self.failure.is_some() || is_decorator(node) {
			return node;
		}
		if is_statement(node) {
			self.prerequisite_stack.push(Vec::new());
			let factory = state.factory.clone();
			let transformed = map_children(&factory, node, |child| Some(self.visit(state, child)));
			let prerequisites = self.prerequisite_stack.pop().unwrap_or_default();
			if prerequisites.is_empty() {
				return transformed;
			}
			let mut list = prerequisites;
			list.push(transformed);
			return state.factory.new_syntax_list(rotor_tsrs::tscore::alloc_vec(list));
		}
		if is_expression(node) {
			let transformed = match transform_flamework_expression_with_runtime(state, node, self.runtime) {
				Ok(transformed) => transformed,
				Err(error) => {
					if !capture_flamework_diagnostic(state, &error) {
						self.failure = Some(error);
					}
					return node;
				}
			};
			if !transformed.prerequisites.is_empty() {
				let Some(top) = self.prerequisite_stack.last_mut() else {
					self.failure = Some(Error::kinded(
						ErrorKind::InvalidTransformInput,
						": macro prerequisites have no owning statement",
					));
					return node;
				};
				top.extend(transformed.prerequisites);
			}
			self.imports.extend(transformed.imports);
			if transformed.expression != node && expression_uses_attribute_setter(transformed.expression) {
				self.attribute_setter_required = true;
			}
			return transformed.expression;
		}
		let factory = state.factory.clone();
		map_children(&factory, node, |child| Some(self.visit(state, child)))
	}
}

/// `defaultFlameworkMacroRuntime`
pub(crate) fn default_flamework_macro_runtime() -> MacroRuntime {
	MacroRuntime {
		uuid: Some(Rc::new(new_uuid_v4)),
		random_index: Some(Rc::new(flamework_macro_random_index)),
		build_guard: Some(build_flamework_guard_for_macro),
	}
}

/// `flameworkMacroRandomIndex`: a uniform random index in `[0, upper_bound)`.
fn flamework_macro_random_index(upper_bound: usize) -> Result<usize, Error> {
	if upper_bound == 0 {
		return Err(Error::kinded(
			ErrorKind::InvalidMacroRandomIndex,
			&format!(": upper bound {upper_bound} must be positive"),
		));
	}
	let bound = u64::try_from(upper_bound).unwrap_or(u64::MAX);
	// Rejection sampling, as crypto/rand.Int does, so every index is equally likely.
	let zone = u64::MAX - (u64::MAX % bound);
	loop {
		let mut bytes = [0u8; 8];
		getrandom::fill(&mut bytes)
			.map_err(|error| Error::msg(format!("generate Flamework macro random index: {error}")))?;
		let value = u64::from_le_bytes(bytes);
		if value < zone {
			return Ok(usize::try_from(value % bound).unwrap_or(0));
		}
	}
}

/// `macroImportStatements`: one named import per distinct macro import, in order.
pub(crate) fn macro_import_statements(factory: &NodeFactory, imports: &[MacroImport]) -> Vec<P<Node>> {
	let mut seen = HashSet::with_capacity(imports.len());
	let mut statements = Vec::with_capacity(imports.len());
	for macro_import in imports {
		if !seen.insert(macro_import) {
			continue;
		}
		let name = factory.new_identifier(alloc(&macro_import.local));
		let property_name =
			(macro_import.export != macro_import.local).then(|| factory.new_identifier(alloc(&macro_import.export)));
		let specifier = factory.new_import_specifier(false, property_name, name);
		let named_imports = factory.new_named_imports(factory.new_node_list(vec![specifier]));
		let clause = factory.new_import_clause(Kind::Unknown, None, Some(named_imports));
		statements.push(factory.new_import_declaration(
			None,
			Some(clause),
			factory.new_string_literal(alloc(&macro_import.module), TokenFlags::None),
			None,
		));
	}
	statements
}

/// `macroTransformChanged`: whether the macro replaced the node, its callee or an argument.
pub(crate) fn macro_transform_changed(node: P<Node>, expression: Option<P<Node>>) -> bool {
	let Some(expression) = expression else {
		return true;
	};
	if expression.kind() != node.kind() || expression.expression() != node.expression() {
		return true;
	}
	let arguments = node.arguments();
	let transformed = expression.arguments();
	arguments.len() != transformed.len() || arguments.iter().zip(transformed).any(|(left, right)| left != right)
}

/// `macroTransformImports`: the macro's imports, plus the guard library when it built guards.
pub(crate) fn macro_transform_imports(
	state: &mut TransformState<'_>,
	node: P<Node>,
	result: &MacroTransformResult,
) -> Vec<MacroImport> {
	let mut imports = result.imports.clone();
	let mut guard_references = synthesized_guard_reference_count(result.expression);
	for prerequisite in &result.prerequisites {
		guard_references += synthesized_guard_reference_count(Some(*prerequisite));
	}
	if guard_references > 0
		&& let Some(file) = get_source_file_of_node(node)
	{
		let mut module = String::new();
		for _ in 0..guard_references {
			module = flamework_guard_library(state, file);
		}
		imports.push(MacroImport {
			module,
			export: "t".to_owned(),
			local: "t".to_owned(),
		});
	}
	imports
}

/// `synthesizedGuardReferenceCount`: the synthesized `t` identifiers under `root`.
fn synthesized_guard_reference_count(root: Option<P<Node>>) -> usize {
	let Some(root) = root else {
		return 0;
	};
	let own = usize::from(is_identifier(root) && root.text() == "t" && root.pos().is_synthetic());
	own + root
		.iter_children()
		.into_iter()
		.map(|child| synthesized_guard_reference_count(Some(child)))
		.sum::<usize>()
}
