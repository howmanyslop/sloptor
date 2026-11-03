//! `nativetypes_policy_test.go`: the native policy on hand-built trees. Every expected spelling
//! is a line the `//!native` fixtures pin or a rule of the policy written out as literal Luau.

use std::collections::HashMap;

use rotor_luau::{TypeAccess, TypeArray, TypeFunctionParameter};
use rotor_tsrs::ast::SymbolFlags;

use super::super::luautypes::{SymKey, clone_type, render_type_text};
use super::super::nativetypes::{NativePosition, NativeTypeKind as K};
use super::super::nativetypes_policy::{NativeOrigins, apply_native_policy};
use crate::prelude::*;

/// `fakeNativeOrigins`: which symbol a node came from, what a named alias stands for, and which
/// generics are the annotated function's own.
#[derive(Default)]
struct Fake {
	origins: HashMap<NodeId, SymKey>,
	kinds: HashMap<SymKey, K>,
	own: HashMap<String, bool>,
	symbols: u32,
}

impl NativeOrigins for Fake {
	fn origin_of(&self, node: NodeId) -> Option<SymKey> {
		self.origins.get(&node).copied()
	}

	fn alias_kind_of(&self, symbol: SymKey) -> Option<K> {
		self.kinds.get(&symbol).copied()
	}

	fn own_generic(&self, name: &str) -> bool {
		self.own.get(name).copied().unwrap_or(false)
	}

	fn transfer_origin(&mut self, from: NodeId, to: NodeId) {
		if let Some(&symbol) = self.origins.get(&from) {
			self.origins.insert(to, symbol);
		}
	}
}

/// The arena and the fake origins a case builds into.
#[derive(Default)]
struct Env {
	ast: Ast,
	f: Fake,
}

impl Env {
	/// `&ast.Symbol{Flags: flags}`
	fn symbol(&mut self, flags: SymbolFlags) -> SymKey {
		self.f.symbols += 1;
		SymKey::Synthetic(self.f.symbols, flags)
	}

	/// `from`: marks `node` as spelled from a fresh symbol with `flags`.
	fn from(&mut self, node: NodeId, flags: SymbolFlags) -> NodeId {
		let symbol = self.symbol(flags);
		self.f.origins.insert(node, symbol);
		node
	}

	fn tn(&mut self, name: &str) -> NodeId {
		self.ast.type_name(name)
	}

	/// `alias`: a reference to a `type` statement of the given kind.
	fn alias(&mut self, name: &str, kind: K, arguments: Vec<NodeId>) -> NodeId {
		let symbol = self.symbol(SymbolFlags::Interface);
		self.f.kinds.insert(symbol, kind);
		let list = (!arguments.is_empty()).then(|| self.ast.list_from::<element::TypeNode>(arguments));
		let reference = self.ast.type_reference(name, list);
		self.f.origins.insert(reference, symbol);
		reference
	}

	fn generic(&mut self, name: &str, own: bool) -> NodeId {
		self.f.own.insert(name.to_owned(), own);
		let node = self.tn(name);
		self.from(node, SymbolFlags::TypeParameter)
	}

	fn roblox(&mut self, name: &str) -> NodeId {
		let node = self.tn(name);
		self.from(node, SymbolFlags::Interface)
	}

	fn class(&mut self, name: &str) -> NodeId {
		let node = self.tn(name);
		self.from(node, SymbolFlags::Class)
	}

	fn union(&mut self, members: Vec<NodeId>) -> NodeId {
		let list = self.ast.list_from::<element::TypeNode>(members);
		self.ast.type_union(list)
	}

	fn intersection(&mut self, members: Vec<NodeId>) -> NodeId {
		let list = self.ast.list_from::<element::TypeNode>(members);
		self.ast.type_intersection(list)
	}

	fn table(&mut self, fields: Vec<NodeId>) -> NodeId {
		let list = self.ast.list_from::<element::TypeTableField>(fields);
		self.ast.type_table(list)
	}

	fn prop(&mut self, access: TypeAccess, name: &str, value: NodeId) -> NodeId {
		self.ast.type_table_prop(access, name, value)
	}

	fn array(&mut self, element: &str, access: TypeAccess) -> NodeId {
		let element = self.tn(element);
		let array = self.ast.type_array(element);
		if let Some(payload) = self.ast.get_mut::<TypeArray>(array) {
			payload.access = access;
		}
		array
	}

	fn function(&mut self, parameters: Vec<(&str, NodeId)>, var_args: Option<NodeId>, ret: NodeId) -> NodeId {
		let parameters = parameters
			.into_iter()
			.map(|(name, ty)| TypeFunctionParameter::new(name, ty))
			.collect();
		self.ast.type_function(Vec::new(), parameters, var_args, ret)
	}

