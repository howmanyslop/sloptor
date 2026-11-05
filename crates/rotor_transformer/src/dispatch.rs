//! `dispatch.go`: the expression dispatch (`nodes/expressions/transformExpression.ts`) and the
//! statement dispatch (`nodes/statements/transformStatement.ts`).
//!
//! Every kind upstream handles routes to the module that ports its Go file (see `SLICES.md`); any
//! other kind reports `rotorNotYetSupported`, as in Go.

use rotor_luau::Origin;
use rotor_tsrs::ast as tsast;

use crate::prelude::*;
use crate::{
	access, binary, call, classes, conditional, control, delete, diagnostics, expressions, extras, identifier,
	literals, loops, luau_types, methodassignment, newexpression, statements, unary, voidexpr,
};

/// `TransformExpression`: dispatches on the node's kind. Banned kinds add their diagnostic and
/// return `None` (no emit); diagnostics never abort the walk.
pub fn transform_expression(state: &mut State<'_>, node: P<Node>) -> NodeId {
	let expression = transform_expression_kind(state, node);
	methodassignment::validate_method_expression(state, node);
	if state.source_map_enabled {
		return tag_expression_origin(state, expression, node);
	}
	expression
}

/// `tagExpressionOrigin`: anchors an expression to its source node; a shared node (an
/// identifier, an adopted node, or one with an origin) gets an occurrence-specific clone.
pub fn tag_expression_origin(state: &mut State<'_>, expression: NodeId, node: P<Node>) -> NodeId {
	if state.ast.kind(expression).is_none() {
		return expression;
	}
	if matches!(
		node.kind(),
		Kind::ParenthesizedExpression
			| Kind::AsExpression
			| Kind::SatisfiesExpression
			| Kind::NonNullExpression
			| Kind::TypeAssertionExpression
	) {
		return expression;
	}
	let Some(start) = state.original_source_position(Some(node), false) else {
		return expression;
	};
	let mut origin = Origin {
		start,
		..Origin::default()
	};
	let previous = state.ast.origin(expression);
	if let Some(previous) = previous {
		let previous = *state.ast.origin_data(previous);
		origin.closing = previous.closing;
		origin.operator = previous.operator;
	}
	// The other State caches hold identifiers, covered here; decorated object keys may also be
	// literals or compound expressions.
	let shared = state.ast.kind(expression).is_any_identifier()
		|| state.ast.parent(expression).is_some()
		|| previous.is_some()
		|| state.is_source_map_object_key(expression);
	let origin = state.ast.add_origin(origin);
	if shared {
		return state.ast.with_origin(expression, Some(origin));
	}
	state.ast.set_origin(expression, origin);
	expression
}

fn transform_expression_kind(state: &mut State<'_>, node: P<Node>) -> NodeId {
	match node.kind() {
		// banned expressions
		Kind::BigIntLiteral => banned(state, diagnostics::no_big_int(node)),
		Kind::NullKeyword => banned(state, diagnostics::no_null_literal(node)),
		Kind::PrivateIdentifier => banned(state, diagnostics::no_private_identifier(node)),
		Kind::RegularExpressionLiteral => banned(state, diagnostics::no_regex(node)),
		Kind::TypeOfExpression => banned(state, diagnostics::no_type_of_expression(node)),

		// skip transforms
		Kind::ImportKeyword => state.ast.none(),

		// regular transforms
		Kind::ArrayLiteralExpression => literals::transform_array_literal_expression(state, node),
		Kind::ArrowFunction | Kind::FunctionExpression => control::transform_function_expression(state, node),
		Kind::AwaitExpression => extras::transform_await_expression(state, node),
		Kind::YieldExpression => extras::transform_yield_expression(state, node),
		Kind::BinaryExpression => binary::transform_binary_expression(state, node),
		Kind::CallExpression => call::transform_call_expression(state, node),
		Kind::ClassExpression => classes::transform_class_expression(state, node),
		Kind::ConditionalExpression => conditional::transform_conditional_expression(state, node),
		Kind::DeleteExpression => delete::transform_delete_expression(state, node),
		Kind::ElementAccessExpression => access::transform_element_access_expression(state, node),
		Kind::PropertyAccessExpression => access::transform_property_access_expression(state, node),
		Kind::FalseKeyword => state.ast.bool(false),
		Kind::TrueKeyword => state.ast.bool(true),
		Kind::Identifier => identifier::transform_identifier(state, node),
		Kind::JsxElement => extras::transform_jsx_element(state, node),
		Kind::JsxExpression => extras::transform_jsx_expression(state, node),
		Kind::JsxFragment => extras::transform_jsx_fragment(state, node),
		Kind::JsxSelfClosingElement => extras::transform_jsx_self_closing_element(state, node),
		Kind::NewExpression => newexpression::transform_new_expression(state, node),
		Kind::NoSubstitutionTemplateLiteral => literals::transform_no_substitution_template_literal(state, node),
		Kind::NumericLiteral => literals::transform_numeric_literal(state, node),
		Kind::ObjectLiteralExpression => literals::transform_object_literal_expression(state, node),
		Kind::ParenthesizedExpression => literals::transform_parenthesized_expression(state, node),
		Kind::PostfixUnaryExpression => unary::transform_postfix_unary_expression(state, node),
		Kind::PrefixUnaryExpression => unary::transform_prefix_unary_expression(state, node),
		Kind::SpreadElement => expressions::transform_spread_element(state, node),
		Kind::StringLiteral => literals::transform_string_literal(state, node),
		Kind::TaggedTemplateExpression => literals::transform_tagged_template_expression(state, node),
		Kind::TemplateExpression => literals::transform_template_expression(state, node),
		Kind::ThisKeyword => classes::transform_this_expression(state, node),
		Kind::SuperKeyword => classes::transform_super_keyword(state),
		Kind::VoidExpression => voidexpr::transform_void_expression(state, node),

		// type-only wrappers: the inner expression
		Kind::AsExpression
		| Kind::ExpressionWithTypeArguments
		| Kind::NonNullExpression
		| Kind::SatisfiesExpression
		| Kind::TypeAssertionExpression => match node.expression() {
			Some(expression) => transform_expression(state, expression),
			None => state.ast.none(),
		},

		// kinds upstream supports but rotor has not ported, and unknown kinds, fail loudly
		kind => {
			state.diags.add(diagnostics::rotor_not_yet_supported(
				node,
				&diagnostics::kind_name(kind),
			));
			state.ast.none()
		}
	}
}

