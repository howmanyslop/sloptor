//! Port of `internal/luau/render/solvetempids_test.go`.

#![expect(
	clippy::tuple_array_conversions,
	reason = "tests bind sibling nodes in one statement"
)]

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use super::dsl::{call, id};
use super::{render_both, stmts, temp_of};
use crate::render::solve_temp_ids::solve_temp_ids;
use crate::render::{RenderState, render_statements};
use crate::{Ast, List, NodeId, TemporaryIdentifier, element};

fn solve_for(a: &Ast, statements: List<element::Statement>) -> RenderState {
	let mut state = RenderState::new();
	solve_temp_ids(&mut state, a, statements.into()).expect("solve");
	state
}

fn name_of<'s>(a: &Ast, state: &'s RenderState, temp: NodeId) -> Option<&'s str> {
	state.temp_name(temp_of(a, temp))
}

fn declare(a: &mut Ast, left: NodeId, value: f64) -> NodeId {
	let value = a.num(value);
	a.variable_declaration(left, Some(value.into()))
}

#[test]
fn temp_ids_basic() {
	let mut a = Ast::new();
	let (t1, t2) = (a.temp_id(""), a.temp_id(""));
	let (first, second) = (declare(&mut a, t1, 1.0), declare(&mut a, t2, 2.0));
	let statements = stmts(&mut a, &[first, second]);
	let state = solve_for(&a, statements);
	assert_eq!(name_of(&a, &state, t1), Some("_"), "first temp");
	assert_eq!(name_of(&a, &state, t2), Some("_1"), "second temp");
}

#[test]
fn temp_ids_named() {
	let mut a = Ast::new();
	let (t1, t2) = (a.temp_id("foo"), a.temp_id("foo"));
	let (first, second) = (declare(&mut a, t1, 1.0), declare(&mut a, t2, 2.0));
	let statements = stmts(&mut a, &[first, second]);
	let state = solve_for(&a, statements);
	assert_eq!(name_of(&a, &state, t1), Some("_foo"), "first");
	assert_eq!(name_of(&a, &state, t2), Some("_foo_1"), "second");
}

#[test]
fn temp_ids_exact_named() {
	let mut a = Ast::new();
	let temp = a.exact_temp_id("foo");
	let declaration = declare(&mut a, temp, 1.0);
	let statements = stmts(&mut a, &[declaration]);
	let state = solve_for(&a, statements);
	assert_eq!(name_of(&a, &state, temp), Some("foo"), "exact temp");
}

#[test]
fn temp_ids_exact_named_avoid_collision() {
	let mut a = Ast::new();
	let temp = a.exact_temp_id("foo");
	let foo = a.id("foo");
	let (first, second) = (declare(&mut a, foo, 1.0), declare(&mut a, temp, 2.0));
	let statements = stmts(&mut a, &[first, second]);
	let state = solve_for(&a, statements);
	assert_eq!(name_of(&a, &state, temp), Some("foo_1"), "exact temp");
}

#[test]
fn temp_ids_avoid_declared_locals() {
	let mut a = Ast::new();
	let temp = a.temp_id("foo");
	let local = a.id("_foo");
	let (first, second) = (declare(&mut a, local, 1.0), declare(&mut a, temp, 2.0));
	let statements = stmts(&mut a, &[first, second]);
	let state = solve_for(&a, statements);
	assert_eq!(name_of(&a, &state, temp), Some("_foo_1"), "collides with declared _foo");
}

#[test]
fn temp_ids_avoid_local_function_declarations() {
	let mut a = Ast::new();
	let temp = a.temp_id("foo");
	let function = |a: &mut Ast, name: NodeId| {
		let (params, body) = (
			a.new_list::<element::AnyIdentifier>(),
			a.new_list::<element::Statement>(),
		);
		a.function_declaration(true, name, params, false, body)
	};
	let name = a.id("_foo");
	let first = function(&mut a, name);
	let second = function(&mut a, temp);
	let statements = stmts(&mut a, &[first, second]);
	let state = solve_for(&a, statements);
	assert_eq!(
		name_of(&a, &state, temp),
		Some("_foo_1"),
		"collides with local function _foo"
	);
}

