//! `luautypes_typefunction_prelude.go`: the Luau side of type-function emission: a small
//! constructor DSL for the ordinary Lua a `type function` body is made of, the local helper
//! functions a body may need, and the prelude: the hand-written type functions that stand in
//! for TypeScript's built-in helper types when those are used generically (`Partial<T>` with a
//! free `T` becomes `__Partial<T>`).

use std::collections::HashSet;

use rotor_luau::{FunctionDeclaration, Identifier, TypeFunctionDeclaration, TypeGeneric};

use super::luautypes::{AliasPlacement, TypeAliasEntry, types};
use super::nativetypes::{NATIVE_TYPE_STRING, NativeTypeKind};
use crate::prelude::*;

// -----------------------------------------------------------------------------------------
// DSL
// -----------------------------------------------------------------------------------------

/// `tfTypeType`: the `type` annotation every type-function parameter and helper carries.
pub fn tf_type_type(ast: &mut Ast) -> NodeId {
	ast.type_name("type")
}

pub fn tf_args(ast: &mut Ast, args: impl IntoIterator<Item = NodeId>) -> List<element::Expression> {
	ast.list_from(args)
}

pub fn tf_stmts(ast: &mut Ast, statements: impl IntoIterator<Item = NodeId>) -> Statements {
	ast.list_from(statements)
}

/// `tfTypes`: `types.name`.
pub fn tf_types(ast: &mut Ast, name: &str) -> NodeId {
	let types = ast.id("types");
	ast.property_access(types, name)
}

/// `tfTypesCall`: `types.name(args)`.
pub fn tf_types_call(ast: &mut Ast, name: &str, args: impl IntoIterator<Item = NodeId>) -> NodeId {
	let function = tf_types(ast, name);
	let args = tf_args(ast, args);
	ast.call(function, args)
}

/// `tfCall`: `fn(args)`.
pub fn tf_call(ast: &mut Ast, function: NodeId, args: impl IntoIterator<Item = NodeId>) -> NodeId {
	let args = tf_args(ast, args);
	ast.call(function, args)
}

/// `tfCall` on a local function by name.
pub fn tf_call_id(ast: &mut Ast, function: &str, args: impl IntoIterator<Item = NodeId>) -> NodeId {
	let function = ast.id(function);
	tf_call(ast, function, args)
}

/// `tfLibCall`: `lib.name(args)`.
pub fn tf_lib_call(ast: &mut Ast, lib: &str, name: &str, args: impl IntoIterator<Item = NodeId>) -> NodeId {
	let lib = ast.id(lib);
	let function = ast.property_access(lib, name);
	tf_call(ast, function, args)
}

/// `tfMethod`: `receiver:name(args)`.
pub fn tf_method(ast: &mut Ast, receiver: NodeId, name: &str, args: impl IntoIterator<Item = NodeId>) -> NodeId {
	let args = tf_args(ast, args);
	ast.method_call(name, receiver, args)
}

/// `tfIs`: `value:is("tag")`.
pub fn tf_is(ast: &mut Ast, value: NodeId, tag: &str) -> NodeId {
	let tag = ast.str(tag);
	tf_method(ast, value, "is", [tag])
}

/// `tfProp`: `receiver.name`.
pub fn tf_prop(ast: &mut Ast, receiver: NodeId, name: &str) -> NodeId {
	ast.property_access(receiver, name)
}

/// `tfLocal`: `local name = value`.
pub fn tf_local(ast: &mut Ast, name: &str, value: NodeId) -> NodeId {
	let id = ast.id(name);
	ast.variable_declaration(id, Some(value.into()))
}

/// `tfTypedLocal`: `local name: type`.
pub fn tf_typed_local(ast: &mut Ast, name: &str) -> NodeId {
	let id = ast.id(name);
	let ty = tf_type_type(ast);
	ast.set_binding_type(id, Some(ty));
	ast.variable_declaration(id, None)
}

/// `tfSet`: `target = value`.
pub fn tf_set(ast: &mut Ast, target: NodeId, value: NodeId) -> NodeId {
	ast.assignment(target, AssignmentOperator::Assign, value)
}

/// `tfReturn`
pub fn tf_return(ast: &mut Ast, value: NodeId) -> NodeId {
	ast.return_statement(value)
}

/// `tfIf`: `if condition then ... [else ...] end`; an if statement as `else_body` is an
/// `elseif` chain.
pub fn tf_if(ast: &mut Ast, condition: NodeId, then: Statements, else_body: Option<NodeOrList>) -> NodeId {
	ast.if_statement(condition, then, else_body)
}

