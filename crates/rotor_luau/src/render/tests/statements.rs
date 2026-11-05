//! Port of `internal/luau/render/statements_test.go`.

#![expect(
	clippy::tuple_array_conversions,
	reason = "tests bind sibling nodes in one statement"
)]

use super::dsl::{bin, call, id, num, paren, property, string, tn};
use super::{exprs, ids, render_both, stmts};
use crate::render::render_ast;
use crate::{
	AssignmentOperator, Ast, BinaryOperator as B, FunctionDeclaration, FunctionExpression, MethodDeclaration, NodeId,
	TypeAccess, TypeGeneric, element,
};

fn call_statement(a: &mut Ast, callee: &'static str, args: Vec<super::dsl::N>) -> NodeId {
	let call = call(id(callee), args).build(a);
	a.call_statement(call)
}

#[test]
fn render_return_with_empty_expression_list() {
	// `return $tuple()` reaches the renderer with an EMPTY expression list;
	// upstream's unconditional `return ${join(", ")}` renders `return ` with a
	// trailing space.
	let mut a = Ast::new();
	let empty = a.new_list::<element::Expression>();
	let ret = a.return_statement(empty);
	let ast = stmts(&mut a, &[ret]);
	assert_eq!(render_both(&a, ast), "return \n");
}

#[test]
fn render_variable_declaration_and_assignment() {
	let mut a = Ast::new();
	let (x, five) = (a.id("x"), a.num(5.0));
	let declaration = a.variable_declaration(x, Some(five.into()));
	let first = stmts(&mut a, &[declaration]);

	let x = a.id("x");
	let declaration = a.variable_declaration(x, None);
	let second = stmts(&mut a, &[declaration]);

	let (x, y) = (a.id("a"), a.id("b"));
	let left = ids(&mut a, &[x, y]);
	let (one, two) = (a.num(1.0), a.num(2.0));
	let right = exprs(&mut a, &[one, two]);
	let declaration = a.variable_declaration(left, Some(right.into()));
	let third = stmts(&mut a, &[declaration]);

	let (x, one) = (a.id("x"), a.num(1.0));
	let assignment = a.assignment(x, AssignmentOperator::Add, one);
	let fourth = stmts(&mut a, &[assignment]);

	for (ast, want) in [
		(first, "local x = 5\n"),
		(second, "local x\n"),
		(third, "local a, b = 1, 2\n"),
		(fourth, "x += 1\n"),
	] {
		assert_eq!(render_both(&a, ast), want);
	}
}

#[test]
fn render_if_statement() {
	let mut a = Ast::new();
	let big = call_statement(&mut a, "print", vec![string("big")]);
	let inner = stmts(&mut a, &[big]);
	let small = call_statement(&mut a, "print", vec![string("small")]);
	let else_inner = stmts(&mut a, &[small]);
	let condition = bin(id("x"), B::Greater, num(3.0)).build(&mut a);
	let statement = a.if_statement(condition, inner, Some(else_inner.into()));
	let ast = stmts(&mut a, &[statement]);
	assert_eq!(
		render_both(&a, ast),
		"if x > 3 then\n\tprint(\"big\")\nelse\n\tprint(\"small\")\nend\n"
	);
}

#[test]
fn render_else_if_chain() {
	let mut a = Ast::new();
	let g = call_statement(&mut a, "g", vec![]);
	let body = stmts(&mut a, &[g]);
	let b = a.id("b");
	let elseif = a.if_statement(b, body, None);
	let f = call_statement(&mut a, "f", vec![]);
	let body = stmts(&mut a, &[f]);
	let condition = a.id("a");
	let statement = a.if_statement(condition, body, Some(elseif.into()));
	let ast = stmts(&mut a, &[statement]);
	assert_eq!(render_both(&a, ast), "if a then\n\tf()\nelseif b then\n\tg()\nend\n");
}