#[test]
fn temp_ids_avoid_identifier_uses() {
	let mut a = Ast::new();
	let temp = a.temp_id("foo");
	let declaration = declare(&mut a, temp, 1.0);
	let use_call = call(id("_foo"), vec![]).build(&mut a);
	let statement = a.call_statement(use_call);
	let statements = stmts(&mut a, &[declaration, statement]);
	let state = solve_for(&a, statements);
	assert_eq!(
		name_of(&a, &state, temp),
		Some("_foo_1"),
		"collides with identifier use _foo"
	);
}

#[test]
fn temp_ids_avoid_nested_identifier_uses() {
	let mut a = Ast::new();
	let temp = a.temp_id("foo");
	let declaration = declare(&mut a, temp, 1.0);
	let use_call = call(id("_foo"), vec![]).build(&mut a);
	let statement = a.call_statement(use_call);
	let body = stmts(&mut a, &[statement]);
	let (name, params) = (a.id("later"), a.new_list::<element::AnyIdentifier>());
	let later = a.function_declaration(true, name, params, false, body);
	let statements = stmts(&mut a, &[declaration, later]);
	let state = solve_for(&a, statements);
	assert_eq!(
		name_of(&a, &state, temp),
		Some("_foo_1"),
		"collides with nested identifier use _foo"
	);
}

#[test]
fn temp_ids_scoped_functions_dont_collide() {
	// Two function expressions, each with its own temp: both may use "_".
	let mut a = Ast::new();
	let make = |a: &mut Ast| {
		let temp = a.temp_id("");
		let declaration = declare(a, temp, 1.0);
		let body = stmts(a, &[declaration]);
		let params = a.new_list::<element::AnyIdentifier>();
		let function = a.function_expression(params, false, body);
		let f = a.id("f");
		(temp, a.variable_declaration(f, Some(function.into())))
	};
	let (first_temp, first) = make(&mut a);
	let (second_temp, second) = make(&mut a);
	let statements = stmts(&mut a, &[first, second]);
	let state = solve_for(&a, statements);
	assert_eq!(name_of(&a, &state, first_temp), Some("_"), "separate function scopes");
	assert_eq!(name_of(&a, &state, second_temp), Some("_"), "separate function scopes");
}

#[test]
fn temp_ids_overlapping_hints() {
	type Temp = (&'static str, bool);
	type Case = (&'static str, [Option<Temp>; 4], [&'static str; 4]);
	let cases: [Case; 4] = [
		(
			"numbered hints",
			[Some(("foo", false)), Some(("foo", false)), Some(("foo_1", false)), None],
			["_foo", "_foo_1", "_foo_1_2", ""],
		),
		(
			"normal and exact",
			[Some(("foo", false)), Some(("foo", false)), Some(("_foo_1", true)), None],
			["_foo", "_foo_1", "_foo_1_2", ""],
		),
		(
			"anonymous and exact",
			[Some(("", false)), Some(("", false)), Some(("_1", true)), None],
			["_", "_1", "_1_2", ""],
		),
		(
			"different separators",
			[
				Some(("", false)),
				Some(("_", true)),
				Some(("", false)),
				Some(("_", true)),
			],
			["_", "__1", "_1", "__2"],
		),
	];
	for (name, temps, names) in cases {
		let mut a = Ast::new();
		let statements = a.new_list::<element::Statement>();
		let mut want = String::new();
		for ((hint, exact), expected) in temps.into_iter().flatten().zip(names) {
			let temp = if exact { a.exact_temp_id(hint) } else { a.temp_id(hint) };
			let declaration = declare(&mut a, temp, 1.0);
			a.list_push(statements, declaration);
			// Reusing a temp must keep its first assigned name.
			let reuse = call(temp, vec![]).build(&mut a);
			let reuse = a.call_statement(reuse);
			a.list_push(statements, reuse);
			write!(want, "local {expected} = 1\n{expected}()\n").expect("write to a String");
		}
		assert_eq!(render_both(&a, statements), want, "{name}");
	}
}