/// `tfForIn`: `for names in iterator do ... end`.
pub fn tf_for_in(ast: &mut Ast, names: &[&str], iterator: NodeId, body: Statements) -> NodeId {
	let ids: Vec<NodeId> = names.iter().map(|&name| ast.id(name)).collect();
	let ids = ast.list_from::<element::AnyIdentifier>(ids);
	ast.for_statement(ids, iterator, body)
}

pub fn tf_not(ast: &mut Ast, expression: NodeId) -> NodeId {
	ast.unary(UnaryOperator::Not, expression)
}

pub fn tf_and(ast: &mut Ast, left: NodeId, right: NodeId) -> NodeId {
	ast.binary(left, BinaryOperator::And, right)
}

pub fn tf_or(ast: &mut Ast, left: NodeId, right: NodeId) -> NodeId {
	ast.binary(left, BinaryOperator::Or, right)
}

pub fn tf_eq(ast: &mut Ast, left: NodeId, right: NodeId) -> NodeId {
	ast.binary(left, BinaryOperator::Equal, right)
}

fn tf_concat(ast: &mut Ast, left: NodeId, right: NodeId) -> NodeId {
	ast.binary(left, BinaryOperator::Concat, right)
}

fn tf_len(ast: &mut Ast, expression: NodeId) -> NodeId {
	ast.unary(UnaryOperator::Length, expression)
}

/// `tfIndex`: `table[index]`.
fn tf_index(ast: &mut Ast, table: NodeId, index: NodeId) -> NodeId {
	ast.computed_index(table, index)
}

/// `tfError`: the statement `error("message")`.
pub fn tf_error(ast: &mut Ast, message: &str) -> NodeId {
	let message = ast.str(message);
	let call = tf_call_id(ast, "error", [message]);
	ast.call_statement(call)
}

/// `tfInsert`: the statement `table.insert(list, value)`.
pub fn tf_insert(ast: &mut Ast, list: NodeId, value: NodeId) -> NodeId {
	let call = tf_lib_call(ast, "table", "insert", [list, value]);
	ast.call_statement(call)
}

/// `tfCallStatement`
pub fn tf_call_statement(ast: &mut Ast, call: NodeId) -> NodeId {
	ast.call_statement(call)
}

/// `tfField`: the `name = value` field of a Lua table.
pub fn tf_field(ast: &mut Ast, name: &str, value: NodeId) -> NodeId {
	let key = ast.str(name);
	ast.map_field(key, value)
}

/// `tfMap`: a Lua table of named fields; empty renders `{}`.
pub fn tf_map(ast: &mut Ast, fields: impl IntoIterator<Item = NodeId>) -> NodeId {
	let fields = ast.list_from::<element::MapField>(fields);
	ast.map(fields)
}

/// `tfArray`: a Lua array literal.
pub fn tf_array(ast: &mut Ast, members: impl IntoIterator<Item = NodeId>) -> NodeId {
	let members = tf_args(ast, members);
	ast.array(members)
}

/// `tfIfExpr`: `if condition then a else b`.
pub fn tf_if_expr(ast: &mut Ast, condition: NodeId, a: NodeId, b: NodeId) -> NodeId {
	ast.if_expression(condition, a, b)
}

/// `tfLocalFunction`: `local function name(params): returns ... end`.
fn tf_local_function(
	ast: &mut Ast,
	name: &str,
	parameters: Vec<(&str, NodeId)>,
	returns: NodeId,
	body: Statements,
) -> NodeId {
	let ids = ast.new_list::<element::AnyIdentifier>();
	for (parameter, ty) in parameters {
		let id = ast.id(parameter);
		ast.set_binding_type(id, Some(ty));
		ast.list_push(ids, id);
	}
	let function_name = ast.id(name);
	let declaration = ast.function_declaration(true, function_name, ids, false, body);
	if let Some(payload) = ast.get_mut::<FunctionDeclaration>(declaration) {
		payload.return_type = Some(returns);
	}
	declaration
}

/// `tfTypeFunction`: `type function name(T: type, ...) ... end`.
pub fn tf_type_function(ast: &mut Ast, name: &str, parameters: &[String], body: Statements) -> NodeId {
	let ids = ast.new_list::<element::AnyIdentifier>();
	for parameter in parameters {
		let id = ast.id(parameter.as_str());
		let ty = tf_type_type(ast);
		ast.set_binding_type(id, Some(ty));
		ast.list_push(ids, id);
	}
	ast.type_function_declaration(name, ids, false, body)
}

// -----------------------------------------------------------------------------------------
// Helpers: local functions a body may need
// -----------------------------------------------------------------------------------------

