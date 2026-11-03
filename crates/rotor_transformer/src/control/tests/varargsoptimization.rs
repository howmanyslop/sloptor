//! `varargsoptimization_test.go`, over Go's `buildVarArgsOptimizationState` projects.

use rotor_tsrs::ast as tsast;

use super::with_var_args_state;
use crate::control::parameters::transform_parameters;
use crate::control::varargsoptimization::analyze_var_args_optimization;
use crate::prelude::*;
use crate::sourcefile::transform_source_file;
use crate::tests::render_list;

/// Go's `varArgsOptimizationNodes`: the rest parameter of the first function declaration with
/// a body and one, and that function.
fn var_args_optimization_nodes(state: &State<'_>) -> (P<Node>, P<Node>) {
	for &node in state.source_file.statements().nodes() {
		if !tsast::is_function_declaration(node) || node.body().is_none() {
			continue;
		}
		for &parameter in node.parameters() {
			if parameter.as_parameter_declaration().dot_dot_dot_token().is_some() {
				return (parameter, node);
			}
		}
	}
	panic!("no function with a rest parameter");
}

struct AnalysisCase {
	name: &'static str,
	source: &'static str,
	parameter_index: usize,
	size_access_count: usize,
	for_of_access_count: usize,
	is_optimizable: bool,
}

const fn case(name: &'static str, source: &'static str, is_optimizable: bool) -> AnalysisCase {
	AnalysisCase {
		name,
		source,
		parameter_index: 0,
		size_access_count: 0,
		for_of_access_count: 0,
		is_optimizable,
	}
}

#[test]
#[expect(clippy::too_many_lines, reason = "one row per Go table case")]
fn var_args_optimization_analysis() {
	let tests = [
		AnalysisCase {
			parameter_index: 1,
			..case(
				"safe indexed read",
				"function foo(first: any, ...args: any[]) { return args[0]; }",
				true,
			)
		},
		case(
			"safe indexed read through type assertion",
			"function foo(...args: any[]) { return (args as any)[0]; }",
			true,
		),
		AnalysisCase {
			size_access_count: 1,
			..case(
				"safe size call",
				"function foo(...args: any[]) { return args.size(); }",
				true,
			)
		},
		AnalysisCase {
			for_of_access_count: 1,
			..case(
				"safe plain identifier for of",
				"function foo(...args: any[]) { for (const value of args) { value; } }",
				true,
			)
		},
		case(
			"safe call spread",
			"declare function bar(...values: any[]): void; function foo(...args: any[]) { bar(...args); }",
			true,
		),
		case(
			"unsafe nested function capture",
			"function foo(...args: any[]) { return function() { return args[0]; }; }",
			false,
		),
		case(
			"unsafe try block use",
			"function foo(...args: any[]) { try { return args[0]; } catch {} }",
			false,
		),
		case(
			"unsafe element assignment",
			"function foo(...args: any[]) { args[0] = 1; }",
			false,
		),
		case(
			"unsafe alias",
			"function foo(...args: any[]) { const alias = args; return alias; }",
			false,
		),
		case(
			"unsafe passing as an argument",
			"declare function consume(value: any[]): void; function foo(...args: any[]) { consume(args); }",
			false,
		),
		case(
			"unsafe returning the parameter",
			"function foo(...args: any[]) { return args; }",
			false,
		),
		case(
			"unsafe shorthand property",
			"function foo(...args: any[]) { return { args }; }",
			false,
		),
		AnalysisCase {
			size_access_count: 1,
			..case(
				"unsafe shorthand property after a size call",
				"function foo(...args: any[]) { args.size(); return { args }; }",
				false,
			)
		},
		case(
			"unsafe property access",
			"function foo(...args: any[]) { return args.map; }",
			false,
		),
		case(
			"unsafe array literal spread",
			"function foo(...args: any[]) { return [...args]; }",
			false,
		),
		AnalysisCase {
			parameter_index: 1,
			..case(
				"unsafe array literal spread with other elements",
				"function foo(x: any[], ...args: any[]) { return [...x, ...args]; }",
				false,
			)
		},
		case(
			"unsafe destructuring for of",
			"function foo(...args: any[]) { for (const [value] of args) { value; } }",
			false,
		),
		case(
			"unsafe generator",
			"function* foo(...args: any[]) { yield args[0]; }",
			false,
		),
	];

	for test in tests {
		with_var_args_state(test.source, |s| {
			let (parameter, function_node) = var_args_optimization_nodes(s);
			let got = analyze_var_args_optimization(s, parameter, function_node);
			assert_eq!(
				got.parameter_index,
				Some(test.parameter_index),
				"{}: ParameterIndex",
				test.name
			);
			assert_eq!(
				got.size_access_count, test.size_access_count,
				"{}: SizeAccessCount",
				test.name
			);
			assert_eq!(
				got.for_of_access_count, test.for_of_access_count,
				"{}: ForOfAccessCount",
				test.name
			);
			assert_eq!(got.is_optimizable, test.is_optimizable, "{}: IsOptimizable", test.name);
		});
	}
}