#[test]
fn temp_ids_ancestor_and_sibling_reservations() {
	let mut a = Ast::new();
	let declaration = |a: &mut Ast| {
		let temp = a.temp_id("foo");
		declare(a, temp, 1.0)
	};
	let child = |a: &mut Ast| {
		let (first, second) = (declaration(a), declaration(a));
		let body = stmts(a, &[first, second]);
		a.do_statement(body)
	};
	let first = declaration(&mut a);
	let first_child = child(&mut a);
	let second = declaration(&mut a);
	let second_child = child(&mut a);
	// This late declaration must reserve the ancestor name in both children.
	let late = a.id("_foo_2");
	let late = declare(&mut a, late, 0.0);
	let statements = stmts(&mut a, &[first, first_child, second, second_child, late]);
	let want = "local _foo = 1\ndo\n\tlocal _foo_1 = 1\n\tlocal _foo_3 = 1\nend\n\
		local _foo_1 = 1\ndo\n\tlocal _foo_3 = 1\n\tlocal _foo_4 = 1\nend\nlocal _foo_2 = 0\n";
	assert_eq!(render_both(&a, statements), want);
}

/// The original probing rule, independent of the optimized solver's scope
/// walk and progress state. Final-name counter writes are part of the output
/// contract: a later overlapping hint can read them.
struct LegacyScope {
	ids: HashSet<String>,
	last_try: HashMap<String, usize>,
	parent: Option<usize>,
}

struct Legacy(Vec<LegacyScope>);

impl Legacy {
	fn has(&self, mut scope: usize, name: &str) -> bool {
		loop {
			if self.0[scope].ids.contains(name) {
				return true;
			}
			match self.0[scope].parent {
				Some(parent) => scope = parent,
				None => return false,
			}
		}
	}

	fn name(&mut self, scope: usize, temp: &TemporaryIdentifier) -> String {
		let (base, separator) = if temp.exact_name {
			(temp.name.clone(), "_")
		} else {
			(format!("_{}", temp.name), if temp.name.is_empty() { "" } else { "_" })
		};
		let mut i = self.0[scope].last_try.get(&base).copied().unwrap_or(0);
		if i == 0 {
			i = 1;
		}
		let mut input = base.clone();
		while self.has(scope, &input) {
			input = format!("{base}{separator}{i}");
			i += 1;
		}
		self.0[scope].ids.insert(input.clone());
		self.0[scope].last_try.insert(input.clone(), i);
		input
	}
}

/// A deterministic generator standing in for Go's `rand.New(rand.NewSource(1337))`.
struct SplitMix(u64);

impl SplitMix {
	fn next(&mut self) -> u64 {
		self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
		let mut z = self.0;
		z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
		z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
		z ^ (z >> 31)
	}

	fn below(&mut self, n: usize) -> usize {
		usize::try_from(self.next() % u64::try_from(n).expect("n fits")).expect("index fits")
	}
}

struct Scenario {
	rng: SplitMix,
	legacy: Legacy,
	pending: Vec<(NodeId, usize)>,
}

const HINTS: [&str; 9] = ["", "foo", "foo_1", "foo_2", "_foo", "_foo_1", "_", "_1", "_1_2"];

impl Scenario {
	fn build(&mut self, a: &mut Ast, depth: usize, parent: Option<usize>) -> List<element::Statement> {
		self.legacy.0.push(LegacyScope {
			ids: HashSet::new(),
			last_try: HashMap::new(),
			parent,
		});
		let scope = self.legacy.0.len() - 1;
		let statements = a.new_list::<element::Statement>();
		for i in 0..16 {
			if i % 4 == 0 {
				// Declarations reserve names before solving even when they
				// occur after a child or leave holes in the suffix sequence.
				let hint = HINTS[self.rng.below(HINTS.len())];
				let suffix = ["", "_1", "_3", "_7"][self.rng.below(4)];
				let name = format!("_{hint}{suffix}");
				self.legacy.0[scope].ids.insert(name.clone());
				let local = a.id(name);
				let declaration = declare(a, local, 0.0);
				a.list_push(statements, declaration);
			}
			let hint = HINTS[self.rng.below(HINTS.len())];
			let temp = a.temp_id(hint);
			if self.rng.below(2) == 0 && !hint.is_empty() {
				a.get_mut::<TemporaryIdentifier>(temp).expect("temp").exact_name = true;
			}
			self.pending.push((temp, scope));
			let declaration = declare(a, temp, 1.0);
			a.list_push(statements, declaration);
			if depth < 2 && (i == 5 || i == 10) {
				let body = self.build(a, depth + 1, Some(scope));
				let block = a.do_statement(body);
				a.list_push(statements, block);
			}
		}
		statements
	}
}

