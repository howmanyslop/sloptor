//! `luautypes_emit_test.go` and `luautypes_emit_internal_test.go`.

use rotor_tsrs::ast as tsast;

use super::analyze::{analyze_luau_strict, find_luau_analyzer};
use super::{Fixture, render_luau_types, with_fixture};
use crate::control::transform_method_declaration_with_receiver;
use crate::pointer::MapPointer;
use crate::prelude::*;
use crate::sourcefile::transform_source_file;
use crate::tests::{render_list, transformer_testdata, with_state};

/// `requireLines`: every line of `want` appears in `rendered`.
pub fn require_lines(rendered: &str, want: &[&str]) {
	let missing: Vec<String> = want
		.iter()
		.filter(|line| !rendered.contains(&format!("{line}\n")))
		.map(|line| format!("missing line {line:?}"))
		.collect();
	assert!(
		missing.is_empty(),
		"{}\n--- rendered ---\n{rendered}",
		missing.join("\n")
	);
}

/// `requireClean`: the analyzer, when one is installed, reports no type or syntax error.
pub fn require_clean(rendered: &str) {
	let Some(analyzer) = find_luau_analyzer() else {
		return;
	};
	if let Some(findings) = analyze_luau_strict(&analyzer, rendered, true) {
		assert!(
			findings.is_empty(),
			"does not analyze clean:\n{}\n--- rendered ---\n{rendered}",
			findings.join("\n")
		);
	}
}

/// Parameters, `...`, `self`, generics and returns are spelled from the builder; a defaulted
/// parameter is re-bound to its non-nil type right after its default; async bodies return the
/// settled value and the `TS.async` local carries the Promise-returning type; a void body with
/// a bare `return` is `...nil`.
#[test]
#[ignore = "needs slice ii"]
fn luau_types_emission_signatures() {
	let rendered = render_luau_types("src/emission.ts", true);
	require_lines(
		&rendered,
		&[
			"local function plain(a: number, b: string): boolean",
			"local function optionalParams(a: number, b: string?, c: number?): number",
			"local function restParams(first: number, ...: string): number",
			"local function restLoop(...: number): number",
			"local function restArray(...: string): { string }",
			"\tlocal rest = { ... }",
			"local function fact(n: number): number",
			"local function destructured(_param: { x: number, y: number }, _param_1: { string }): string",
			"local function thisParam(self: { x: number }, n: number): number",
			"local function luaTuple(): (number, string)",
			"local function optionalTuple(flag: boolean): (number, string?)",
			"local function nothing(): ()",
			"local function bareReturn(flag: boolean): ...nil",
			"local fetched: (n: number) -> Promise<number> = TS.async(function(n: number): number",
			"local fetchArrow: (n: number) -> Promise<number> = TS.async(function(n: number): number",
			"local awaited: () -> Promise<number> = TS.async(function(): number",
			"\tlocal value = TS.await(fetched(1))",
			"\tlocal _binding = TS.await(pair())",
			"local function counter(limit: number): Generator<number, nil, unknown>",
			"local identity = function<T>(value: T): T",
			"local function nested(): (n: number) -> number",
			"\tlocal double = function(n: number): number",
		],
	);
	// The refinement shadow follows the default and infers from the cast.
	assert!(
		rendered.contains("\tif c == nil then\n\t\tc = 2\n\tend\n\tlocal c = c :: number\n"),
		"defaulted parameter is not re-bound after its default:\n{rendered}"
	);
	require_clean(&rendered);
}

/// A logical chain's `_condition` temp is re-bound to the expression's TS type, and a literal
/// spilled to a temp by such a chain keeps its shape.
#[test]
#[ignore = "needs slice ii"]
fn luau_types_emission_refines_condition_temps() {
	let rendered = render_luau_types("src/emission.ts", true);
	require_lines(
		&rendered,
		&[
			"local function merged(defaults: Options, partial: { host: string?, port: number? }): Options",
			"\tlocal _condition = _condition :: string",
			"\t_object.host = _condition",
			"\tlocal _condition_1 = _condition_1 :: number",
			"\t_object.port = _condition_1",
			"\treturn _object",
			"local function fallback(name: string?): string",
		],
	);
	assert!(
		!rendered.contains("_object[_left]"),
		"a literal key must not be pinned to a temp with luauTypes on:\n{rendered}"
	);
}

/// Object-literal methods take an explicit `self`: the literal's contextual alias when there is
/// one, a written `this` parameter's type, and `any` otherwise.
#[test]
#[ignore = "needs slice ii"]
fn luau_types_emission_object_literal_methods() {
	let rendered = render_luau_types("src/emission.ts", true);
	require_lines(
		&rendered,
		&[
			"local typed = {",
			"\tinc = function(self: Counter): ()",
			"\tinc = function(self: any): ()",
		],
	);
	assert_eq!(
		rendered.matches("function(self: Counter): ()").count(),
		2,
		"expected the contextual and the explicit `this` method to both take `self: Counter`:\n{rendered}"
	);
}