	/// `nestedTables`: `{ a: { a: ... { a: leaf } } }` with the given number of levels.
	fn nested_tables(&mut self, levels: usize, leaf: &str) -> NodeId {
		let mut node = self.tn(leaf);
		for _ in 0..levels {
			let prop = self.prop(TypeAccess::None, "a", node);
			node = self.table(vec![prop]);
		}
		node
	}
}

/// `policyCase`; `want` is the rendered result, `""` when the annotation must be dropped.
struct Case {
	name: &'static str,
	build: fn(&mut Env) -> NodeId,
	position: NativePosition,
	want: &'static str,
	kind: K,
	outer: bool,
}

const fn case(
	name: &'static str,
	build: fn(&mut Env) -> NodeId,
	position: NativePosition,
	want: &'static str,
	kind: K,
) -> Case {
	Case {
		name,
		build,
		position,
		want,
		kind,
		outer: false,
	}
}

/// `runPolicyCases`
fn run_policy_cases(cases: &[Case]) {
	let mut failures = Vec::new();
	for case in cases {
		let mut env = Env::default();
		let node = (case.build)(&mut env);
		let (result, kind, outer) = apply_native_policy(&mut env.ast, &mut env.f, node, case.position, 0);
		let got = result.map_or_else(String::new, |node| render_type_text(&env.ast, node));
		if got != case.want {
			failures.push(format!("{}: spelling = {got:?}, want {:?}", case.name, case.want));
		}
		if kind != case.kind {
			failures.push(format!("{}: kind = {kind:?}, want {:?}", case.name, case.kind));
		}
		if outer != case.outer {
			failures.push(format!("{}: outer generic = {outer}, want {}", case.name, case.outer));
		}
		let Some(result) = result else {
			continue;
		};
		// The spelling hook probes a constraint with the policy before the parameter pass runs
		// over the whole annotation, so a second pass must change nothing.
		let (again, again_kind, _) = apply_native_policy(&mut env.ast, &mut env.f, result, case.position, 0);
		let again_text = again.map_or_else(String::new, |node| render_type_text(&env.ast, node));
		if again.is_none() || again_text != got || again_kind != kind {
			failures.push(format!("{}: second pass changed the result: {again_text:?}", case.name));
		}
	}
	assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const PARAMETER: NativePosition = NativePosition::Parameter;
const RETURN: NativePosition = NativePosition::Return;
const NESTED: NativePosition = NativePosition::Nested;

/// A parameter annotated with a type Luau tags as something other than a table, vector, string
/// or number: the entry guard then fails or buys nothing.
#[test]
#[expect(clippy::too_many_lines, reason = "the Go case table, kept whole")]
fn native_policy_parameter_tags() {
	run_policy_cases(&[
		case(
			"mixed-kind union is dropped",
			|e| {
				let number = e.tn("number");
				let string = e.tn("string");
				e.union(vec![number, string])
			},
			PARAMETER,
			"",
			K::Unspellable,
		),
		case(
			"same-kind table union keeps its tag",
			|e| {
				let a = e.array("string", TypeAccess::None);
				let b = e.array("number", TypeAccess::None);
				e.union(vec![a, b])
			},
			PARAMETER,
			"{ string } | { number }",
			K::Table,
		),
		case(
			"optional table union keeps its tag",
			|e| {
				let a = e.array("string", TypeAccess::None);
				let b = e.array("number", TypeAccess::None);
				let union = e.union(vec![a, b]);
				e.ast.optional_type(union)
			},
			PARAMETER,
			"({ string } | { number })?",
			K::Table,
		),
		case(
			"both vector names share a tag",
			|e| {
				let a = e.roblox("Vector3");
				let b = e.roblox("vector");
				e.union(vec![a, b])
			},
			PARAMETER,
			"Vector3 | vector",
			K::Vector,
		),
		case(
			"string singletons share the string tag",
			|e| {
				let x = e.ast.type_string_singleton("x");
				let y = e.ast.type_string_singleton("y");
				e.union(vec![x, y])
			},
			PARAMETER,
			r#""x" | "y""#,
			K::String,
		),
		case("boolean is dropped", |e| e.tn("boolean"), PARAMETER, "", K::Boolean),
		case(
			"optional boolean literal is dropped",
			|e| {
				let value = e.ast.type_boolean_singleton(true);
				e.ast.optional_type(value)
			},
			PARAMETER,
			"",
			K::Boolean,
		),
		case("nil is dropped", |e| e.tn("nil"), PARAMETER, "", K::Unspellable),
		case("unknown is dropped", |e| e.tn("unknown"), PARAMETER, "", K::Unspellable),
		case(
			"an Instance subclass is dropped",
			|e| e.roblox("Part"),
			PARAMETER,
			"",
			K::Unspellable,
		),
		case(
			"a vector is kept",
			|e| e.roblox("vector"),
			PARAMETER,
			"vector",
			K::Vector,
		),
		case(
			"a class instance is dropped",
			|e| e.class("Counter"),
			PARAMETER,
			"",
			K::Unspellable,
		),
		case(
			"an empty table is dropped",
			|e| e.table(Vec::new()),
			PARAMETER,
			"",
			K::Unspellable,
		),
		case(
			"a table alias is kept",
			|e| e.alias("Foo", K::Table, Vec::new()),
			PARAMETER,
			"Foo",
			K::Table,
		),
		case(
			"a function alias is kept",
			|e| e.alias("Notify", K::Function, Vec::new()),
			PARAMETER,
			"Notify",
			K::Function,
		),
		case(
			"a table alias and a function alias mix",
			|e| {
				let foo = e.alias("Foo", K::Table, Vec::new());
				let notify = e.alias("Notify", K::Function, Vec::new());
				e.union(vec![foo, notify])
			},
			PARAMETER,
			"",
			K::Unspellable,
		),
	]);
}

/// A parameter annotated with an enclosing function's generic, which Luau tags as userdata so
/// the native entry guard fails on every call (rule R1).
#[test]
fn native_policy_outer_generics() {
	run_policy_cases(&[
		case(
			"own generic at a parameter",
			|e| e.generic("K", true),
			PARAMETER,
			"K",
			K::Generic,
		),
		Case {
			outer: true,
			..case(
				"outer generic at a parameter",
				|e| e.generic("K", false),
				PARAMETER,
				"",
				K::Generic,
			)
		},
		Case {
			outer: true,
			..case(
				"outer generic behind an optional",
				|e| {
					let generic = e.generic("K", false);
					e.ast.optional_type(generic)
				},
				PARAMETER,
				"",
				K::Generic,
			)
		},
		case(
			"outer generic as an alias argument",
			|e| {
				let k = e.generic("K", false);
				let v = e.generic("V", false);
				e.alias("Node", K::Table, vec![k, v])
			},
			PARAMETER,
			"Node<K, V>",
			K::Table,
		),
		case(
			"outer generic at a return",
			|e| e.generic("V", false),
			RETURN,
			"V",
			K::Generic,
		),
	]);
}

/// A declared return that claims a value the body cannot produce or that Luau cannot spell as
/// a pack.
#[test]
fn native_policy_returns() {
	run_policy_cases(&[
		case("nil becomes the empty pack", |e| e.tn("nil"), RETURN, "()", K::Nil),
		case(
			"an unknown pack stays unannotated",
			|e| {
				let any = e.tn("any");
				e.ast.type_variadic(any)
			},
			RETURN,
			"",
			K::Unspellable,
		),
		case(
			"a class instance stays unannotated",
			|e| e.class("Counter"),
			RETURN,
			"",
			K::Unspellable,
		),
		case(
			"a union with an unspellable member stays unannotated",
			|e| {
				let number = e.tn("number");
				let never = e.tn("never");
				e.union(vec![number, never])
			},
			RETURN,
			"",
			K::Unspellable,
		),
		case(
			"a mixed union is spelled",
			|e| {
				let number = e.tn("number");
				let string = e.tn("string");
				e.union(vec![number, string])
			},
			RETURN,
			"number | string",
			K::Number,
		),
		case("boolean is spelled", |e| e.tn("boolean"), RETURN, "boolean", K::Boolean),
		case(
			"an Instance is spelled by name",
			|e| e.roblox("Instance"),
			RETURN,
			"Instance",
			K::Vector,
		),
		case(
			"pack elements are nested slots",
			|e| {
				let number = e.tn("number");
				let class = e.class("Counter");
				let optional = e.ast.optional_type(class);
				let string = e.tn("string");
				let variadic = e.ast.type_variadic(string);
				let elements = e.ast.list_from::<element::TypeNode>([number, optional, variadic]);
				e.ast.type_pack(elements)
			},
			RETURN,
			"(number, any, ...string)",
			K::Unspellable,
		),
	]);
}

fn overloads(e: &mut Env) -> NodeId {
	let receiver = e.alias("OverloadedMethod", K::Table, Vec::new());
	let number = e.tn("number");
	let string = e.tn("string");
	let first = e.function(vec![("self", receiver), ("value", number)], None, string);
	let receiver = clone_type(&mut e.ast, receiver);
	let string = e.tn("string");
	let number = e.tn("number");
	let second = e.function(vec![("self", receiver), ("value", string)], None, number);
	e.intersection(vec![first, second])
}

fn table_intersection(e: &mut Env) -> NodeId {
	let number = e.tn("number");
	let a = e.prop(TypeAccess::None, "a", number);
	let a = e.table(vec![a]);
	let string = e.tn("string");
	let b = e.prop(TypeAccess::None, "b", string);
	let b = e.table(vec![b]);
	e.intersection(vec![a, b])
}

/// An overload set annotated where it hurts: at a parameter `(...any) -> ...any` wins;
/// everywhere else the overloads are kept.
#[test]
fn native_policy_overloads() {
	run_policy_cases(&[
		case(
			"overloads fall back at a parameter",
			overloads,
			PARAMETER,
			"(...any) -> ...any",
			K::Function,
		),
		case(
			"overloads are kept nested, with the receiver widened",
			overloads,
			NESTED,
			"((self: any, value: number) -> string) & ((self: any, value: string) -> number)",
			K::Function,
		),
		case(
			"overloads are kept at a return",
			overloads,
			RETURN,
			"((self: any, value: number) -> string) & ((self: any, value: string) -> number)",
			K::Function,
		),
		case(
			"a table intersection widens nested",
			table_intersection,
			NESTED,
			"any",
			K::Unspellable,
		),
		case(
			"a table intersection is dropped at a parameter",
			table_intersection,
			PARAMETER,
			"",
			K::Unspellable,
		),
	]);
}

/// A nested shape the analyzer rejects or native codegen cannot use.
#[test]
#[expect(clippy::too_many_lines, reason = "the Go case table, kept whole")]
fn native_policy_nested_shapes() {
	run_policy_cases(&[
		case(
			"readonly members are stripped",
			|e| {
				let string = e.tn("string");
				let hi = e.prop(TypeAccess::Read, "hi", string);
				let key = e.tn("string");
				let value = e.tn("number");
				let indexer = e.ast.type_table_indexer(TypeAccess::Read, key, value);
				e.table(vec![hi, indexer])
			},
			NESTED,
			"{ hi: string, [string]: number }",
			K::Table,
		),
		case(
			"readonly arrays are stripped",
			|e| e.array("string", TypeAccess::Read),
			PARAMETER,
			"{ string }",
			K::Table,
		),
		case(
			"the receiver is any",
			|e| {
				let receiver = e.alias("WithMethod", K::Table, Vec::new());
				let number = e.tn("number");
				e.function(vec![("self", receiver)], None, number)
			},
			NESTED,
			"(self: any) -> number",
			K::Function,
		),
		case(
			"function type slots are nested",
			|e| {
				let symbol = e.symbol(SymbolFlags::Class);
				let number = e.tn("number");
				let boolean = e.tn("boolean");
				let counter = e.tn("Counter");
				e.f.origins.insert(counter, symbol);
				let var_args = e.ast.type_variadic(counter);
				let ret = e.tn("Counter");
				e.f.origins.insert(ret, symbol);
				e.function(vec![("start", number), ("flag", boolean)], Some(var_args), ret)
			},
			PARAMETER,
			"(start: number, flag: boolean, ...any) -> any",
			K::Function,
		),
		case(
			"a nested unspellable union member widens the union",
			|e| {
				let number = e.tn("number");
				let unknown = e.tn("unknown");
				e.union(vec![number, unknown])
			},
			NESTED,
			"any",
			K::Unspellable,
		),
		case(
			"nested booleans are spelled",
			|e| {
				let boolean = e.tn("boolean");
				e.ast.optional_type(boolean)
			},
			NESTED,
			"boolean?",
			K::Boolean,
		),
		case(
			"a nested Instance is spelled by name",
			|e| {
				let part = e.roblox("Part");
				let prop = e.prop(TypeAccess::None, "part", part);
				e.table(vec![prop])
			},
			PARAMETER,
			"{ part: Part }",
			K::Table,
		),
		case(
			"an enum object is any",
			|e| {
				let string = e.tn("string");
				let prop = e.prop(TypeAccess::None, "Up", string);
				let table = e.table(vec![prop]);
				e.from(table, SymbolFlags::RegularEnum)
			},
			NESTED,
			"any",
			K::Unspellable,
		),
		case(
			"a Roblox enum is any",
			|e| {
				let reference = e.ast.type_qualified_reference("Enum", "Material", None);
				e.from(reference, SymbolFlags::None)
			},
			NESTED,
			"any",
			K::Unspellable,
		),
		case(
			"a table at the depth limit is spelled",
			|e| e.nested_tables(5, "number"),
			PARAMETER,
			"{ a: { a: { a: { a: { a: number } } } } }",
			K::Table,
		),
		case(
			"a table past the depth limit is any table",
			|e| e.nested_tables(6, "number"),
			PARAMETER,
			"{ a: { a: { a: { a: { a: { [any]: any } } } } } }",
			K::Table,
		),
		case(
			"duplicate members after widening collapse",
			|e| {
				let plain = e.array("number", TypeAccess::None);
				let readonly = e.array("number", TypeAccess::Read);
				e.union(vec![plain, readonly])
			},
			NESTED,
			"{ number }",
			K::Table,
		),
	]);
}
