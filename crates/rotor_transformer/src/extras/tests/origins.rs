//! `origin_tagging_test.go`, `origin_body_test.go`, `origin_classes_test.go`,
//! `origin_control_test.go`, `origin_exports_test.go`, `origin_statements_test.go`,
//! `expression_origins_test.go` and `sourcemap_test.go`'s `TestSourcePositionExportSyntaxAnchors`:
//! the source anchors `RenderWithSourceMap` reads.

use std::collections::HashMap;
use std::path::Path;

use rotor_luau::render::render_ast_with_positions;
use rotor_luau::{
	Assignment, BinaryExpression, CallExpression, CallStatement, ComputedIndexExpression, DoStatement,
	FunctionDeclaration, FunctionExpression, IfExpression, IfStatement, Map, MapField, MethodDeclaration, Origin,
	Position, PropertyAccessExpression, RepeatStatement, ReturnStatement, StringLiteral, TemporaryIdentifier,
	VariableDeclaration,
};

use super::{with_origin_source, write_fixture};
use crate::dispatch::{tag_expression_origin, transform_expression};
use crate::prelude::*;
use crate::sourcefile::transform_source_file;
use crate::statementlist::transform_statement_list;
use crate::tests::with_state;

fn position(line: u32, column: u32) -> Position {
	Position::new(line, column)
}

fn origin(ast: &Ast, node: NodeId) -> Option<Origin> {
	ast.origin(node).map(|origin| *ast.origin_data(origin))
}

/// Go's `assertNodeOrigin`.
fn assert_node_origin(ast: &Ast, node: NodeId, start: Position, closing: Position, name: &str) {
	let Some(origin) = origin(ast, node) else {
		panic!("{name}: {:?} has no origin", ast.kind(node));
	};
	assert_eq!(origin.start, start, "{name}: {:?} start", ast.kind(node));
	assert_eq!(origin.closing, Some(closing), "{name}: {:?} closing", ast.kind(node));
}

fn node_of(value: Option<NodeOrList>) -> NodeId {
	match value {
		Some(NodeOrList::Node(node)) => node,
		other => panic!("expected a node, got {other:?}"),
	}
}

fn declaration_right(ast: &Ast, statement: NodeId) -> NodeId {
	node_of(
		ast.get::<VariableDeclaration>(statement)
			.expect("a variable declaration")
			.right,
	)
}

fn function_statements(ast: &Ast, function: NodeId) -> Vec<NodeId> {
	if let Some(declaration) = ast.get::<FunctionDeclaration>(function) {
		return ast.list_to_vec(declaration.statements);
	}
	if let Some(method) = ast.get::<MethodDeclaration>(function) {
		return ast.list_to_vec(method.statements);
	}
	let expression = ast.get::<FunctionExpression>(function).expect("a function");
	ast.list_to_vec(expression.statements)
}

fn if_statements(ast: &Ast, statement: NodeId) -> Vec<NodeId> {
	ast.list_to_vec(ast.get::<IfStatement>(statement).expect("an if statement").statements)
}

