//! Small helpers from slice-owned Go files that more than one Stage 4b slice calls. They are
//! ported here, in core, so that no slice has to edit another slice's directory. The owning slice
//! does not port them again.

use rotor_luau::TypeGeneric;
use rotor_tsrs::ast as tsast;
use rotor_tsrs::checker::LiteralValue;

use crate::control::VarArgsData;
use crate::dispatch::transform_expression;
use crate::prelude::*;

/// `isOmittedBindingElement` (`binding.go`): an array binding hole. tsgo parses `const [, x]` as
/// a binding element with no name rather than an `OmittedExpression`.
#[must_use]
pub fn is_omitted_binding_element(element: P<Node>) -> bool {
	tsast::is_omitted_expression(element) || (tsast::is_binding_element(element) && element.name().is_none())
}

/// `getExtendsNode` (`classes.go`, `util/getExtendsNode.ts`): the first `extends` heritage
/// element.
#[must_use]
pub fn get_extends_node(node: P<Node>) -> Option<P<Node>> {
	tsast::get_class_extends_heritage_element(node)
}

/// `findConstructor` (`classes.go`, `util/findConstructor.ts`): the first constructor with a
/// body (overload signatures have none).
#[must_use]
pub fn find_constructor(node: P<Node>) -> Option<P<Node>> {
	node.members()
		.iter()
		.copied()
		.find(|&member| tsast::is_constructor_declaration(member) && member.body().is_some())
}

/// `createClassNameFunction` (`classes.go`, `createNameFunction`):
/// `function() return "<name>" end`.
pub fn create_class_name_function(ast: &mut Ast, name: &str) -> NodeId {
	let parameters = ast.new_list::<element::AnyIdentifier>();
	let value = ast.str(name);
	let ret = ast.return_statement(value);
	let statements = ast.list_from::<element::Statement>([ret]);
	ast.function_expression(parameters, false, statements)
}

/// `isClassHoisted` (`classes.go`). Go panics on a class name with no symbol (the upstream
/// assert); the port records an internal error.
pub fn is_class_hoisted(state: &mut State<'_>, node: P<Node>) -> bool {
	let Some(name) = node.name() else {
		return false;
	};
	let Some(symbol) = state.ck().get_symbol_at_location(name) else {
		state.ice("transformer: class name has no symbol");
		return false;
	};
	state.is_hoisted.get(&symbol).copied().unwrap_or(false)
}

/// `needsInverseEntry` (`enums.go`): every member whose constant value is not a string,
/// including computed members the evaluator could not fold.
pub fn needs_inverse_entry(state: &mut State<'_>, member: P<Node>) -> bool {
	!matches!(state.ck().get_constant_value(member), Some(LiteralValue::String(_)))
}

/// `transformInitializer` (`parameters.go`): `if id == nil then <prereqs> id = <init> end`.
pub fn transform_initializer(state: &mut State<'_>, id: NodeId, initializer: P<Node>) -> NodeId {
	let statements = state.capture_statements(|state| {
		let value = transform_expression(state, initializer);
		let assignment = state.ast.assignment(id, AssignmentOperator::Assign, value);
		state.prereq(assignment);
	});
	let nil = state.ast.nil();
	let condition = state.ast.binary(id, BinaryOperator::Equal, nil);
	state.ast.if_statement(condition, statements, None)
}

/// `thisParameter` (`luautypes_emit.go`): a function's written `this` parameter.
#[must_use]
pub fn this_parameter(node: P<Node>) -> Option<P<Node>> {
	let &first = node.parameters().first()?;
	first
		.name()
		.filter(|&name| tsast::is_this_identifier(name))
		.map(|_| first)
}

/// `typeGenericNames` (`luautypes_emit.go`): the names a generic list puts in scope.
#[must_use]
pub fn type_generic_names(generics: &[TypeGeneric]) -> Vec<String> {
	generics.iter().map(|generic| generic.name.clone()).collect()
}

/// `registerOptimizableVarArgsForFunction` (`varargsoptimization.go`): registers `data` on the
/// rest parameter and returns it.
pub fn register_optimizable_var_args_for_function(
	state: &mut State<'_>,
	node: P<Node>,
	data: Option<&VarArgsData>,
) -> Option<P<Node>> {
	let data = data.filter(|data| data.is_optimizable)?;
	let rest = node
		.parameters()
		.iter()
		.copied()
		.find(|parameter| parameter.as_parameter_declaration().dot_dot_dot_token().is_some())?;
	state.register_optimizable_var_args(rest, data.clone());
	Some(rest)
}

impl State<'_> {
	/// `luauTypedSignatures` (`luautypes_emit.go`): whether the `luauTypes` builder spells
	/// function signatures. A `//!native` file keeps the native policy for its signatures.
	#[must_use]
	pub const fn luau_typed_signatures(&self) -> bool {
		self.is_luau_typed && !self.is_native
	}
}