#[test]
fn render_loops() {
	let mut a = Ast::new();
	let f = call_statement(&mut a, "f", vec![]);
	let body = stmts(&mut a, &[f]);
	let mut cases: Vec<(NodeId, &str)> = Vec::new();

	let numeric = |a: &mut Ast, step: Option<NodeId>| {
		let (i, one, ten) = (a.id("i"), a.num(1.0), a.num(10.0));
		let clone = a.list_clone(body);
		a.numeric_for(i, one, ten, step, clone)
	};
	cases.push((numeric(&mut a, None), "for i = 1, 10 do\n\tf()\nend\n"));
	let step = a.num(1.0);
	// A step of 1 is omitted.
	cases.push((numeric(&mut a, Some(step)), "for i = 1, 10 do\n\tf()\nend\n"));
	let step = a.number_literal("0x1");
	// A hex step of 1 is omitted too (JS Number("0x1") == 1).
	cases.push((numeric(&mut a, Some(step)), "for i = 1, 10 do\n\tf()\nend\n"));
	let step = a.num(2.0);
	cases.push((numeric(&mut a, Some(step)), "for i = 1, 10, 2 do\n\tf()\nend\n"));

	let (k, v) = (a.id("k"), a.id("v"));
	let loop_ids = ids(&mut a, &[k, v]);
	let iterator = call(id("pairs"), vec![id("t")]).build(&mut a);
	let clone = a.list_clone(body);
	cases.push((
		a.for_statement(loop_ids, iterator, clone),
		"for k, v in pairs(t) do\n\tf()\nend\n",
	));

	let empty = a.new_list::<element::AnyIdentifier>();
	let iterator = a.id("it");
	let clone = a.list_clone(body);
	// Empty ids render as `_`.
	cases.push((a.for_statement(empty, iterator, clone), "for _ in it do\n\tf()\nend\n"));

	let condition = a.bool(true);
	let clone = a.list_clone(body);
	cases.push((a.while_statement(condition, clone), "while true do\n\tf()\nend\n"));

	let condition = a.id("done");
	let clone = a.list_clone(body);
	cases.push((a.repeat_statement(condition, clone), "repeat\n\tf()\nuntil done\n"));

	for (statement, want) in cases {
		let ast = stmts(&mut a, &[statement]);
		assert_eq!(render_both(&a, ast), want);
	}
}

#[test]
fn render_functions() {
	let mut a = Ast::new();
	let x = a.id("x");
	let ret = a.return_statement(x);
	let body = stmts(&mut a, &[ret]);
	let param = a.id("x");
	let params = ids(&mut a, &[param]);
	let name = a.id("f");
	let declaration = a.function_declaration(true, name, params, false, body);
	let ast = stmts(&mut a, &[declaration]);
	assert_eq!(render_both(&a, ast), "local function f(x)\n\treturn x\nend\n");

	let nil = a.nil();
	let ret = a.return_statement(nil);
	let body = stmts(&mut a, &[ret]);
	let (class, params) = (a.id("Class"), a.new_list::<element::AnyIdentifier>());
	let method = a.method_declaration(class, "method", params, false, body);
	let ast = stmts(&mut a, &[method]);
	assert_eq!(render_both(&a, ast), "function Class:method()\n\treturn nil\nend\n");
}

#[test]
fn render_comment() {
	let mut a = Ast::new();
	let cases = [
		("hello", "--hello\n"),
		("line1\nline2", "--[[\n\tline1\n\tline2\n]]\n"),
		// Text containing "]]" forces the bracket padding up to "=".
		("a]]b\nc", "--[=[\n\ta]]b\n\tc\n]=]\n"),
		("line1\r\nline2", "--[[\n\tline1\n\tline2\n]]\n"),
	];
	for (text, want) in cases {
		let comment = a.comment(text);
		let ast = stmts(&mut a, &[comment]);
		assert_eq!(render_both(&a, ast), want);
	}
}

#[test]
fn semicolon_ambiguity() {
	// `local a = b` followed by a statement starting with a parenthesis needs
	// `;`.
	let mut a = Ast::new();
	let (x, b) = (a.id("a"), a.id("b"));
	let first = a.variable_declaration(x, Some(b.into()));
	let access = property(paren(bin(id("c"), B::Add, id("d"))), "e").build(&mut a);
	let f = a.id("f");
	let second = a.assignment(access, AssignmentOperator::Assign, f);
	let ast = stmts(&mut a, &[first, second]);
	assert_eq!(render_both(&a, ast), "local a = b;\n(c + d).e = f\n");
}

#[test]
fn temp_ids_end_to_end() {
	let mut a = Ast::new();
	let temp = a.temp_id("result");
	let one = a.num(1.0);
	let declaration = a.variable_declaration(temp, Some(one.into()));
	let print = call(id("print"), vec![temp.into()]).build(&mut a);
	let statement = a.call_statement(print);
	let ast = stmts(&mut a, &[declaration, statement]);
	assert_eq!(render_both(&a, ast), "local _result = 1\nprint(_result)\n");
}