#[test]
fn optimizable_var_args_registration() {
	with_var_args_state("function foo(...args: any[]) { return args[0]; }", |s| {
		let (parameter, function_node) = var_args_optimization_nodes(s);
		let data = analyze_var_args_optimization(s, parameter, function_node);
		assert!(data.is_optimizable, "expected indexed rest parameter to be optimizable");

		s.register_optimizable_var_args(parameter, data.clone());
		let name = parameter.as_parameter_declaration().name;
		assert_eq!(s.get_optimizable_var_args_data(name), Some(data), "registered data");

		s.unregister_optimizable_var_args(parameter);
		assert_eq!(s.get_optimizable_var_args_data(name), None, "unregistered data");
	});
}

#[test]
fn transform_parameters_analyzes_rest_parameter() {
	with_var_args_state("function foo(...args: any[]) { return args.size(); }", |s| {
		let (_, function_node) = var_args_optimization_nodes(s);
		let transformed = transform_parameters(s, function_node);
		let data = transformed
			.var_args_data
			.filter(|data| data.is_optimizable)
			.expect("transformParameters did not retain optimizable rest parameter data");
		assert_eq!(data.size_access_count, 1, "SizeAccessCount");
	});
}

#[test]
fn var_args_optimized_emit() {
	let source = "declare function consume(...values: Array<number>): void;

export function safeRead(index: number, ...args: Array<number>) {
	const a = args[0];
	const b = args[1];
	const c = args[index];
	return a + b + c;
}

export function safeSize(...args: Array<number>) {
	return args.size();
}

export function safeCachedSize(...args: Array<number>) {
	return args.size() + args.size();
}

export function safeSpread(...args: Array<number>) {
	consume(...args);
}

export function safeForOf(...args: Array<number>) {
	let sum = 0;
	for (const x of args) {
		sum += x;
	}
	return sum;
}
";
	let got = with_var_args_state(source, |s| {
		let statements = transform_source_file(s);
		render_list(s, statements)
	});
	let want = "-- Compiled with sloptor v3.0.0-beta.2
local function safeRead(index, ...)
	local a = (...)
	local b = (select(2, ...))
	local c = (select(index + 1, ...))
	return a + b + c
end
local function safeSize(...)
	return select(\"#\", ...)
end
local function safeCachedSize(...)
	local _args_length = select(\"#\", ...)
	return _args_length + _args_length
end
local function safeSpread(...)
	consume(...)
end
local function safeForOf(...)
	local sum = 0
	for _i = 1, select(\"#\", ...) do
		local x = (select(_i, ...))
		sum += x
	end
	return sum
end
return {
	safeRead = safeRead,
	safeSize = safeSize,
	safeCachedSize = safeCachedSize,
	safeSpread = safeSpread,
	safeForOf = safeForOf,
}
";
	assert_eq!(got, want);
}

#[test]
fn var_args_unsafe_fallback() {
	let source = "export function unsafeNested(...args: Array<number>) {
	return function () {
		return args[0];
	};
}

export function* unsafeGenerator(...args: Array<number>) {
	yield args[0];
}
";
	let got = with_var_args_state(source, |s| {
		let statements = transform_source_file(s);
		render_list(s, statements)
	});
	let want = "-- Compiled with sloptor v3.0.0-beta.2
local TS = _G[script]
local function unsafeNested(...)
	local args = { ... }
	return function()
		return args[1]
	end
end
local function unsafeGenerator(...)
	local args = { ... }
	return TS.generator(function()
		coroutine.yield(args[1])
	end)
end
return {
	unsafeNested = unsafeNested,
	unsafeGenerator = unsafeGenerator,
}
";
	assert_eq!(got, want);
}