/// `helperDistribute`
pub const HELPER_DISTRIBUTE: &str = "distribute";
/// `helperJoin`
pub const HELPER_JOIN: &str = "join";
/// `helperWithoutNil`
pub const HELPER_WITHOUT_NIL: &str = "withoutnil";
/// `helperKeys`
pub const HELPER_KEYS: &str = "keys";
/// `helperIndexOf`
pub const HELPER_INDEX_OF: &str = "indexof";
/// `helperTemplate`
pub const HELPER_TEMPLATE: &str = "template";
/// `helperReturnOf`
pub const HELPER_RETURN_OF: &str = "returnof";

/// `typeFunctionHelperOrder`: a helper is always declared before the helpers and statements
/// that call it.
pub const TYPE_FUNCTION_HELPER_ORDER: &[&str] = &[
	HELPER_DISTRIBUTE,
	HELPER_JOIN,
	HELPER_WITHOUT_NIL,
	HELPER_KEYS,
	HELPER_INDEX_OF,
	HELPER_TEMPLATE,
	HELPER_RETURN_OF,
];

/// `typeFunctionHelperDependencies`: the helpers a helper calls.
fn type_function_helper_dependencies(name: &str) -> &'static [&'static str] {
	match name {
		HELPER_WITHOUT_NIL => &[HELPER_DISTRIBUTE, HELPER_JOIN],
		HELPER_KEYS | HELPER_INDEX_OF => &[HELPER_JOIN],
		_ => &[],
	}
}

/// `typeFunctionHelperStatements`: the declarations of the requested helpers and everything
/// they depend on, in declaration order.
pub fn type_function_helper_statements(ast: &mut Ast, requested: &HashSet<&'static str>) -> Vec<NodeId> {
	fn require(name: &'static str, needed: &mut HashSet<&'static str>) {
		if !needed.insert(name) {
			return;
		}
		for &dependency in type_function_helper_dependencies(name) {
			require(dependency, needed);
		}
	}
	let mut needed = HashSet::new();
	for &name in requested {
		require(name, &mut needed);
	}
	TYPE_FUNCTION_HELPER_ORDER
		.iter()
		.filter(|name| needed.contains(*name))
		.map(|&name| type_function_helper(ast, name))
		.collect()
}