fn banned(state: &mut State<'_>, diagnostic: Diagnostic) -> NodeId {
	state.diags.add(diagnostic);
	state.ast.none()
}

/// `transformStatementDispatch` (`transformStatement.ts`).
pub fn transform_statement(state: &mut State<'_>, node: P<Node>) -> Statements {
	// `declare` means the identifier is defined elsewhere: emit nothing (L103-107).
	if tsast::can_have_modifiers(node)
		&& node
			.modifier_nodes()
			.iter()
			.any(|modifier| modifier.kind() == Kind::DeclareKeyword)
	{
		return state.ast.new_list();
	}

	match node.kind() {
		// No emit, except that a top-level interface or type alias carries its Luau `type`
		// statement in place: every declaration with luauTypes, the aliased ones in a `//!native`
		// file. With both options on the luauTypes rule owns the aliases.
		Kind::InterfaceDeclaration | Kind::TypeAliasDeclaration => {
			if state.is_luau_typed {
				return luau_types::luau_declaration_statements(state, node);
			}
			luau_types::native_declaration_alias(state, node)
		}
		Kind::EmptyStatement => state.ast.new_list(),

		// banned statements
		Kind::ForInStatement => {
			state.diags.add(diagnostics::no_for_in_statement(node));
			state.ast.new_list()
		}
		Kind::DebuggerStatement => {
			state.diags.add(diagnostics::no_debugger_statement(node));
			state.ast.new_list()
		}

		// regular transforms
		Kind::Block => statements::transform_block(state, node),
		Kind::BreakStatement => statements::transform_break_statement(state, node),
		Kind::ClassDeclaration => classes::transform_class_declaration(state, node),
		Kind::ContinueStatement => statements::transform_continue_statement(state, node),
		Kind::DoStatement => loops::transform_do_statement(state, node),
		Kind::EnumDeclaration => classes::transform_enum_declaration(state, node),
		Kind::ExportAssignment => extras::transform_export_assignment(state, node),
		Kind::ExportDeclaration => extras::transform_export_declaration(state, node),
		Kind::ForStatement => loops::transform_for_statement(state, node),
		Kind::ForOfStatement => control::transform_for_of_statement(state, node),
		Kind::FunctionDeclaration => control::transform_function_declaration(state, node),
		Kind::ImportDeclaration => extras::transform_import_declaration(state, node),
		Kind::ImportEqualsDeclaration => extras::transform_import_equals_declaration(state, node),
		Kind::IfStatement => statements::transform_if_statement(state, node),
		Kind::LabeledStatement => extras::transform_labeled_statement(state, node),
		Kind::ModuleDeclaration => classes::transform_module_declaration(state, node),
		Kind::ReturnStatement => statements::transform_return_statement(state, node),
		Kind::SwitchStatement => control::transform_switch_statement(state, node),
		Kind::ThrowStatement => statements::transform_throw_statement(state, node),
		Kind::TryStatement => extras::transform_try_statement(state, node),
		Kind::VariableStatement => statements::transform_variable_statement(state, node),
		Kind::ExpressionStatement => statements::transform_expression_statement(state, node),
		Kind::WhileStatement => loops::transform_while_statement(state, node),

		// statement kinds not yet ported fail loudly rather than emitting wrong output
		kind => {
			state.diags.add(diagnostics::rotor_not_yet_supported(
				node,
				&diagnostics::kind_name(kind),
			));
			state.ast.new_list()
		}
	}
}