#[test]
fn statement_after_final_panics() {
	let mut a = Ast::new();
	let nil = a.nil();
	let ret = a.return_statement(nil);
	let brk = a.break_statement();
	let ast = stmts(&mut a, &[ret, brk]);
	let error = render_ast(&a, ast).expect_err("must fail on statement after return");
	assert_eq!(
		error.message(),
		"Cannot render statement after break, continue, or return!"
	);
}

#[test]
fn render_typed_function_signatures() {
	let mut a = Ast::new();
	let (x, y) = (a.id("a"), a.id("b"));
	let vector = a.type_name("vector");
	a.set_binding_type(x, Some(vector));
	let optional = super::dsl::optional(tn("vector")).build(&mut a);
	a.set_binding_type(y, Some(optional));
	let (name, params, body) = (a.id("f"), ids(&mut a, &[x, y]), a.new_list::<element::Statement>());
	let declaration = a.function_declaration(true, name, params, false, body);
	let return_type = super::dsl::optional(tn("number")).build(&mut a);
	a.get_mut::<FunctionDeclaration>(declaration)
		.expect("declaration")
		.return_type = Some(return_type);
	let ast = stmts(&mut a, &[declaration]);
	assert_eq!(
		render_both(&a, ast),
		"local function f(a: vector, b: vector?): number?\nend\n"
	);
}

#[test]
fn render_type_alias() {
	let mut a = Ast::new();
	let (key_type, value_type) = (a.type_name("K"), a.type_name("V"));
	let (key, value) = (
		a.type_table_prop(TypeAccess::None, "key", key_type),
		a.type_table_prop(TypeAccess::None, "value", value_type),
	);
	let fields = a.list_from::<element::TypeTableField>([key, value]);
	let table = a.type_table(fields);
	let node = a.type_alias("Node", vec![TypeGeneric::new("K"), TypeGeneric::new("V")], Some(table));
	let first = stmts(&mut a, &[node]);

	let (any, number) = (a.type_name("any"), a.type_name("number"));
	let indexer = a.type_table_indexer(TypeAccess::None, any, number);
	let fields = a.list_from::<element::TypeTableField>([indexer]);
	let table = a.type_table(fields);
	let both = a.type_alias("Both", Vec::new(), Some(table));
	let second = stmts(&mut a, &[both]);

	for (ast, want) in [
		(first, "type Node<K, V> = { key: K, value: V }\n"),
		(second, "type Both = { [any]: number }\n"),
	] {
		assert_eq!(render_both(&a, ast), want);
	}
}

#[test]
fn render_function_generics() {
	let mut a = Ast::new();
	let x = a.id("x");
	let ret = a.return_statement(x);
	let body = stmts(&mut a, &[ret]);
	let param = a.id("x");
	let params = ids(&mut a, &[param]);

	let (name, clone) = (a.id("f"), a.list_clone(body));
	let declaration = a.function_declaration(true, name, params, false, clone);
	a.get_mut::<FunctionDeclaration>(declaration)
		.expect("declaration")
		.generics = vec![TypeGeneric::new("K"), TypeGeneric::new("V")];
	let ast = stmts(&mut a, &[declaration]);
	assert_eq!(render_both(&a, ast), "local function f<K, V>(x)\n\treturn x\nend\n");

	let (class, params, clone) = (
		a.id("Class"),
		a.new_list::<element::AnyIdentifier>(),
		a.list_clone(body),
	);
	let method = a.method_declaration(class, "method", params, false, clone);
	a.get_mut::<MethodDeclaration>(method).expect("method").generics = vec![TypeGeneric::new("T")];
	let ast = stmts(&mut a, &[method]);
	assert_eq!(render_both(&a, ast), "function Class:method<T>()\n\treturn x\nend\n");

	let (params, clone) = (a.new_list::<element::AnyIdentifier>(), a.list_clone(body));
	let expression = a.function_expression(params, false, clone);
	a.get_mut::<FunctionExpression>(expression)
		.expect("expression")
		.generics = vec![TypeGeneric::new("T")];
	let f = a.id("f");
	let variable = a.variable_declaration(f, Some(expression.into()));
	let ast = stmts(&mut a, &[variable]);
	assert_eq!(render_both(&a, ast), "local f = function<T>()\n\treturn x\nend\n");
}
