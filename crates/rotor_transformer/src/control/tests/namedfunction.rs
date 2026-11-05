//! `namedfunction_test.go`: named function expressions lift to local function declarations.

use super::{render_file, transform_expecting_diagnostics};

/// `const f = function named() {}` is upstream's noFunctionExpressionName error. rotor lifts the
/// name into a local function declaration instead, because that is the only Luau form that carries
/// a debug name into a traceback — an assignment of a function expression, to a local or anything
/// else, compiles to an unnamed closure. rbxtsc aborts emission on the error, so there is no
/// oracle for the output shape.
#[test]
fn named_function_expression_mismatched_const_lifts_the_name() {
	let want = "local function named(x)
	return x
end
local f = named
print(f(1))
";
	assert_eq!(render_file("functions", "src/namedexpr.ts"), want);
}

/// A named function expression: direct call argument emits local declaration.
#[test]
fn named_function_expression_direct_call_argument_emits_local_declaration() {
	let want = "local function doSomeBullshit()
end
useEffect(doSomeBullshit, {})
";
	assert_eq!(render_file("functions", "src/namedcallarg.ts"), want);
}

/// A named function expression: matching const emits recursive local declaration.
#[test]
fn named_function_expression_matching_const_emits_recursive_local_declaration() {
	let want = "local function namedFunction(value)
	return if value == 0 then 0 else namedFunction(value - 1)
end
print(namedFunction(2))
";
	assert_eq!(render_file("functions", "src/namedconst.ts"), want);
}

/// A named function expression: direct call argument preserves recursion and argument order.
#[test]
fn named_function_expression_direct_call_argument_preserves_recursion_and_argument_order() {
	let want = "local function before()
	return 1
end
local function after()
	return 2
end
local _exp = before()
local function recurse(value)
	return if value == 0 then 0 else recurse(value - 1)
end
consume(_exp, recurse, after())
";
	assert_eq!(render_file("functions", "src/namedcallordering.ts"), want);
}

/// `registerOnClose(async function onBindToCloseAsync() {})` used to fail with
/// noFunctionExpressionName. The inner local function keeps the Luau debug name; the assignment
/// makes the name bind to TS.async's wrapper so a caller (and a self-call) get a Promise.
#[test]
fn named_function_expression_async_call_argument_wraps_the_named_local() {
	let want = "local function onBindToCloseAsync()
end
onBindToCloseAsync = TS.async(onBindToCloseAsync)
local stopBindToClose = registerOnClose(onBindToCloseAsync)
print(stopBindToClose)
";
	assert_eq!(render_file("functions", "src/namedasyncarg.ts"), want);
}

/// a self-call through the function name must hit TS.async, not the raw body. Matching names fold
/// into one local; a mismatched const still lifts the expression name and aliases it.
#[test]
fn named_function_expression_async_matching_const_binds_the_wrapper() {
	let want = "local function named(value)
	return if value == 0 then 0 else named(value - 1)
end
named = TS.async(named)
local function different()
end
different = TS.async(different)
local f = different
print(named(2), f)
";
	assert_eq!(render_file("functions", "src/namedasyncconst.ts"), want);
}

/// a named generator expression is the factory, same as `function* named()`. The name binds to
/// that factory, so a self-call creates a new generator instead of re-entering the iterator
/// callback.
#[test]
fn named_function_expression_generator_lifts_as_a_declaration() {
	let want = "local function named()
	return TS.generator(function()
		coroutine.yield(1)
	end)
end
local function generatorNamed()
	return TS.generator(function()
		coroutine.yield(2)
	end)
end
local g = generatorNamed
print(named(), g())
";
	assert_eq!(render_file("functions", "src/namedgenerator.ts"), want);
}

/// constructing the wrapper is work, so a short-circuit operand and a ternary arm must keep the
/// declaration and the TS.async assignment inside the branch.
#[test]
fn named_function_expression_async_conditional_operands_stay_conditional() {
	let want = "local _condition = flag
if _condition then
	local function shortNamed()
		return 1
	end
	shortNamed = TS.async(shortNamed)
	_condition = shortNamed
end
local short = _condition
local _result
if flag then
	local function thenNamed()
		return 1
	end
	thenNamed = TS.async(thenNamed)
	_result = thenNamed
else
	local function elseNamed()
		return 2
	end
	elseNamed = TS.async(elseNamed)
	_result = elseNamed
end
local ternary = _result
print(short, ternary)
";
	assert_eq!(render_file("functions", "src/namedasyncconditional.ts"), want);
}

/// `TestNamedFunctionExpression_lifts_from_every_synchronous_position` sweeps the positions that
/// used to report the diagnostic: a const whose name differs from the binding, a `let`, an export,
/// an assertion, an object-literal value, one of several declarators, an IIFE callee, and a call
/// argument behind an assertion. A colliding name is renamed rather than shadowing the binding it
/// collides with.
#[test]
fn named_function_expression_lifts_from_every_synchronous_position() {
	let want = "local function different()
end
local mismatch = different
local function letNamed_1()
end
local letNamed = letNamed_1
local function exportedNamed_1()
end
local exportedNamed = exportedNamed_1
local function wrapped_1()
end
local wrapped = wrapped_1
local function asserted_1()
end
local asserted = asserted_1
local _object = {}
local _left = \"value\"
local function objectNamed(self)
end
_object[_left] = objectNamed
local objectValue = _object
local function first_1()
end
local first = first_1
local function second_1()
end
local second = second_1
local function separatelyExported_1()
end
local separatelyExported = separatelyExported_1
local function callee()
end
callee()
local function wrappedArgument()
end
consume(wrappedArgument)
local function assertedArgument()
end
consume(assertedArgument)
consume(mismatch)
consume(letNamed)
consume(objectValue)
consume(first)
consume(second)
";
	assert_eq!(render_file("functions", "src/namedsweep.ts"), want);
}