/// The project Go's `expressionOriginState` builds: `input.ts` with `sourceMap` set to `maps`.
fn with_expression_origin_state<R: Send>(source: &str, maps: bool, test: impl FnOnce(&mut State<'_>) -> R + Send) -> R {
	let directory = tempfile::tempdir().expect("a temporary directory");
	write_fixture(
		directory.path(),
		"tsconfig.json",
		&format!(
			r#"{{"compilerOptions":{{"target":"ESNext","strict":true,"types":[],"moduleDetection":"force","sourceMap":{maps}}}}}"#
		),
	);
	write_fixture(directory.path(), "input.ts", source);
	let root = directory.path().canonicalize().expect("the directory resolves");
	with_state(&root, "input.ts", test)
}

fn with_project<R: Send>(tsconfig: &str, source: &str, test: impl FnOnce(&mut State<'_>) -> R + Send) -> R {
	let directory = tempfile::tempdir().expect("a temporary directory");
	write_fixture(directory.path(), "tsconfig.json", tsconfig);
	write_fixture(directory.path(), "input.ts", source);
	let root: &Path = &directory.path().canonicalize().expect("the directory resolves");
	with_state(root, "input.ts", test)
}

fn file_statements(state: &mut State<'_>) -> Statements {
	let file = state.source_file;
	transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None)
}

fn expression_of_statement(state: &State<'_>, index: usize) -> P<Node> {
	state.source_file.statements().nodes()[index]
		.as_expression_statement()
		.expression
}

// ---------------------------------------------------------------------------------------------
// origin_tagging_test.go
// ---------------------------------------------------------------------------------------------

#[test]
fn function_origins() {
	type Select = fn(&Ast, NodeId) -> NodeId;
	let tests: [(&str, &str, Select, Position, Position); 6] = [
		(
			"function expression",
			"const callback = function() {\n\treturn 1;\n};",
			declaration_right,
			position(0, 17),
			position(2, 0),
		),
		(
			"arrow",
			"const callback = () => {\n\treturn 1;\n};",
			declaration_right,
			position(0, 17),
			position(2, 0),
		),
		(
			"object method",
			"const object = {\n\tmethod() {\n\t\treturn 1;\n\t},\n};",
			|ast, statement| {
				let map = ast.get::<Map>(declaration_right(ast, statement)).expect("a map");
				let field = ast.list_to_vec(map.fields)[0];
				ast.get::<MapField>(field).expect("a map field").value
			},
			position(1, 1),
			position(3, 1),
		),
		(
			"async declaration callback",
			"async function task() {\n\treturn 1;\n}",
			|ast, statement| {
				let call = ast
					.get::<CallExpression>(declaration_right(ast, statement))
					.expect("a call");
				ast.list_to_vec(call.args)[0]
			},
			position(0, 0),
			position(2, 0),
		),
		(
			"named expression declaration",
			"const callback = function named() {\n\treturn 1;\n};",
			|_, statement| statement,
			position(0, 17),
			position(2, 0),
		),
		(
			"matching named expression declaration",
			"export {};\nconst named = function named() {\n\treturn 1;\n};",
			|_, statement| statement,
			position(1, 14),
			position(3, 0),
		),
	];
	for (name, source, select, start, closing) in tests {
		with_origin_source(source, |state, statements| {
			let head = state.ast.list_to_vec(statements)[0];
			let node = select(&state.ast, head);
			assert_node_origin(&state.ast, node, start, closing, name);
		});
	}
}

// ---------------------------------------------------------------------------------------------
// origin_body_test.go
// ---------------------------------------------------------------------------------------------

#[test]
fn parameter_prologue_origins() {
	let tests = [
		(
			"default",
			"function f(value = 1) { return value; }",
			position(0, 11),
			position(0, 19),
			1,
		),
		(
			"destructuring",
			"function f([x, y]: number[]) { return x + y; }",
			position(0, 11),
			position(0, 26),
			2,
		),
		(
			"rest capture",
			"function f(...rest: number[]) { return rest; }",
			position(0, 11),
			position(0, 27),
			1,
		),
		(
			"flattened rest default",
			"function f(...[x = 1]: [number?]) { return x; }",
			position(0, 11),
			position(0, 31),
			1,
		),
	];
	for (name, source, start, closing, prologues) in tests {
		with_origin_source(source, |state, statements| {
			let ast = &state.ast;
			let function = ast.list_to_vec(statements)[0];
			let body = function_statements(ast, function);
			assert_eq!(body.len(), prologues + 1, "{name}: body statements");
			for &statement in &body[..prologues] {
				assert_node_origin(ast, statement, start, closing, name);
				assert_eq!(
					ast.origin(statement),
					ast.origin(body[0]),
					"{name}: prologues for one parameter must share an origin"
				);
				if ast.get::<IfStatement>(statement).is_some() {
					assert!(
						ast.origin(if_statements(ast, statement)[0]).is_none(),
						"{name}: synthetic initializer assignment must inherit the parameter origin"
					);
				}
			}
		});
	}
}

// ---------------------------------------------------------------------------------------------
// origin_classes_test.go
// ---------------------------------------------------------------------------------------------

#[test]
fn class_member_origins() {
	let source = "class Point {
	value = 1;
	static origin = 0;
	constructor(public x: number) {
		this.x = x;
	}
	method() {
		return this.x;
	}
	static read(this: void) {
		return 1;
	}
	static {
		this.origin += 1;
	}
}";
	with_origin_source(source, |state, statements| {
		let ast = &state.ast;
		let block = ast.list_to_vec(statements)[1];
		let block_statements = ast.list_to_vec(ast.get::<DoStatement>(block).expect("a do block").statements);
		let mut methods: HashMap<String, NodeId> = HashMap::new();
		let mut static_method = None;
		let mut static_property = None;
		let mut static_block = None;
		for &statement in &block_statements {
			if let Some(method) = ast.get::<MethodDeclaration>(statement) {
				methods.insert(method.name.clone(), statement);
			} else if let Some(function) = ast.get::<FunctionDeclaration>(statement) {
				if ast
					.get::<PropertyAccessExpression>(function.name)
					.is_some_and(|property| property.name == "read")
				{
					static_method = Some(statement);
				}
			} else if let Some(assignment) = ast.get::<Assignment>(statement) {
				if let NodeOrList::Node(left) = assignment.left
					&& let Some(property) = ast.get::<ComputedIndexExpression>(left)
					&& ast
						.get::<StringLiteral>(property.index)
						.is_some_and(|name| name.value == "origin")
				{
					static_property = Some(statement);
				}
			} else if ast.get::<DoStatement>(statement).is_some() {
				static_block = Some(statement);
			}
		}
		let constructor = *methods.get("constructor").expect("the constructor");
		let method = *methods.get("method").expect("the method");
		let constructor_body = function_statements(ast, constructor);
		let tests = [
			("constructor", constructor, position(3, 1), position(5, 1)),
			(
				"parameter property",
				constructor_body[0],
				position(3, 13),
				position(3, 28),
			),
			(
				"instance property",
				constructor_body[1],
				position(1, 1),
				position(1, 10),
			),
			("method", method, position(6, 1), position(8, 1)),
			(
				"static function",
				static_method.expect("the static function"),
				position(9, 1),
				position(11, 1),
			),
			(
				"static property",
				static_property.expect("the static property"),
				position(2, 1),
				position(2, 18),
			),
			(
				"static block",
				static_block.expect("the static block"),
				position(12, 1),
				position(14, 1),
			),
		];
		for (name, node, start, closing) in tests {
			assert_node_origin(ast, node, start, closing, name);
		}
		assert!(
			ast.origin(block_statements[0]).is_none(),
			"class boilerplate must inherit the class origin"
		);
	});
}