/// Locals read like hand-written Luau: only a local with no initializer, a hoisted one, or one
/// whose written type differs from its initializer's widened type is annotated.
#[test]
#[ignore = "needs slice ii"]
fn luau_types_emission_locals() {
	let rendered = render_luau_types("src/locals.ts", true);
	require_lines(
		&rendered,
		&[
			"local aNumber = _primitives.aNumber",
			"local aString = _primitives.aString",
			"\tlocal count = 0",
			"\tlocal label: string?",
			"\tlocal value = if flag then \"a\" else 1",
			"\tlocal mode: \"r\" | \"w\" = \"r\"",
			"local later: () -> number",
			"function later(): number",
		],
	);
	assert!(
		rendered.contains("\tlocal names = { \"a\", \"b\" }\n"),
		"a const with an initializer must stay unannotated for Luau to infer:\n{rendered}"
	);
	require_clean(&rendered);
}

/// In a `//!native` file with both options on, the native policy still owns every signature
/// (byte-identical to nativeTypes alone) and luauTypes owns the locals.
#[test]
#[ignore = "needs slice ii"]
fn luau_types_keeps_native_signatures() {
	let render_with = |luau_types: bool| {
		with_state(&transformer_testdata("functions"), "src/native.ts", |state| {
			state.native_types = true;
			state.luau_types = luau_types;
			let statements = transform_source_file(state);
			let diagnostics = state.diags.flush();
			assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
			render_list(state, statements)
		})
	};
	// A function statement, or the callback handed to TS.async (whose local is luauTypes' to
	// annotate, so only the callback is compared).
	let signatures = |rendered: &str| -> Vec<String> {
		rendered
			.split('\n')
			.filter_map(|line| {
				let trimmed = line.trim_start();
				if trimmed.starts_with("function ") || trimmed.starts_with("local function ") {
					return Some(line.trim().to_owned());
				}
				line.find("TS.async(function")
					.map(|index| line[index..].trim().to_owned())
			})
			.collect()
	};
	let native = render_with(false);
	let both = render_with(true);
	assert_eq!(
		signatures(&both),
		signatures(&native),
		"signatures changed when luauTypes joined nativeTypes"
	);
	assert!(
		both.contains("local loadAsync: (count: number) -> Promise<number> = TS.async("),
		"luauTypes should still annotate the TS.async local in a native file:\n{both}"
	);
	assert!(
		!native.contains("local loadAsync:"),
		"nativeTypes alone must not annotate locals:\n{native}"
	);
}

/// `objectLiteralMethod`: the method of a top-level `const` initialized with an object literal.
fn object_literal_method(fixture: &Fixture<'_, '_>, name: &str) -> P<Node> {
	let declaration = fixture.declaration(name);
	let initializer = declaration
		.initializer()
		.filter(|&initializer| tsast::is_object_literal_expression(initializer))
		.unwrap_or_else(|| panic!("{name:?} is not initialized with an object literal"));
	initializer
		.properties()
		.iter()
		.copied()
		.find(|&property| tsast::is_method_declaration(property))
		.unwrap_or_else(|| panic!("{name:?} has no method"))
}

/// A method whose object was spilled to a temp is emitted in dot form with an explicit `self`:
/// `any` for an unnamed literal, the receiver the class transform passes, and a written `this`
/// parameter over either.
#[test]
#[ignore = "needs slice ii"]
fn luau_types_method_receiver() {
	with_fixture("src/emission.ts", |fixture| {
		fixture.state.luau_types = true;
		fixture.state.is_luau_typed = true;
		for (name, receiver, want) in [
			("anonymous", None, "function _object.inc(self: any): ()"),
			("anonymous", Some("Counter"), "function _object.inc(self: Counter): ()"),
			("explicit", Some("Other"), "function _object.inc(self: Counter): ()"),
		] {
			let receiver = receiver.map(|receiver| fixture.state.ast.type_name(receiver));
			let value = fixture.state.ast.temp_id("object");
			let mut pointer = MapPointer {
				name: "object".to_owned(),
				value,
			};
			let method = object_literal_method(fixture, name);
			let statements = transform_method_declaration_with_receiver(fixture.state, method, &mut pointer, receiver);
			let got = render_list(fixture.state, statements);
			assert!(
				got.starts_with(&format!("{want}\n")),
				"rendered:\n{got}\nwant first line {want:?}"
			);
			assert!(
				!got.contains("_object:inc"),
				"luauTypes must never emit a colon-form method:\n{got}"
			);
		}
	});
}

/// Without luauTypes the same method keeps rbxtsc's colon form.
#[test]
#[ignore = "needs slice ii"]
fn method_receiver_ignored_without_luau_types() {
	with_fixture("src/emission.ts", |fixture| {
		let value = fixture.state.ast.temp_id("object");
		let mut pointer = MapPointer {
			name: "object".to_owned(),
			value,
		};
		let receiver = fixture.state.ast.type_name("Counter");
		let method = object_literal_method(fixture, "anonymous");
		let statements =
			transform_method_declaration_with_receiver(fixture.state, method, &mut pointer, Some(receiver));
		let got = render_list(fixture.state, statements);
		assert!(got.starts_with("function _object:inc()\n"), "rendered:\n{got}");
	});
}

