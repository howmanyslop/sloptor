//! `functions_test.go`. The expected text is byte for byte what rbxtsc 3.0.0 emits for the same
//! source. `render_file` pins statement lists (header and module return stripped);
//! `render_source_file` pins whole files.

use super::{has_diagnostic, render_file, render_source_file, transform_expecting_diagnostics};

/// Overload signatures have no body and emit nothing; only the implementation compiles.
#[test]
fn bodiless_overloads_dropped() {
	let want = "local function over(a)
	return 1
end
print(over(1), over(\"x\"))
";
	assert_eq!(render_file("functions", "src/overloads.ts"), want);
}

/// `export default function foo` keeps its own name for the local; only the export key is
/// `default`.
#[test]
fn export_default_named_function() {
	let want = "-- Compiled with sloptor v3.0.0-beta.3
local function foo(n)
	return if n == 0 then 1 else n * foo(n - 1)
end
return {
	default = foo,
}
";
	assert_eq!(render_source_file("functions", "src/defaultnamed.ts"), want);
}

/// An anonymous `export default function` is emitted under the literal name `default` and always
/// localized.
#[test]
fn export_default_anonymous_function() {
	let want = "-- Compiled with sloptor v3.0.0-beta.3
local function default(x)
	return x + 10
end
return {
	default = default,
}
";
	assert_eq!(render_source_file("functions", "src/defaultanon.ts"), want);
}

/// Method shorthand and the function expression quirk `{ f: function () {} }` are both implicitly
/// methods: each gets a leading `self` parameter and call sites use `:`.
#[test]
fn object_literal_methods() {
	let want = "local obj = {
	m = function(self, x)
		return x + 1
	end,
	f = function(self, x)
		return x * 2
	end,
}
print(obj:m(1), obj:f(2))
";
	assert_eq!(render_file("functions", "src/methods.ts"), want);
}

/// A localized async function declaration emits `local f = TS.async(function() ... end)`.
#[test]
fn async_function_declaration() {
	let want = "local fetchValue = TS.async(function()
	return 1
end)
print(fetchValue())
";
	assert_eq!(render_file("functions", "src/asyncfn.ts"), want);
}

/// Generator declarations stay real function declarations whose body is
/// `return TS.generator(function() <body> end)`.
#[test]
fn generator_function_declaration() {
	let want = "local function gen()
	return TS.generator(function()
		coroutine.yield(1)
	end)
end
print(gen())
";
	assert_eq!(render_file("functions", "src/genfn.ts"), want);
}

/// `async function*` is upstream's own `noAsyncGeneratorFunctions` error.
#[test]
fn async_generator_functions_diagnostic() {
	let diagnostics = transform_expecting_diagnostics("functions", "src/asyncgen.ts");
	let mut found = false;
	for diagnostic in &diagnostics {
		if diagnostic.code == "noAsyncGeneratorFunctions" {
			found = true;
			assert!(
				diagnostic
					.message
					.contains("Async generator functions are not supported!"),
				"noAsyncGeneratorFunctions message = {:?}, want upstream text",
				diagnostic.message
			);
		}
	}
	assert!(found, "no noAsyncGeneratorFunctions diagnostic; got: {diagnostics:?}");
}

/// An arrow assigned where a method is expected raises `expectedMethodGotFunction`; a function
/// expression assigned where a callback is expected raises `expectedFunctionGotMethod`.
#[test]
fn validate_method_assignment() {
	let diagnostics = transform_expecting_diagnostics("functions", "src/methodmismatch.ts");
	assert!(
		has_diagnostic(
			&diagnostics,
			"expectedMethodGotFunction",
			"Attempted to assign non-method where method was expected."
		),
		"no expectedMethodGotFunction diagnostic; got: {diagnostics:?}"
	);
	assert!(
		has_diagnostic(
			&diagnostics,
			"expectedFunctionGotMethod",
			"Attempted to assign method where non-method was expected."
		),
		"no expectedFunctionGotMethod diagnostic; got: {diagnostics:?}"
	);
}