#[test]
fn class_property_prereq_origins() {
	let source = "function make(a?: number) {\n\tclass Example {\n\t\tvalue = a ?? 1;\n\t\tstatic value = a ?? 2;\n\t}\n\treturn Example;\n}";
	with_origin_source(source, |state, statements| {
		let ast = &state.ast;
		let function = ast.list_to_vec(statements)[0];
		let block = function_statements(ast, function)[1];
		let class_body = ast.list_to_vec(ast.get::<DoStatement>(block).expect("a do block").statements);
		let constructor = class_body
			.iter()
			.copied()
			.find(|&statement| {
				ast.get::<MethodDeclaration>(statement)
					.is_some_and(|method| method.name == "constructor")
			})
			.expect("the implicit constructor");
		let instance = function_statements(ast, constructor);
		let static_statements = class_body[class_body.len() - 3..].to_vec();
		let tests = [
			("instance initializer", instance, position(2, 2), position(2, 16)),
			("static initializer", static_statements, position(3, 2), position(3, 23)),
		];
		for (name, statements, start, closing) in tests {
			assert_eq!(statements.len(), 3, "{name}: initializer statements");
			for &statement in &statements {
				assert_node_origin(ast, statement, start, closing, name);
				assert_eq!(
					ast.origin(statement),
					ast.origin(statements[0]),
					"{name}: property prereqs and assignment must share an origin"
				);
			}
			assert!(
				ast.origin(if_statements(ast, statements[1])[0]).is_none(),
				"{name}: nested synthetic assignment must inherit the property origin"
			);
		}
		assert!(
			ast.origin(constructor).is_none(),
			"implicit constructor must inherit the class origin"
		);
	});
}

