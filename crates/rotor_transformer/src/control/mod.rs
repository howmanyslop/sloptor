//! Stage 4b slice (ii), statements and control flow: functions (`functions.go`,
//! `parameters.go`, `namedfunction.go`, `varargsoptimization.go`), `switch.go`, `forof.go` and
//! `arrayappendoptimization.go`.
//!
//! The functions are stubs with the signatures that core code and the other slices call.
//! [`VarArgsData`] is the Go shape. The array append plan types and `getArrayAppendTarget` are in
//! core (`crate::state`), as in Go's `state.go`. The slice port replaces the bodies and may add
//! submodules below this file. See `SLICES.md`.

use std::collections::HashMap;

use crate::pointer::MapPointer;
use crate::prelude::*;
use crate::state::ArrayAppendPlan;

/// Per-file state owned by this slice.
#[derive(Debug, Default)]
pub struct SliceState {}

/// Pass-wide state owned by this slice.
#[derive(Debug, Default)]
pub struct MultiSliceState {}

/// `VarArgsData` (`varargsoptimization.go`).
#[derive(Clone, Debug, Default)]
pub struct VarArgsData {
	/// Go's `-1` is `None`.
	pub(crate) parameter_index: Option<usize>,
	pub(crate) size_access_count: usize,
	pub(crate) for_of_access_count: usize,
	pub(crate) is_optimizable: bool,
	pub(crate) length_id: Option<NodeId>,
}

impl VarArgsData {
	/// `usesLengthCache`
	pub const fn uses_length_cache(&self) -> bool {
		self.is_optimizable && self.size_access_count + self.for_of_access_count > 1 && self.length_id.is_some()
	}
}

/// `createVarArgsLengthSelect`: `select("#", ...)`.
pub fn create_var_args_length_select(ast: &mut Ast) -> NodeId {
	let select = ast.global_id("select");
	let count = ast.str("#");
	let var_args = ast.var_args();
	let args = ast.list_from([count, var_args]);
	ast.call(select, args)
}

/// `analyzeArrayAppendStatements` (`arrayappendoptimization.go`): nothing unless the
/// `optimizedArrayAppends` option is on.
pub fn analyze_array_append_statements(
	state: &mut State<'_>,
	statements: &[P<Node>],
) -> HashMap<P<Node>, ArrayAppendPlan> {
	if state.optimized_array_appends
		&& let Some(&first) = statements.first()
	{
		state.not_yet_ported(first, "optimizedArrayAppends");
	}
	HashMap::new()
}

/// `transformFunctionExpression` (`functions.go`), for function expressions and arrows.
pub fn transform_function_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	state.not_yet_ported(node, "function expressions");
	state.ast.none()
}

/// `transformFunctionDeclaration` (`functions.go`).
pub fn transform_function_declaration(state: &mut State<'_>, node: P<Node>) -> Statements {
	state.not_yet_ported(node, "function declarations");
	state.ast.new_list()
}

/// `transformMethodDeclaration` (`functions.go`): an object literal method.
pub fn transform_method_declaration(state: &mut State<'_>, node: P<Node>, _pointer: &mut MapPointer) -> Statements {
	state.not_yet_ported(node, "object literal methods");
	state.ast.new_list()
}

/// `transformMatchingNamedFunctionConst` (`namedfunction.go`): `None` when the statement is not a
/// `const f = function f() {}` pattern. The stub never matches, and the function expression it
/// leaves reports itself.
pub const fn transform_matching_named_function_const(_state: &mut State<'_>, _node: P<Node>) -> Option<Statements> {
	None
}

/// `transformForOfStatement` (`forof.go`).
pub fn transform_for_of_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	state.not_yet_ported(node, "for-of statements");
	state.ast.new_list()
}

/// `transformSwitchStatement` (`switch.go`).
pub fn transform_switch_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	state.not_yet_ported(node, "switch statements");
	state.ast.new_list()
}

/// `transformParameters` (`parameters.go`)'s results.
#[derive(Debug)]
pub struct TransformedParameters {
	/// The Luau parameter list.
	pub parameters: List<element::AnyIdentifier>,
	/// The prologue statements (defaults, destructuring).
	pub statements: Statements,
	/// Whether the function takes `...`.
	pub has_dot_dot_dot: bool,
	/// The rest parameter's vararg data, when it has one.
	pub var_args_data: Option<VarArgsData>,
}

/// `transformParameters` (`parameters.go`), called by the class constructor port (slice iii).
pub fn transform_parameters(state: &mut State<'_>, node: P<Node>) -> TransformedParameters {
	state.not_yet_ported(node, "parameters");
	TransformedParameters {
		parameters: state.ast.new_list(),
		statements: state.ast.new_list(),
		has_dot_dot_dot: false,
		var_args_data: None,
	}
}

/// `transformMethodDeclarationWithReceiver` (`functions.go`), called by the class method port
/// (slice iii). `receiver` is the Luau `self` type, if any.
pub fn transform_method_declaration_with_receiver(
	state: &mut State<'_>,
	node: P<Node>,
	_pointer: &mut MapPointer,
	_receiver: Option<NodeId>,
) -> Statements {
	state.not_yet_ported(node, "methods");
	state.ast.new_list()
}