/// The signature line slice ii will emit for a function declaration, assembled from this
/// slice's hooks the way `transformFunctionBody` and `transformParameters` call them.
fn signature_line(fixture: &mut Fixture<'_, '_>, name: &str) -> String {
	use super::super::{
		get_luau_parameter_type, get_luau_self_type, get_luau_vararg_type, pop_type_generics, push_type_generics,
		signature_generics, signature_return_type,
	};
	let node = fixture.declaration(name);
	let generics = signature_generics(fixture.state, node);
	let names: Vec<String> = generics.iter().map(|generic| generic.name.clone()).collect();
	push_type_generics(fixture.state, names.clone(), false);
	let mut parameters = Vec::new();
	if crate::shared::this_parameter(node).is_some() {
		let this = get_luau_self_type(fixture.state, node).expect("a self type");
		parameters.push(format!("self: {}", fixture.text(this)));
	}
	for &parameter in node.parameters() {
		let declaration = parameter.as_parameter_declaration();
		if tsast::is_this_identifier(declaration.name) {
			continue;
		}
		if declaration.dot_dot_dot_token().is_some() {
			let var_args = get_luau_vararg_type(fixture.state, node).expect("a vararg type");
			parameters.push(format!("...: {}", fixture.text(var_args)));
			continue;
		}
		let t = get_luau_parameter_type(fixture.state, parameter).expect("a parameter type");
		parameters.push(format!("{}: {}", declaration.name.text(), fixture.text(t)));
	}
	let ret = signature_return_type(fixture.state, node).map(|ret| format!(": {}", fixture.text(ret)));
	pop_type_generics(fixture.state);
	let generics = if names.is_empty() {
		String::new()
	} else {
		format!("<{}>", names.join(", "))
	};
	format!(
		"local function {name}{generics}({}){}",
		parameters.join(", "),
		ret.unwrap_or_default()
	)
}

/// The signature lines of `TestLuauTypesEmissionSignatures` that need no destructuring,
/// straight from the hooks.
#[test]
fn luau_types_emission_signature_hooks() {
	with_fixture("src/emission.ts", |fixture| {
		fixture.state.luau_types = true;
		fixture.state.is_luau_typed = true;
		let mut failures = Vec::new();
		for (name, want) in [
			("plain", "local function plain(a: number, b: string): boolean"),
			(
				"optionalParams",
				"local function optionalParams(a: number, b: string?, c: number?): number",
			),
			(
				"restParams",
				"local function restParams(first: number, ...: string): number",
			),
			("restLoop", "local function restLoop(...: number): number"),
			("restArray", "local function restArray(...: string): { string }"),
			(
				"thisParam",
				"local function thisParam(self: { x: number }, n: number): number",
			),
			("luaTuple", "local function luaTuple(): (number, string)"),
			(
				"optionalTuple",
				"local function optionalTuple(flag: boolean): (number, string?)",
			),
			("nothing", "local function nothing(): ()"),
			("bareReturn", "local function bareReturn(flag: boolean): ...nil"),
			(
				"counter",
				"local function counter(limit: number): Generator<number, nil, unknown>",
			),
			(
				"merged",
				"local function merged(defaults: Options, partial: { host: string?, port: number? }): Options",
			),
			("fallback", "local function fallback(name: string?): string"),
			("nested", "local function nested(): (n: number) -> number"),
		] {
			let got = signature_line(fixture, name);
			if got != want {
				failures.push(format!("{name}: got {got:?}, want {want:?}"));
			}
		}
		assert!(failures.is_empty(), "{}", failures.join("\n"));
	});
}

/// With neither feature on, the hooks slices ii and iii call return what Go returns.
#[test]
fn luau_types_hooks_off() {
	use super::super::{
		luau_enum_table_cast, luau_refined_condition, pop_type_generics, push_type_generics, signature_generics,
		signature_return_type, signature_var_args_type,
	};
	with_fixture("src/emission.ts", |fixture| {
		let node = fixture.declaration("restParams");
		push_type_generics(fixture.state, Vec::new(), false);
		assert_eq!(signature_generics(fixture.state, node).len(), 0);
		assert!(signature_return_type(fixture.state, node).is_none());
		assert!(signature_var_args_type(fixture.state, node).is_none());
		pop_type_generics(fixture.state);
		let id = fixture.state.ast.temp_id("condition");
		assert_eq!(luau_refined_condition(fixture.state, node, id), id);
		let table = fixture.state.ast.id("t");
		assert_eq!(luau_enum_table_cast(fixture.state, node, table), table);
		assert!(
			fixture.state.slices.luau_types.shared.is_none(),
			"no builder was created"
		);
		let diagnostics = fixture.state.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}