// ---------------------------------------------------------------------------------------------
// origin_control_test.go
// ---------------------------------------------------------------------------------------------

#[test]
fn else_if_origins() {
	for condition in ["a === 1", "(a ?? 0) === 1"] {
		let source = format!(
			"function f(a?: number) {{\n\tif (a === 0) {{\n\t\treturn 0;\n\t}} else if ({condition}) {{\n\t\treturn 1;\n\t}}\n\treturn 2;\n}}"
		);
		with_origin_source(&source, |state, statements| {
			let ast = &state.ast;
			let function = ast.list_to_vec(statements)[0];
			let outer = function_statements(ast, function)[0];
			assert_node_origin(ast, outer, position(1, 1), position(5, 1), condition);
			let else_body = ast.get::<IfStatement>(outer).expect("an if").else_body;
			match else_body {
				NodeOrList::Node(linked) => {
					assert_node_origin(ast, linked, position(3, 8), position(5, 1), condition);
				}
				NodeOrList::List(list) => {
					let list = list.typed::<element::Statement>().expect("a statement list");
					let statements = ast.list_to_vec(list);
					assert_eq!(statements.len(), 3, "{condition}: else statements");
					for &statement in &statements {
						assert_node_origin(ast, statement, position(3, 8), position(5, 1), condition);
					}
					assert_eq!(
						ast.origin(statements[0]),
						ast.origin(statements[1]),
						"{condition}: else-if prereqs must share their origin"
					);
					assert!(
						ast.origin(if_statements(ast, statements[1])[0]).is_none(),
						"{condition}: nested nullish assignment must inherit the inner if origin"
					);
				}
			}
		});
	}
}

#[test]
fn switch_clause_origins() {
	let source = "function f(n: number, a?: number) {\n\tswitch (n) {\n\t\tcase 1:\n\t\t\tn = 2;\n\t\tcase a ?? 2:\n\t\t\tbreak;\n\t}\n}";
	with_origin_source(source, |state, statements| {
		let ast = &state.ast;
		let function = ast.list_to_vec(statements)[0];
		let body = function_statements(ast, function);
		let repeat = *body.last().expect("the switch");
		let statements = ast.list_to_vec(ast.get::<RepeatStatement>(repeat).expect("a repeat").statements);
		assert_eq!(statements.len(), 4, "switch statements: flag, case, prereq, case");
		let (first, prereq, second) = (statements[1], statements[2], statements[3]);
		let first_body = if_statements(ast, first);
		let prereq_body = if_statements(ast, prereq);
		let tests = [
			("first case", first, position(2, 2), position(3, 8)),
			(
				"fallthrough update",
				*first_body.last().expect("a body"),
				position(2, 2),
				position(3, 8),
			),
			("second case", second, position(4, 2), position(5, 8)),
			("clause prereq", prereq, position(4, 2), position(5, 8)),
			(
				"fallthrough condition assignment",
				*prereq_body.last().expect("a body"),
				position(4, 2),
				position(5, 8),
			),
		];
		for (name, node, start, closing) in tests {
			assert_node_origin(ast, node, start, closing, name);
		}
		assert!(
			ast.origin(statements[0]).is_none(),
			"initial fallthrough flag must inherit the switch origin"
		);
		assert!(
			ast.origin(if_statements(ast, prereq_body[1])[0]).is_none(),
			"nested nullish assignment must inherit its enclosing clause origin"
		);
	});
}

