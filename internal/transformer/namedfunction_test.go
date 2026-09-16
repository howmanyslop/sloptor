package transformer_test

import (
	"testing"
)

// TestNamedFunctionExpression_mismatched_const_lifts_the_name: `const f =
// function named() {}` is upstream's noFunctionExpressionName error. rotor
// lifts the name into a local function declaration instead, because that is
// the only Luau form that carries a debug name into a traceback — an
// assignment of a function expression, to a local or anything else, compiles
// to an unnamed closure. rbxtsc aborts emission on the error, so there is no
// oracle for the output shape.
func TestNamedFunctionExpression_mismatched_const_lifts_the_name(t *testing.T) {
	want := `local function named(x)
	return x
end
local f = named
print(f(1))
`
	if got := renderFunctionsFile(t, "src/namedexpr.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestNamedFunctionExpression_direct_call_argument_emits_local_declaration(t *testing.T) {
	want := `local function doSomeBullshit()
end
useEffect(doSomeBullshit, {})
`
	if got := renderFunctionsFile(t, "src/namedcallarg.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestNamedFunctionExpression_matching_const_emits_recursive_local_declaration(t *testing.T) {
	want := `local function namedFunction(value)
	return if value == 0 then 0 else namedFunction(value - 1)
end
print(namedFunction(2))
`
	if got := renderFunctionsFile(t, "src/namedconst.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestNamedFunctionExpression_direct_call_argument_preserves_recursion_and_argument_order(t *testing.T) {
	want := `local function before()
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
`
	if got := renderFunctionsFile(t, "src/namedcallordering.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestNamedFunctionExpression_async_call_argument_wraps_the_named_local:
// `registerOnClose(async function onBindToCloseAsync() {})` used to fail
// with noFunctionExpressionName. The name is the TS.async wrapper, same
// shape as a hoisted async declaration, so a caller (and a self-call)
// get a Promise.
func TestNamedFunctionExpression_async_call_argument_wraps_the_named_local(t *testing.T) {
	want := `local onBindToCloseAsync
onBindToCloseAsync = TS.async(function() end)
local stopBindToClose = registerOnClose(onBindToCloseAsync)
print(stopBindToClose)
`
	if got := renderFunctionsFile(t, "src/namedasyncarg.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestNamedFunctionExpression_async_matching_const_binds_the_wrapper:
// a self-call through the function name must hit TS.async, not the raw
// body. The local is declared before the wrapper is assigned so Lua
// captures that local, not a global. Matching names fold into one local;
// a mismatched const still lifts the expression name and aliases it.
func TestNamedFunctionExpression_async_matching_const_binds_the_wrapper(t *testing.T) {
	want := `local named
named = TS.async(function(value)
	return if value == 0 then 0 else named(value - 1)
end)
local different
different = TS.async(function() end)
local f = different
print(named(2), f)
`
	if got := renderFunctionsFile(t, "src/namedasyncconst.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestNamedFunctionExpression_generator_lifts_as_a_declaration: a named
// generator expression is the factory, same as `function* named()`. The
// name binds to that factory, so a self-call creates a new generator
// instead of re-entering the iterator callback.
func TestNamedFunctionExpression_generator_lifts_as_a_declaration(t *testing.T) {
	want := `local function named()
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
`
	if got := renderFunctionsFile(t, "src/namedgenerator.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestNamedFunctionExpression_async_conditional_operands_stay_conditional:
// constructing the wrapper is work, so a short-circuit operand and a
// ternary arm must keep the local and the TS.async assignment inside
// the branch.
func TestNamedFunctionExpression_async_conditional_operands_stay_conditional(t *testing.T) {
	want := `local _condition = flag
if _condition then
	local shortNamed
	shortNamed = TS.async(function()
		return 1
	end)
	_condition = shortNamed
end
local short = _condition
local _result
if flag then
	local thenNamed
	thenNamed = TS.async(function()
		return 1
	end)
	_result = thenNamed
else
	local elseNamed
	elseNamed = TS.async(function()
		return 2
	end)
	_result = elseNamed
end
local ternary = _result
print(short, ternary)
`
	if got := renderFunctionsFile(t, "src/namedasyncconditional.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestNamedFunctionExpression_async_generator_keeps_diagnostic: async
// generators are still unsupported (the name cannot bind to both wrappers).
// They must not fall back to noFunctionExpressionName now that plain async
// and generator names compile.
func TestNamedFunctionExpression_async_generator_keeps_diagnostic(t *testing.T) {
	diagnostics := transformExpectingDiagnostics(t, "src/namedunsupported.ts")
	namedFunctionDiagnostics := 0
	asyncGeneratorDiagnostics := 0
	for _, diagnostic := range diagnostics {
		switch diagnostic.Code {
		case "noFunctionExpressionName":
			namedFunctionDiagnostics++
		case "noAsyncGeneratorFunctions":
			asyncGeneratorDiagnostics++
		}
	}
	if namedFunctionDiagnostics != 0 {
		t.Errorf("noFunctionExpressionName diagnostic count = %d, want 0; got: %v", namedFunctionDiagnostics, diagnostics)
	}
	if asyncGeneratorDiagnostics != 1 {
		t.Errorf("noAsyncGeneratorFunctions diagnostic count = %d, want 1; got: %v", asyncGeneratorDiagnostics, diagnostics)
	}
}

// TestNamedFunctionExpression_lifts_from_every_synchronous_position sweeps the
// positions that used to report the diagnostic: a const whose name differs
// from the binding, a `let`, an export, an assertion, an object-literal
// value, one of several declarators, an IIFE callee, and a call argument
// behind an assertion. A colliding name is renamed rather than shadowing the
// binding it collides with.
func TestNamedFunctionExpression_lifts_from_every_synchronous_position(t *testing.T) {
	want := `local function different()
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
local _left = "value"
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
`
	if got := renderFunctionsFile(t, "src/namedsweep.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestNamedFunctionExpression_assignment_lifts_inside_the_branch pins the
// shape a transformer plugin produces when it hoists a hook argument out of
// the call. The declaration has to land inside the branch, so the closure is
// still built only on a cache miss.
func TestNamedFunctionExpression_assignment_lifts_inside_the_branch(t *testing.T) {
	want := `local effect
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
`
	if got := renderFunctionsFile(t, "src/namedassignment.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestNamedFunctionExpression_conditional_operands_stay_conditional: a
// short-circuit operand and a ternary arm are not always evaluated, so the
// lifted declaration must sit inside the branch the prereq machinery builds,
// never above it.
func TestNamedFunctionExpression_conditional_operands_stay_conditional(t *testing.T) {
	want := `local _condition = flag
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
`
	if got := renderFunctionsFile(t, "src/namedconditional.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

// TestNamedFunctionExpression_lifts_into_the_enclosing_scope covers three
// positions whose prereqs land in a scope of their own: a parameter default,
// a loop body, and an arrow expression body.
func TestNamedFunctionExpression_lifts_into_the_enclosing_scope(t *testing.T) {
	want := `local function withDefault(callback)
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
`
	if got := renderFunctionsFile(t, "src/namedbindingpositions.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestNamedFunctionExpression_matching_const_preserves_switch_case_predeclaration(t *testing.T) {
	want := `repeat
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
`
	if got := renderFunctionsFile(t, "src/namedconstswitch.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestNamedFunctionExpression_direct_call_argument_does_not_shadow_callee(t *testing.T) {
	want := `local function useEffect_1()
	return useEffect_1()
end
useEffect(useEffect_1, {})
`
	if got := renderFunctionsFile(t, "src/namedcallcalleecollision.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestNamedFunctionExpression_direct_call_argument_does_not_shadow_outer_const(t *testing.T) {
	want := `local collide = 5
local function collide_1(value)
	return if value == 0 then 0 else collide_1(value - 1)
end
consume(collide, collide_1, collide)
print(collide)
`
	if got := renderFunctionsFile(t, "src/namedcalloutercollision.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestNamedFunctionExpression_direct_call_argument_ignores_noncolliding_generated_prefix(t *testing.T) {
	want := `local function _collide(callback)
	return callback()
end
local function collide()
	return collide()
end
_collide(collide)
`
	if got := renderFunctionsFile(t, "src/namedcallgeneratedcollision.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestNamedFunctionExpression_direct_call_argument_ignores_noncolliding_ambient_prefix(t *testing.T) {
	want := `local function collide()
	return collide()
end
_collide(collide)
`
	if got := renderFunctionsFile(t, "src/namedcallambientcollision.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}

func TestNamedFunctionExpression_direct_call_argument_ignores_noncolliding_nested_prefix(t *testing.T) {
	want := `local function collide()
	return collide()
end
consume(collide)
local function later()
	return _collide()
end
`
	if got := renderFunctionsFile(t, "src/namedcallnestedcollision.ts"); got != want {
		t.Errorf("rendered output:\ngot:\n%s\nwant:\n%s", got, want)
	}
}