/// `typeFunctionHelper`: one helper by name.
#[expect(clippy::too_many_lines, reason = "one arm per helper, as in Go")]
fn type_function_helper(ast: &mut Ast, name: &str) -> NodeId {
	match name {
		HELPER_DISTRIBUTE => {
			// The members a conditional or a mapped key distributes over: a union's
			// components, nothing for never, the type itself otherwise.
			let value = ast.id("value");
			let parameter = tf_type_type(ast);
			let element = tf_type_type(ast);
			let returns = ast.type_array(element);
			let is_union = tf_is(ast, value, "union");
			let components = tf_method(ast, value, "components", []);
			let ret = tf_return(ast, components);
			let then = tf_stmts(ast, [ret]);
			let first = tf_if(ast, is_union, then, None);
			let is_never = tf_is(ast, value, "never");
			let empty = tf_map(ast, []);
			let ret = tf_return(ast, empty);
			let then = tf_stmts(ast, [ret]);
			let second = tf_if(ast, is_never, then, None);
			let array = tf_array(ast, [value]);
			let ret = tf_return(ast, array);
			let body = tf_stmts(ast, [first, second, ret]);
			tf_local_function(ast, name, vec![("value", parameter)], returns, body)
		}
		HELPER_JOIN => {
			// The union of a list of types; types.unionof needs two or more.
			let parts = ast.id("parts");
			let element = tf_type_type(ast);
			let type_list = ast.type_array(element);
			let returns = tf_type_type(ast);
			let length = tf_len(ast, parts);
			let zero = ast.num(0.0);
			let condition = tf_eq(ast, length, zero);
			let never = tf_types(ast, "never");
			let ret = tf_return(ast, never);
			let then = tf_stmts(ast, [ret]);
			let first = tf_if(ast, condition, then, None);
			let length = tf_len(ast, parts);
			let one = ast.num(1.0);
			let condition = tf_eq(ast, length, one);
			let one = ast.num(1.0);
			let index = tf_index(ast, parts, one);
			let ret = tf_return(ast, index);
			let then = tf_stmts(ast, [ret]);
			let second = tf_if(ast, condition, then, None);
			let unpack = tf_lib_call(ast, "table", "unpack", [parts]);
			let union = tf_types_call(ast, "unionof", [unpack]);
			let ret = tf_return(ast, union);
			let body = tf_stmts(ast, [first, second, ret]);
			tf_local_function(ast, name, vec![("parts", type_list)], returns, body)
		}
		HELPER_WITHOUT_NIL => {
			// `-?` and NonNullable: the type without its nil member.
			let value = ast.id("value");
			let parts = ast.id("parts");
			let component = ast.id("component");
			let parameter = tf_type_type(ast);
			let returns = tf_type_type(ast);
			let empty = tf_map(ast, []);
			let local = tf_local(ast, "parts", empty);
			let distribute = tf_call_id(ast, HELPER_DISTRIBUTE, [value]);
			let is_nil = tf_is(ast, component, "nil");
			let condition = tf_not(ast, is_nil);
			let insert = tf_insert(ast, parts, component);
			let then = tf_stmts(ast, [insert]);
			let check = tf_if(ast, condition, then, None);
			let loop_body = tf_stmts(ast, [check]);
			let for_in = tf_for_in(ast, &["_", "component"], distribute, loop_body);
			let join = tf_call_id(ast, HELPER_JOIN, [parts]);
			let ret = tf_return(ast, join);
			let body = tf_stmts(ast, [local, for_in, ret]);
			tf_local_function(ast, name, vec![("value", parameter)], returns, body)
		}
		HELPER_KEYS => {
			// `keyof T`: the union of a table's property keys.
			let value = ast.id("value");
			let parts = ast.id("parts");
			let key = ast.id("key");
			let parameter = tf_type_type(ast);
			let returns = tf_type_type(ast);
			let is_table = tf_is(ast, value, "table");
			let condition = tf_not(ast, is_table);
			let error = tf_error(ast, "keyof: expected a table type");
			let then = tf_stmts(ast, [error]);
			let check = tf_if(ast, condition, then, None);
			let empty = tf_map(ast, []);
			let local = tf_local(ast, "parts", empty);
			let properties = tf_method(ast, value, "properties", []);
			let insert = tf_insert(ast, parts, key);
			let loop_body = tf_stmts(ast, [insert]);
			let for_in = tf_for_in(ast, &["key"], properties, loop_body);
			let join = tf_call_id(ast, HELPER_JOIN, [parts]);
			let ret = tf_return(ast, join);
			let body = tf_stmts(ast, [check, local, for_in, ret]);
			tf_local_function(ast, name, vec![("value", parameter)], returns, body)
		}
		HELPER_INDEX_OF => {
			// `T[K]`: the property's read type, distributed over a union key.
			let object = ast.id("object");
			let key = ast.id("key");
			let parts = ast.id("parts");
			let component = ast.id("component");
			let value = ast.id("value");
			let object_type = tf_type_type(ast);
			let key_type = tf_type_type(ast);
			let returns = tf_type_type(ast);
			let is_union = tf_is(ast, key, "union");
			let empty = tf_map(ast, []);
			let local_parts = tf_local(ast, "parts", empty);
			let components = tf_method(ast, key, "components", []);
			let recurse = tf_call_id(ast, HELPER_INDEX_OF, [object, component]);
			let insert = tf_insert(ast, parts, recurse);
			let loop_body = tf_stmts(ast, [insert]);
			let for_in = tf_for_in(ast, &["_", "component"], components, loop_body);
			let join = tf_call_id(ast, HELPER_JOIN, [parts]);
			let ret = tf_return(ast, join);
			let then = tf_stmts(ast, [local_parts, for_in, ret]);
			let first = tf_if(ast, is_union, then, None);
			let is_table = tf_is(ast, object, "table");
			let condition = tf_not(ast, is_table);
			let error = tf_error(ast, "indexed access: expected a table type");
			let then = tf_stmts(ast, [error]);
			let second = tf_if(ast, condition, then, None);
			let read = tf_method(ast, object, "readproperty", [key]);
			let local_value = tf_local(ast, "value", read);
			let condition = tf_not(ast, value);
			let error = tf_error(ast, "indexed access: the property does not exist");
			let then = tf_stmts(ast, [error]);
			let third = tf_if(ast, condition, then, None);
			let ret = tf_return(ast, value);
			let body = tf_stmts(ast, [first, second, local_value, third, ret]);
			tf_local_function(
				ast,
				name,
				vec![("object", object_type), ("key", key_type)],
				returns,
				body,
			)
		}
		HELPER_TEMPLATE => {
			// A template literal: the singleton of the concatenated parts, or `string` once a
			// hole is not a singleton.
			let parts = ast.id("parts");
			let text = ast.id("text");
			let part = ast.id("part");
			let string = ast.type_name(NATIVE_TYPE_STRING);
			let type_type = tf_type_type(ast);
			let members = ast.list_from::<element::TypeNode>([string, type_type]);
			let union = ast.type_union(members);
			let part_type = ast.type_array(union);
			let returns = tf_type_type(ast);
			let empty = ast.str("");
			let local_text = tf_local(ast, "text", empty);
			let type_call = tf_call_id(ast, "type", [part]);
			let string_tag = ast.str("string");
			let condition = tf_eq(ast, type_call, string_tag);
			let concat = tf_concat(ast, text, part);
			let set = tf_set(ast, text, concat);
			let then = tf_stmts(ast, [set]);
			let is_singleton = tf_is(ast, part, "singleton");
			let value = tf_method(ast, part, "value", []);
			let tostring = tf_call_id(ast, "tostring", [value]);
			let concat = tf_concat(ast, text, tostring);
			let set = tf_set(ast, text, concat);
			let singleton_then = tf_stmts(ast, [set]);
			let string_type = tf_types(ast, "string");
			let ret = tf_return(ast, string_type);
			let otherwise = tf_stmts(ast, [ret]);
			let elseif = tf_if(ast, is_singleton, singleton_then, Some(otherwise.into()));
			let check = tf_if(ast, condition, then, Some(elseif.into()));
			let loop_body = tf_stmts(ast, [check]);
			let for_in = tf_for_in(ast, &["_", "part"], parts, loop_body);
			let singleton = tf_types_call(ast, "singleton", [text]);
			let ret = tf_return(ast, singleton);
			let body = tf_stmts(ast, [local_text, for_in, ret]);
			tf_local_function(ast, name, vec![("parts", part_type)], returns, body)
		}
		_ => {
			// HELPER_RETURN_OF: `T extends (...args: any) => infer R`: the first return type,
			// the variadic return, or nil for a function that returns nothing.
			let function = ast.id("fn");
			let returns_id = ast.id("returns");
			let parameter = tf_type_type(ast);
			let returns = tf_type_type(ast);
			let method = tf_method(ast, function, "returns", []);
			let local = tf_local(ast, "returns", method);
			let head = tf_prop(ast, returns_id, "head");
			let head_again = tf_prop(ast, returns_id, "head");
			let one = ast.num(1.0);
			let first = tf_index(ast, head_again, one);
			let condition = tf_and(ast, head, first);
			let head_value = tf_prop(ast, returns_id, "head");
			let one = ast.num(1.0);
			let first_value = tf_index(ast, head_value, one);
			let ret = tf_return(ast, first_value);
			let then = tf_stmts(ast, [ret]);
			let check_head = tf_if(ast, condition, then, None);
			let tail = tf_prop(ast, returns_id, "tail");
			let tail_value = tf_prop(ast, returns_id, "tail");
			let ret = tf_return(ast, tail_value);
			let then = tf_stmts(ast, [ret]);
			let check_tail = tf_if(ast, tail, then, None);
			let nil = ast.nil();
			let singleton = tf_types_call(ast, "singleton", [nil]);
			let ret = tf_return(ast, singleton);
			let body = tf_stmts(ast, [local, check_head, check_tail, ret]);
			tf_local_function(ast, name, vec![("fn", parameter)], returns, body)
		}
	}
}