/// `TestNamedFunctionExpression_assignment_lifts_inside_the_branch` pins the shape a transformer
/// plugin produces when it hoists a hook argument out of the call. The declaration has to land
/// inside the branch, so the closure is still built only on a cache miss.
#[test]
fn named_function_expression_assignment_lifts_inside_the_branch() {
	let want = "local effect
local dependencies
if cache[1] ~= name then
	dependencies = { name }
	local function namedEffect()
		warn(name)
	end
	effect = namedEffect
	cache[1] = name
	cache[2] = effect
else
	effect = cache[2]
	dependencies = cache[3]
end
useEffect(effect, dependencies)
";
	assert_eq!(render_file("functions", "src/namedassignment.ts"), want);
}

/// a short-circuit operand and a ternary arm are not always evaluated, so the lifted declaration
/// must sit inside the branch the prereq machinery builds, never above it.
#[test]
fn named_function_expression_conditional_operands_stay_conditional() {
	let want = "local _condition = flag
if _condition then
	local function shortNamed()
		return 1
	end
	_condition = shortNamed
end
local short = _condition
local _result
if flag then
	local function thenNamed()
		return 1
	end
	_result = thenNamed
else
	local function elseNamed()
		return 2
	end
	_result = elseNamed
end
local ternary = _result
print(short, ternary)
";
	assert_eq!(render_file("functions", "src/namedconditional.ts"), want);
}

/// `TestNamedFunctionExpression_lifts_into_the_enclosing_scope` covers three positions whose prereqs
/// land in a scope of their own: a parameter default, a loop body, and an arrow expression body.
#[test]
fn named_function_expression_lifts_into_the_enclosing_scope() {
	let want = "local function withDefault(callback)
	if callback == nil then
		local function defaultNamed()
		end
		callback = defaultNamed
	end
	consume(callback)
end
for index = 0, 2 do
	local function loopNamed()
		return index
	end
	consume(loopNamed)
end
local factory = function()
	local function returned()
		return 4
	end
	return returned
end
withDefault()
consume(factory)
";
	assert_eq!(render_file("functions", "src/namedbindingpositions.ts"), want);
}

/// A named function expression: matching const preserves switch case predeclaration.
#[test]
fn named_function_expression_matching_const_preserves_switch_case_predeclaration() {
	let want = "repeat
	local namedFunction
	if selector == 0 then
		function namedFunction()
			return namedFunction
		end
		break
	end
	if selector == 1 then
		-- @ts-expect-error exercising cross-clause predeclaration lowering
		print(namedFunction)
		break
	end
until true
";
	assert_eq!(render_file("functions", "src/namedconstswitch.ts"), want);
}

/// A named function expression: direct call argument does not shadow callee.
#[test]
fn named_function_expression_direct_call_argument_does_not_shadow_callee() {
	let want = "local function useEffect_1()
	return useEffect_1()
end
useEffect(useEffect_1, {})
";
	assert_eq!(render_file("functions", "src/namedcallcalleecollision.ts"), want);
}

/// A named function expression: direct call argument does not shadow outer const.
#[test]
fn named_function_expression_direct_call_argument_does_not_shadow_outer_const() {
	let want = "local collide = 5
local function collide_1(value)
	return if value == 0 then 0 else collide_1(value - 1)
end
consume(collide, collide_1, collide)
print(collide)
";
	assert_eq!(render_file("functions", "src/namedcalloutercollision.ts"), want);
}

/// A named function expression: direct call argument ignores noncolliding generated prefix.
#[test]
fn named_function_expression_direct_call_argument_ignores_noncolliding_generated_prefix() {
	let want = "local function _collide(callback)
	return callback()
end
local function collide()
	return collide()
end
_collide(collide)
";
	assert_eq!(render_file("functions", "src/namedcallgeneratedcollision.ts"), want);
}

/// A named function expression: direct call argument ignores noncolliding ambient prefix.
#[test]
fn named_function_expression_direct_call_argument_ignores_noncolliding_ambient_prefix() {
	let want = "local function collide()
	return collide()
end
_collide(collide)
";
	assert_eq!(render_file("functions", "src/namedcallambientcollision.ts"), want);
}

/// A named function expression: direct call argument ignores noncolliding nested prefix.
#[test]
fn named_function_expression_direct_call_argument_ignores_noncolliding_nested_prefix() {
	let want = "local function collide()
	return collide()
end
consume(collide)
local function later()
	return _collide()
end
";
	assert_eq!(render_file("functions", "src/namedcallnestedcollision.ts"), want);
}

/// Async generators are still unsupported (the name cannot bind to both wrappers). They must not
/// fall back to `noFunctionExpressionName` now that plain async and generator names compile.
#[test]
fn named_function_expression_async_generator_keeps_diagnostic() {
	let diagnostics = transform_expecting_diagnostics("functions", "src/namedunsupported.ts");
	let named_function_diagnostics = diagnostics
		.iter()
		.filter(|diagnostic| diagnostic.code == "noFunctionExpressionName")
		.count();
	let async_generator_diagnostics = diagnostics
		.iter()
		.filter(|diagnostic| diagnostic.code == "noAsyncGeneratorFunctions")
		.count();
	assert_eq!(
		named_function_diagnostics, 0,
		"noFunctionExpressionName diagnostic count; got: {diagnostics:?}"
	);
	assert_eq!(
		async_generator_diagnostics, 1,
		"noAsyncGeneratorFunctions diagnostic count; got: {diagnostics:?}"
	);
}