#[test]
fn try_callback_origins() {
	let source = "function attempt() {\n\ttry {\n\t\trisky();\n\t} catch (e) {\n\t\tuse(e);\n\t} finally {\n\t\trisky();\n\t}\n}\ndeclare function risky(): void;\ndeclare function use(value: unknown): void;";
	with_origin_source(source, |state, statements| {
		let ast = &state.ast;
		let function = *ast.list_to_vec(statements).last().expect("the function");
		let call_statement = function_statements(ast, function)[0];
		let call = ast
			.get::<CallStatement>(call_statement)
			.expect("a call statement")
			.expression;
		let callbacks = ast.list_to_vec(ast.get::<CallExpression>(call).expect("a call").args);
		let tests = [
			("try block", position(1, 5), position(3, 1)),
			("catch clause", position(3, 3), position(5, 1)),
			("finally block", position(5, 11), position(7, 1)),
		];
		for (index, (name, start, closing)) in tests.into_iter().enumerate() {
			assert_node_origin(ast, callbacks[index], start, closing, name);
		}
		let first = function_statements(ast, callbacks[0])[0];
		assert_node_origin(ast, first, position(2, 2), position(2, 9), "try body");
	});
}

// ---------------------------------------------------------------------------------------------
// origin_exports_test.go and sourcemap_test.go TestSourcePositionExportSyntaxAnchors
// ---------------------------------------------------------------------------------------------

#[test]
fn export_return_map_field_origins() {
	with_project(
		r#"{"compilerOptions":{"target":"ESNext","types":[]}}"#,
		"export const value = 1;\nexport { value as alias };\nexport default value;\n",
		|state| {
			let statements = transform_source_file(state);
			let diagnostics = state.diags.flush();
			assert!(diagnostics.is_empty(), "transform diagnostics: {diagnostics:?}");
			let ast = &state.ast;
			let returned = *ast.list_to_vec(statements).last().expect("the return");
			let table = node_of(Some(
				ast.get::<ReturnStatement>(returned)
					.expect("a return statement")
					.expression,
			));
			let mut fields: HashMap<String, NodeId> = HashMap::new();
			for field in ast.list_to_vec(ast.get::<Map>(table).expect("a map").fields) {
				let index = ast.get::<MapField>(field).expect("a field").index;
				let key = ast.get::<StringLiteral>(index).expect("a string key").value.clone();
				fields.insert(key, field);
			}
			assert_eq!(fields.len(), 3, "export fields");
			let tests = [
				("value", position(0, 0), position(0, 22)),
				("alias", position(1, 9), position(1, 22)),
				("default", position(2, 0), position(2, 20)),
			];
			for (name, start, closing) in tests {
				assert_node_origin(ast, fields[name], start, closing, name);
			}
			assert!(
				ast.origin(returned).is_none() && ast.origin(table).is_none(),
				"export return wrapper must remain untagged"
			);
		},
	);
}

#[test]
fn source_position_export_syntax_anchors() {
	// The mutable export selects exports-table assignments, which already have source anchors.
	with_project(
		r#"{"compilerOptions":{"target":"ESNext"}}"#,
		"export const value = 1;\nexport { value as alias };\nexport default value;\nexport let mutable = 0;\n",
		|state| {
			let statements = transform_source_file(state);
			let ast = &state.ast;
			let mut origins: HashMap<String, Option<Origin>> = HashMap::new();
			for statement in ast.list_to_vec(statements) {
				if let Some(assignment) = ast.get::<Assignment>(statement)
					&& let NodeOrList::Node(left) = assignment.left
					&& let Some(property) = ast.get::<PropertyAccessExpression>(left)
				{
					origins.insert(property.name.clone(), origin(ast, statement));
				}
			}
			let tests = [
				("export specifier", "alias", position(1, 9), position(1, 22)),
				("export assignment", "default", position(2, 0), position(2, 20)),
				("export declaration", "value", position(0, 0), position(0, 22)),
			];
			for (name, export_name, start, closing) in tests {
				let Some(Some(origin)) = origins.get(export_name) else {
					panic!("{name}: export {export_name} has no origin");
				};
				assert_eq!(origin.start, start, "{name}: start");
				assert_eq!(origin.closing, Some(closing), "{name}: closing");
			}
		},
	);
}