// -----------------------------------------------------------------------------------------
// Prelude
// -----------------------------------------------------------------------------------------

/// `luauTypesPreludeNames`: the TypeScript built-in helper types that have a prelude type
/// function.
pub const LUAU_TYPES_PRELUDE_NAMES: &[&str] = &[
	"Partial",
	"Required",
	"Readonly",
	"Pick",
	"Omit",
	"Record",
	"Exclude",
	"Extract",
	"NonNullable",
	"ReturnType",
	"Parameters",
];

/// `luauTypesPreludeTableNames`: the prelude functions whose result is a table when their input
/// is.
const LUAU_TYPES_PRELUDE_TABLE_NAMES: &[&str] = &[
	"Partial",
	"Required",
	"Readonly",
	"Pick",
	"Omit",
	"Record",
	"Parameters",
];

/// How the homomorphic copy rewrites one property type.
#[derive(Clone, Copy)]
enum CopyRewrite {
	Optional,
	WithoutNil,
}

/// `luauTypesPrelude`: the prelude type function for a TS helper type, or `None` for a name
/// without one.
#[expect(clippy::too_many_lines, reason = "one arm per helper type, as in Go")]
fn luau_types_prelude(ast: &mut Ast, name: &str) -> Option<NodeId> {
	if !LUAU_TYPES_PRELUDE_NAMES.contains(&name) {
		return None;
	}
	let t = ast.id("T");
	let k = ast.id("K");
	let u = ast.id("U");
	let key = ast.id("key");
	let prop = ast.id("prop");
	let result = ast.id("result");
	let current = ast.id("current");
	let parts = ast.id("parts");
	let item = ast.id("item");
	let mut helpers: HashSet<&'static str> = HashSet::new();
	let mut parameters: Vec<String> = Vec::new();
	let mut body: Vec<NodeId> = Vec::new();

	let copy_rewrite = |ast: &mut Ast, parameters: &mut Vec<String>, body: &mut Vec<NodeId>, rewrite: CopyRewrite| {
		*parameters = vec!["T".to_owned()];
		let apply = |ast: &mut Ast, current: NodeId| match rewrite {
			CopyRewrite::Optional => tf_types_call(ast, "optional", [current]),
			CopyRewrite::WithoutNil => tf_call_id(ast, HELPER_WITHOUT_NIL, [current]),
		};
		let is_table = tf_is(ast, t, "table");
		let condition = tf_not(ast, is_table);
		let ret = tf_return(ast, t);
		let then = tf_stmts(ast, [ret]);
		let guard = tf_if(ast, condition, then, None);
		let copy = tf_types_call(ast, "copy", [t]);
		let local = tf_local(ast, "result", copy);
		let properties = tf_method(ast, t, "properties", []);
		let read = tf_prop(ast, prop, "read");
		let read_value = tf_prop(ast, prop, "read");
		let rewritten = apply(ast, read_value);
		let set_read = tf_method(ast, result, "setreadproperty", [key, rewritten]);
		let set_read = tf_call_statement(ast, set_read);
		let then = tf_stmts(ast, [set_read]);
		let read_check = tf_if(ast, read, then, None);
		let write = tf_prop(ast, prop, "write");
		let write_value = tf_prop(ast, prop, "write");
		let rewritten = apply(ast, write_value);
		let set_write = tf_method(ast, result, "setwriteproperty", [key, rewritten]);
		let set_write = tf_call_statement(ast, set_write);
		let then = tf_stmts(ast, [set_write]);
		let write_check = tf_if(ast, write, then, None);
		let loop_body = tf_stmts(ast, [read_check, write_check]);
		let for_in = tf_for_in(ast, &["key", "prop"], properties, loop_body);
		let ret = tf_return(ast, result);
		body.extend([guard, local, for_in, ret]);
	};

	let pick_rewrite = |ast: &mut Ast, parameters: &mut Vec<String>, body: &mut Vec<NodeId>, negate: bool| {
		*parameters = vec!["T".to_owned(), "K".to_owned()];
		let is_table = tf_is(ast, t, "table");
		let condition = tf_not(ast, is_table);
		let error = tf_error(ast, &format!("{name}<T, K>: expected a table type"));
		let then = tf_stmts(ast, [error]);
		let guard = tf_if(ast, condition, then, None);
		let newtable = tf_types_call(ast, "newtable", []);
		let local = tf_local(ast, "result", newtable);
		let properties = tf_method(ast, t, "properties", []);
		let keep = tf_method(ast, key, "issubtypeof", [k]);
		let keep = if negate { tf_not(ast, keep) } else { keep };
		let read = tf_prop(ast, prop, "read");
		let read_value = tf_prop(ast, prop, "read");
		let set_read = tf_method(ast, result, "setreadproperty", [key, read_value]);
		let set_read = tf_call_statement(ast, set_read);
		let then = tf_stmts(ast, [set_read]);
		let read_check = tf_if(ast, read, then, None);
		let write = tf_prop(ast, prop, "write");
		let write_value = tf_prop(ast, prop, "write");
		let set_write = tf_method(ast, result, "setwriteproperty", [key, write_value]);
		let set_write = tf_call_statement(ast, set_write);
		let then = tf_stmts(ast, [set_write]);
		let write_check = tf_if(ast, write, then, None);
		let kept = tf_stmts(ast, [read_check, write_check]);
		let keep_check = tf_if(ast, keep, kept, None);
		let loop_body = tf_stmts(ast, [keep_check]);
		let for_in = tf_for_in(ast, &["key", "prop"], properties, loop_body);
		let ret = tf_return(ast, result);
		body.extend([guard, local, for_in, ret]);
	};

	let filter_rewrite = |ast: &mut Ast,
	                      parameters: &mut Vec<String>,
	                      body: &mut Vec<NodeId>,
	                      helpers: &mut HashSet<&'static str>,
	                      negate: bool| {
		*parameters = vec!["T".to_owned(), "U".to_owned()];
		helpers.insert(HELPER_DISTRIBUTE);
		helpers.insert(HELPER_JOIN);
		let empty = tf_map(ast, []);
		let local = tf_local(ast, "parts", empty);
		let distribute = tf_call_id(ast, HELPER_DISTRIBUTE, [t]);
		let keep = tf_method(ast, item, "issubtypeof", [u]);
		let keep = if negate { tf_not(ast, keep) } else { keep };
		let insert = tf_insert(ast, parts, item);
		let then = tf_stmts(ast, [insert]);
		let check = tf_if(ast, keep, then, None);
		let loop_body = tf_stmts(ast, [check]);
		let for_in = tf_for_in(ast, &["_", "item"], distribute, loop_body);
		let join = tf_call_id(ast, HELPER_JOIN, [parts]);
		let ret = tf_return(ast, join);
		body.extend([local, for_in, ret]);
	};

	match name {
		"Partial" => copy_rewrite(ast, &mut parameters, &mut body, CopyRewrite::Optional),
		"Required" => {
			helpers.insert(HELPER_WITHOUT_NIL);
			copy_rewrite(ast, &mut parameters, &mut body, CopyRewrite::WithoutNil);
		}
		"Readonly" => {
			parameters = vec!["T".to_owned()];
			let is_table = tf_is(ast, t, "table");
			let condition = tf_not(ast, is_table);
			let ret = tf_return(ast, t);
			let then = tf_stmts(ast, [ret]);
			let guard = tf_if(ast, condition, then, None);
			let copy = tf_types_call(ast, "copy", [t]);
			let local = tf_local(ast, "result", copy);
			let properties = tf_method(ast, t, "properties", []);
			let read = tf_prop(ast, prop, "read");
			let write = tf_prop(ast, prop, "write");
			let either = tf_or(ast, read, write);
			let local_current = tf_local(ast, "current", either);
			let set_read = tf_method(ast, result, "setreadproperty", [key, current]);
			let set_read = tf_call_statement(ast, set_read);
			let nil = ast.nil();
			let set_write = tf_method(ast, result, "setwriteproperty", [key, nil]);
			let set_write = tf_call_statement(ast, set_write);
			let then = tf_stmts(ast, [set_read, set_write]);
			let check = tf_if(ast, current, then, None);
			let loop_body = tf_stmts(ast, [local_current, check]);
			let for_in = tf_for_in(ast, &["key", "prop"], properties, loop_body);
			let ret = tf_return(ast, result);
			body.extend([guard, local, for_in, ret]);
		}
		"Pick" => pick_rewrite(ast, &mut parameters, &mut body, false),
		"Omit" => pick_rewrite(ast, &mut parameters, &mut body, true),
		"Record" => {
			parameters = vec!["K".to_owned(), "T".to_owned()];
			helpers.insert(HELPER_DISTRIBUTE);
			let newtable = tf_types_call(ast, "newtable", []);
			let local = tf_local(ast, "result", newtable);
			let distribute = tf_call_id(ast, HELPER_DISTRIBUTE, [k]);
			let is_singleton = tf_is(ast, key, "singleton");
			let set_property = tf_method(ast, result, "setproperty", [key, t]);
			let set_property = tf_call_statement(ast, set_property);
			let singleton_then = tf_stmts(ast, [set_property]);
			let is_string = tf_is(ast, key, "string");
			let is_number = tf_is(ast, key, "number");
			let either = tf_or(ast, is_string, is_number);
			let set_indexer = tf_method(ast, result, "setindexer", [key, t]);
			let set_indexer = tf_call_statement(ast, set_indexer);
			let indexer_then = tf_stmts(ast, [set_indexer]);
			let is_never = tf_is(ast, key, "never");
			let not_never = tf_not(ast, is_never);
			let error = tf_error(
				ast,
				"Record<K, T>: a mapped key must be a string literal, string or number",
			);
			let error_then = tf_stmts(ast, [error]);
			let never_check = tf_if(ast, not_never, error_then, None);
			let indexer_check = tf_if(ast, either, indexer_then, Some(never_check.into()));
			let check = tf_if(ast, is_singleton, singleton_then, Some(indexer_check.into()));
			let loop_body = tf_stmts(ast, [check]);
			let for_in = tf_for_in(ast, &["_", "key"], distribute, loop_body);
			let ret = tf_return(ast, result);
			body.extend([local, for_in, ret]);
		}
		"Exclude" => filter_rewrite(ast, &mut parameters, &mut body, &mut helpers, true),
		"Extract" => filter_rewrite(ast, &mut parameters, &mut body, &mut helpers, false),
		"NonNullable" => {
			parameters = vec!["T".to_owned()];
			helpers.insert(HELPER_WITHOUT_NIL);
			let call = tf_call_id(ast, HELPER_WITHOUT_NIL, [t]);
			let ret = tf_return(ast, call);
			body.push(ret);
		}
		"ReturnType" => {
			parameters = vec!["T".to_owned()];
			helpers.insert(HELPER_RETURN_OF);
			let is_any = tf_is(ast, t, "any");
			let any = tf_types(ast, "any");
			let ret = tf_return(ast, any);
			let then = tf_stmts(ast, [ret]);
			let any_check = tf_if(ast, is_any, then, None);
			let is_function = tf_is(ast, t, "function");
			let condition = tf_not(ast, is_function);
			let error = tf_error(ast, "ReturnType<T>: expected a function type");
			let then = tf_stmts(ast, [error]);
			let function_check = tf_if(ast, condition, then, None);
			let call = tf_call_id(ast, HELPER_RETURN_OF, [t]);
			let ret = tf_return(ast, call);
			body.extend([any_check, function_check, ret]);
		}
		_ => {
			// "Parameters": a tuple has no Luau spelling, so the parameters become an array of
			// their union, as the builder spells every tuple.
			parameters = vec!["T".to_owned()];
			helpers.insert(HELPER_JOIN);
			let signature = ast.id("parameters");
			let parameter = ast.id("parameter");
			let element = ast.id("element");
			let is_function = tf_is(ast, t, "function");
			let condition = tf_not(ast, is_function);
			let error = tf_error(ast, "Parameters<T>: expected a function type");
			let then = tf_stmts(ast, [error]);
			let guard = tf_if(ast, condition, then, None);
			let method = tf_method(ast, t, "parameters", []);
			let local_parameters = tf_local(ast, "parameters", method);
			let empty = tf_map(ast, []);
			let local_parts = tf_local(ast, "parts", empty);
			let head = tf_prop(ast, signature, "head");
			let empty = tf_map(ast, []);
			let iterator = tf_or(ast, head, empty);
			let insert = tf_insert(ast, parts, parameter);
			let loop_body = tf_stmts(ast, [insert]);
			let for_in = tf_for_in(ast, &["_", "parameter"], iterator, loop_body);
			let tail = tf_prop(ast, signature, "tail");
			let tail_value = tf_prop(ast, signature, "tail");
			let insert = tf_insert(ast, parts, tail_value);
			let then = tf_stmts(ast, [insert]);
			let tail_check = tf_if(ast, tail, then, None);
			let join = tf_call_id(ast, HELPER_JOIN, [parts]);
			let local_element = tf_local(ast, "element", join);
			let nil = ast.nil();
			let number = tf_types(ast, "number");
			let index = tf_field(ast, "index", number);
			let read = tf_field(ast, "readresult", element);
			let write = tf_field(ast, "writeresult", element);
			let fields = tf_map(ast, [index, read, write]);
			let newtable = tf_types_call(ast, "newtable", [nil, fields]);
			let ret = tf_return(ast, newtable);
			body.extend([
				guard,
				local_parameters,
				local_parts,
				for_in,
				tail_check,
				local_element,
				ret,
			]);
		}
	}
	let helper_statements = type_function_helper_statements(ast, &helpers);
	let statements = tf_stmts(ast, helper_statements);
	let rest = tf_stmts(ast, body);
	ast.list_push_list(statements, rest);
	Some(tf_type_function(ast, &format!("__{name}"), &parameters, statements))
}