#[test]
fn temp_ids_mixed_hints_match_legacy() {
	let mut scenario = Scenario {
		rng: SplitMix(1337),
		legacy: Legacy(Vec::new()),
		pending: Vec::new(),
	};
	for index in 0..25 {
		let mut a = Ast::new();
		scenario.legacy.0.clear();
		scenario.pending.clear();
		let statements = scenario.build(&mut a, 0, None);
		let mut oracle = RenderState::new();
		for &(temp, scope) in &scenario.pending {
			let payload = a.get::<TemporaryIdentifier>(temp).expect("temp");
			let name = scenario.legacy.name(scope, payload);
			oracle.set_temp_name(payload.id, name);
		}
		let want = render_statements(&mut oracle, &a, statements).expect("oracle render");
		assert_eq!(
			render_both(&a, statements),
			want,
			"scenario {index} differs from legacy naming"
		);
	}
}

#[test]
fn temp_ids_inside_typeof() {
	// A temporary referenced from `typeof(expr)` is solved like any other use,
	// not left to the counter fallback.
	let mut a = Ast::new();
	let temp = a.temp_id("instance");
	let x = a.id("x");
	let annotation = a.type_typeof(temp);
	a.set_binding_type(x, Some(annotation));
	let first = declare(&mut a, temp, 1.0);
	let second = declare(&mut a, x, 2.0);
	let y = a.id("y");
	let ty = a.type_typeof(temp);
	let cast = a.cast(y, ty);
	let ret = a.return_statement(cast);
	let statements = stmts(&mut a, &[first, second, ret]);
	assert_eq!(
		render_both(&a, statements),
		"local _instance = 1\nlocal x: typeof(_instance) = 2\nreturn y :: typeof(_instance)\n"
	);
}

#[test]
fn temp_ids_typeof_reserves_identifier_uses() {
	// An identifier used inside `typeof(expr)` is reserved like a value use.
	let mut a = Ast::new();
	let temp = a.temp_id("instance");
	let used = a.id("_instance");
	let body = a.type_typeof(used);
	let alias = a.type_alias("T", Vec::new(), Some(body));
	let declaration = declare(&mut a, temp, 1.0);
	let statements = stmts(&mut a, &[alias, declaration]);
	assert_eq!(
		render_both(&a, statements),
		"type T = typeof(_instance)\nlocal _instance_1 = 1\n"
	);
}

#[test]
fn temp_ids_type_function_scope() {
	// Type function parameters live in the function's scope: a temporary in
	// the body avoids them, and a temporary after the function does not.
	let mut a = Ast::new();
	let (inner, outer) = (a.temp_id("result"), a.temp_id("result"));
	let param = a.id("_result");
	let params = a.list_from::<element::AnyIdentifier>([param]);
	let body_declaration = declare(&mut a, inner, 1.0);
	let body = stmts(&mut a, &[body_declaration]);
	let declaration = a.type_function_declaration("Keys", params, false, body);
	let after = declare(&mut a, outer, 2.0);
	let statements = stmts(&mut a, &[declaration, after]);
	assert_eq!(
		render_both(&a, statements),
		"type function Keys(_result)\n\tlocal _result_1 = 1\nend\nlocal _result = 2\n"
	);
}

#[test]
fn temp_ids_scope_underflow_fails() {
	// A head unshifted after construction has no parent, so it pushes no
	// scope while the original tail still pops one. Go then indexes the empty
	// scope stack and panics; the port fails with an error.
	let mut a = Ast::new();
	let g = call(id("g"), vec![]).build(&mut a);
	let g = a.call_statement(g);
	let body = stmts(&mut a, &[g]);
	let block = a.do_statement(body);
	let local = a.id("a");
	let early = declare(&mut a, local, 1.0);
	a.list_unshift(body, early);
	let temp = a.temp_id("t");
	let late = declare(&mut a, temp, 2.0);
	let statements = stmts(&mut a, &[block, late]);
	let mut state = RenderState::new();
	assert!(
		solve_temp_ids(&mut state, &a, statements.into()).is_err(),
		"solver must fail on underflow"
	);
	assert!(
		crate::render::render_ast(&a, statements).is_err(),
		"render_ast must fail on underflow"
	);
}
