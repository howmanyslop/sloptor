//! Port of `internal/luau/guards_test.go`.

use crate::{Ast, BinaryOperator, TypeAccess, TypeFunctionDeclaration, element};

#[test]
fn range_guards() {
	let mut a = Ast::new();
	let x = a.id("x");
	let brk = a.break_statement();
	let (key, nil) = (a.str("k"), a.nil());
	let field = a.map_field(key, nil);
	assert!(
		a.kind(x).is_expression() && a.kind(x).is_indexable_expression(),
		"identifier guards"
	);
	assert!(
		!a.kind(x).is_statement() && !a.kind(brk).is_expression(),
		"category confusion"
	);
	assert!(
		a.kind(brk).is_statement() && a.kind(field).is_field(),
		"statement/field guards"
	);
}

#[test]
#[expect(clippy::many_single_char_names, reason = "mirrors the Go test's names")]
fn composite_guards() {
	let mut a = Ast::new();
	let (s, temp) = (a.str("s"), a.temp_id(""));
	let empty = a.new_list::<element::Expression>();
	let array = a.array(empty);
	assert!(
		a.kind(s).is_simple() && a.kind(temp).is_simple() && !a.kind(array).is_simple(),
		"is_simple"
	);
	let (nil, x) = (a.nil(), a.id("x"));
	assert!(
		a.kind(nil).is_simple_primitive() && !a.kind(x).is_simple_primitive(),
		"is_simple_primitive"
	);
	let empty = a.new_list::<element::Expression>();
	let set = a.set(empty);
	assert!(a.kind(set).is_table() && !a.kind(s).is_table(), "is_table");
	let (brk, cont) = (a.break_statement(), a.continue_statement());
	let body = a.new_list::<element::Statement>();
	let block = a.do_statement(body);
	assert!(
		a.kind(brk).is_final_statement() && a.kind(cont).is_final_statement() && !a.kind(block).is_final_statement(),
		"is_final_statement"
	);
	let (f, args) = (a.id("f"), a.new_list::<element::Expression>());
	let call = a.call(f, args);
	let (o, args) = (a.id("o"), a.new_list::<element::Expression>());
	let method = a.method_call("m", o, args);
	assert!(a.kind(call).is_call() && a.kind(method).is_call(), "is_call");
	let (params, body) = (
		a.new_list::<element::AnyIdentifier>(),
		a.new_list::<element::Statement>(),
	);
	let function = a.function_expression(params, false, body);
	assert!(a.kind(function).is_function_like(), "is_function_like");
	assert!(
		a.kind(block).has_statements() && !a.kind(brk).has_statements(),
		"has_statements"
	);
	let (left, right) = (a.id("a"), a.id("b"));
	let sum = a.binary(left, BinaryOperator::Add, right);
	let plain = a.id("a");
	assert!(
		a.kind(sum).is_expression_with_precedence() && !a.kind(plain).is_expression_with_precedence(),
		"is_expression_with_precedence"
	);
}

#[test]
fn type_guards() {
	let mut a = Ast::new();
	let (name, x) = (a.type_name("x"), a.id("x"));
	let value = a.type_name("x");
	let prop = a.type_table_prop(TypeAccess::None, "a", value);
	assert!(
		a.kind(name).is_type_node() && !a.kind(x).is_type_node() && !a.kind(prop).is_type_node(),
		"is_type_node"
	);
	let empty = a.new_list::<element::TypeNode>();
	let pack = a.type_pack(empty);
	let inner = a.type_name("x");
	let variadic = a.type_variadic(inner);
	let generic_pack = a.type_generic_pack("T");
	assert!(
		a.kind(pack).is_type_pack() && a.kind(variadic).is_type_pack() && a.kind(generic_pack).is_type_pack(),
		"is_type_pack must accept every pack shape"
	);
	let inner = a.type_name("x");
	let optional = a.type_optional(inner);
	assert!(
		!a.kind(name).is_type_pack() && !a.kind(optional).is_type_pack() && !a.kind(x).is_type_pack(),
		"is_type_pack must reject types and expressions"
	);
	let (operand, ty) = (a.id("x"), a.type_name("T"));
	let cast = a.cast(operand, ty);
	let kind = a.kind(cast);
	assert!(
		kind.is_expression()
			&& !kind.is_indexable_expression()
			&& !kind.is_statement()
			&& !kind.is_expression_with_precedence(),
		"CastExpression guards"
	);
	let (params, body) = (
		a.new_list::<element::AnyIdentifier>(),
		a.new_list::<element::Statement>(),
	);
	let declaration = a.type_function_declaration("F", params, false, body);
	let statements = a
		.get::<TypeFunctionDeclaration>(declaration)
		.map(|declaration| declaration.statements);
	let kind = a.kind(declaration);
	assert!(
		kind.is_statement()
			&& kind.is_function_like()
			&& kind.has_statements()
			&& a.statements_of(declaration) == statements,
		"TypeFunctionDeclaration guards"
	);
	let body = a.type_name("x");
	let alias = a.type_alias("T", Vec::new(), Some(body));
	let kind = a.kind(alias);
	assert!(
		kind.is_statement() && !kind.is_function_like() && !kind.has_statements() && a.statements_of(alias).is_none(),
		"TypeAlias guards"
	);
}