// ---------------------------------------------------------------------------------------------
// origin_statements_test.go
// ---------------------------------------------------------------------------------------------

#[test]
fn concise_arrow_body_origins() {
	with_origin_source("const callback = (a?: number) =>\n\ta ?? 1;", |state, statements| {
		let ast = &state.ast;
		let function = declaration_right(ast, ast.list_to_vec(statements)[0]);
		let body = function_statements(ast, function);
		assert_eq!(body.len(), 3, "body statements: local, if, return");
		for &statement in &body {
			assert_node_origin(ast, statement, position(1, 1), position(1, 6), "concise body");
			assert_eq!(
				ast.origin(statement),
				ast.origin(body[0]),
				"concise-arrow prereqs and return must share an origin"
			);
		}
		assert!(
			ast.origin(if_statements(ast, body[1])[0]).is_none(),
			"nested synthetic nullish assignment must inherit the body origin"
		);
	});
}

#[test]
fn statement_origins_include_prereqs_and_hoists() {
	// prereqs share the statement origin
	with_origin_source(
		"function f(a?: number) {\n\tconst result = a ?? 1;\n\treturn result;\n}",
		|state, statements| {
			let ast = &state.ast;
			let function = ast.list_to_vec(statements)[0];
			let body = function_statements(ast, function);
			assert_eq!(body.len(), 4, "body statements: local, if, local, return");
			for &statement in &body[..3] {
				assert_node_origin(ast, statement, position(1, 1), position(1, 22), "prereqs");
				assert_eq!(
					ast.origin(statement),
					ast.origin(body[0]),
					"prereqs and result must share their TS statement's origin"
				);
			}
		},
	);
	// hoist shares the triggering statement origin
	with_origin_source(
		"function first() {\n\treturn later();\n}\nfunction later() { return 1; }",
		|state, statements| {
			let ast = &state.ast;
			let statements = ast.list_to_vec(statements);
			assert_eq!(statements.len(), 3, "statements: hoist and two functions");
			assert!(
				ast.get::<VariableDeclaration>(statements[0]).is_some(),
				"first statement is the hoist declaration"
			);
			for &statement in &statements[..2] {
				assert_node_origin(ast, statement, position(0, 0), position(2, 0), "hoist");
			}
			assert_eq!(
				ast.origin(statements[0]),
				ast.origin(statements[1]),
				"hoist and function must share the triggering statement's origin"
			);
		},
	);
}

// ---------------------------------------------------------------------------------------------
// expression_origins_test.go
// ---------------------------------------------------------------------------------------------

#[test]
fn expression_origins_are_per_occurrence() {
	for cached in [false, true] {
		with_expression_origin_state("declare const a: number;\na + a;", true, |state| {
			let node = expression_of_statement(state, 1);
			let binary = node.as_binary_expression();
			let right_node = binary.right.get();
			let id = if cached {
				let id = state.ast.temp_id("a");
				let symbol = state.ck().get_symbol_at_location(binary.left).expect("a symbol");
				state.symbol_to_id.insert(symbol, id);
				Some(id)
			} else {
				None
			};
			let left = transform_expression(state, binary.left);
			if let Some(id) = id {
				assert!(
					state.ast.origin(id).is_none(),
					"first occurrence tagged the cached identifier"
				);
			}
			let right = transform_expression(state, right_node);
			assert_ne!(left, right, "occurrences share a node");
			for (index, expression) in [left, right].into_iter().enumerate() {
				let want = position(1, u32::try_from(index * 4).expect("a column"));
				assert_eq!(
					origin(&state.ast, expression).map(|origin| origin.start),
					Some(want),
					"cached={cached}: occurrence {index} origin"
				);
				if let Some(id) = id {
					let temp = state
						.ast
						.get::<TemporaryIdentifier>(expression)
						.expect("a temporary")
						.id;
					let original = state.ast.get::<TemporaryIdentifier>(id).expect("a temporary").id;
					assert_eq!(temp, original, "clone changed temporary identity");
				}
			}
		});
	}
}