/// `requirePrelude`: registers the prelude type function for a TS helper type once per file
/// (hoisted, never exported) and returns the Luau name a generic use refers to, or `""` for a
/// name without a prelude.
pub fn require_prelude(state: &mut State<'_>, name: &str) -> String {
	let mut builder = types(state);
	if let Some(&entry) = builder.b().preludes.get(name) {
		return builder.b().entry(entry).name.clone();
	}
	let Some(declaration) = luau_types_prelude(builder.ast(), name) else {
		return String::new();
	};
	let base = builder
		.ast()
		.get::<TypeFunctionDeclaration>(declaration)
		.map(|declaration| declaration.name.clone())
		.unwrap_or_default();
	let unique = builder.unique_name(&base);
	let parameters = builder
		.ast()
		.get::<TypeFunctionDeclaration>(declaration)
		.map(|payload| payload.parameters);
	if let Some(payload) = builder.ast().get_mut::<TypeFunctionDeclaration>(declaration) {
		payload.name.clone_from(&unique);
	}
	let symbol = builder.b().synthetic_symbol();
	let mut entry = TypeAliasEntry::new(symbol, unique.clone());
	entry.type_function = Some(declaration);
	entry.kind = if LUAU_TYPES_PRELUDE_TABLE_NAMES.contains(&name) {
		NativeTypeKind::Table
	} else {
		NativeTypeKind::Unspellable
	};
	entry.placement = AliasPlacement::Hoisted;
	if let Some(parameters) = parameters {
		for parameter in builder.ast().list_to_vec(parameters) {
			if let Some(id) = builder.ast().get::<Identifier>(parameter) {
				entry.generics.push(TypeGeneric::new(id.name.clone()));
			}
		}
	}
	let entry = builder.b().push_entry(entry);
	builder.b().preludes.insert(name.to_owned(), entry);
	builder.b().aliases.insert(symbol, Some(entry));
	builder.b().alias_order.push(entry);
	unique
}