#[test]
fn cached_object_key_origins_are_per_occurrence() {
	with_expression_origin_state("\"key\";\n\"key\";", true, |state| {
		let first = expression_of_statement(state, 0);
		let second = expression_of_statement(state, 1);
		let key = state.ast.str("key");
		state.set_class_element_object_key(first, key);
		let cached = state
			.get_class_element_object_key(first)
			.expect("a class element object key");
		for (line, node) in [first, second].into_iter().enumerate() {
			let occurrence = tag_expression_origin(state, cached, node);
			let line = u32::try_from(line).expect("a line");
			assert!(
				occurrence != key
					&& origin(&state.ast, occurrence).map(|origin| origin.start) == Some(position(line, 0)),
				"cached key occurrence {line} has wrong identity or origin: {:?}",
				origin(&state.ast, occurrence)
			);
			assert!(
				state.ast.origin(key).is_none() && state.ast.parent(key).is_none(),
				"tagging an occurrence mutated the cached key"
			);
		}
	});
}

#[test]
fn expression_origins_disabled() {
	with_expression_origin_state("declare const a: number;\na + 1;", false, |state| {
		let node = expression_of_statement(state, 1);
		let (transformed, _) = state.capture(|state| transform_expression(state, node));
		let binary = state.ast.get::<BinaryExpression>(transformed).expect("a binary");
		for item in [transformed, binary.left, binary.right] {
			assert!(
				state.ast.origin(item).is_none(),
				"maps off: {:?} has origin {:?}",
				state.ast.kind(item),
				origin(&state.ast, item)
			);
		}
	});
}

#[test]
fn expression_wrapper_keeps_inner_origin() {
	with_expression_origin_state("(1 as number)!;", true, |state| {
		let node = expression_of_statement(state, 0);
		let expression = transform_expression(state, node);
		assert_eq!(
			origin(&state.ast, expression).map(|origin| origin.start),
			Some(position(0, 1)),
			"wrapper origin, want inner token at 0:1"
		);
	});
}

#[test]
fn assignment_operator_keeps_statement_origin() {
	with_expression_origin_state("let x = 0;\nx += 1;\nx++;", true, |state| {
		let statements = file_statements(state);
		let statements = state.ast.list_to_vec(statements);
		for (index, column) in [2, 1].into_iter().enumerate() {
			let line = u32::try_from(index + 1).expect("a line");
			let origin = origin(&state.ast, statements[index + 1]);
			let Some(origin) = origin.filter(|origin| origin.start == position(line, 0) && origin.closing.is_some())
			else {
				panic!("statement {} lost its source span: {origin:?}", index + 1);
			};
			assert_eq!(
				origin.operator,
				Some(position(line, column)),
				"statement {} operator",
				index + 1
			);
		}
	});
}

#[test]
fn compound_assignment_expression_operator() {
	with_expression_origin_state("let x = 0;\nconst value = (x += 1);", true, |state| {
		let statements = file_statements(state);
		let statement = state.ast.list_to_vec(statements)[1];
		assert!(state.ast.get::<Assignment>(statement).is_some(), "an assignment");
		assert_eq!(
			origin(&state.ast, statement).and_then(|origin| origin.operator),
			Some(position(1, 17)),
			"compound assignment expression operator"
		);
	});
}

#[test]
fn logical_assignment_operator_origins() {
	for operator in ["??=", "&&=", "||="] {
		let source = format!("let value: boolean | undefined = true;\nvalue {operator} false;");
		with_expression_origin_state(&source, true, |state| {
			let statements = file_statements(state);
			let mut found = false;
			for statement in state.ast.list_to_vec(statements) {
				if let Some(branch) = state.ast.get::<IfStatement>(statement) {
					found = true;
					let want = position(1, 6);
					let origin = origin(&state.ast, branch.condition);
					assert!(
						origin.is_some_and(|origin| origin.start == want && origin.operator == Some(want)),
						"{operator}: condition origin = {origin:?}, want start and operator {want:?}"
					);
				}
			}
			assert!(found, "{operator}: logical assignment has no condition");
		});
	}
}

fn rendered_ranges(state: &State<'_>, statements: Statements) -> Vec<rotor_luau::render::Range> {
	render_ast_with_positions(&state.ast, statements)
		.expect("the statements render")
		.1
}

#[test]
fn optional_chain_operator_origins() {
	let source = "declare const object: { value: number; method(): number } | undefined;
const first = object?.value;
const second = object?.method?.();
";
	with_expression_origin_state(source, true, |state| {
		let statements = file_statements(state);
		let want = [position(1, 20), position(2, 21), position(2, 29)];
		let mut got = Vec::new();
		for range in rendered_ranges(state, statements) {
			if let Some(binary) = state.ast.get::<BinaryExpression>(range.node)
				&& binary.operator == BinaryOperator::NotEqual
				&& let Some(operator) = origin(&state.ast, range.node).and_then(|origin| origin.operator)
			{
				got.push(operator);
			}
		}
		assert_eq!(got, want, "optional chain operators");
	});
}

#[test]
fn ternary_origins_do_not_invent_operators() {
	with_expression_origin_state(
		"declare const test: number;\nconst value = test ? 1 : 2;",
		true,
		|state| {
			let statements = file_statements(state);
			let mut ternary = None;
			for range in rendered_ranges(state, statements) {
				if state.ast.get::<IfExpression>(range.node).is_some() {
					ternary = Some(range.node);
				}
				assert!(
					origin(&state.ast, range.node).is_none_or(|origin| origin.operator.is_none()),
					"ternary or truthiness node {:?} has an invented operator",
					state.ast.kind(range.node)
				);
			}
			let ternary = state
				.ast
				.get::<IfExpression>(ternary.expect("a ternary expression"))
				.expect("an if expression");
			for expression in [ternary.condition, ternary.expression, ternary.alternative] {
				assert!(
					state.ast.origin(expression).is_some(),
					"ternary operand {:?} has no source start",
					state.ast.kind(expression)
				);
			}
		},
	);
}

#[test]
fn in_operator_origin_on_continuation_line() {
	with_expression_origin_state(
		"declare const object: { key: number };\nconst present = \"key\"\n\tin object;",
		true,
		|state| {
			let statements = file_statements(state);
			let declaration = state.ast.list_to_vec(statements)[0];
			let expression = declaration_right(&state.ast, declaration);
			assert!(state.ast.get::<BinaryExpression>(expression).is_some(), "a binary");
			assert_eq!(
				origin(&state.ast, expression).and_then(|origin| origin.operator),
				Some(position(2, 1)),
				"in comparison operator"
			);
			assert!(
				rendered_ranges(state, statements)
					.iter()
					.any(|range| range.node == expression && range.operator.is_some()),
				"in comparison has no generated operator marker"
			);
		},
	);
}

#[test]
fn array_index_adjustment_keeps_operator_origin() {
	with_expression_origin_state(
		"declare const array: number[];\ndeclare const index: number;\nconst value = array[index\n\t+ 2];",
		true,
		|state| {
			let statements = file_statements(state);
			let addition = rendered_ranges(state, statements)
				.into_iter()
				.find(|range| {
					state
						.ast
						.get::<BinaryExpression>(range.node)
						.is_some_and(|binary| binary.operator == BinaryOperator::Add)
				})
				.expect("array index has no addition operator");
			assert_eq!(
				origin(&state.ast, addition.node).and_then(|origin| origin.operator),
				Some(position(3, 1)),
				"adjusted index operator"
			);
		},
	);
}
