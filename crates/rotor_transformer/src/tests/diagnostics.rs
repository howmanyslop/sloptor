//! `diagnostics_test.go`. Go passes `new(ast.Node)` or nil; the factories take an optional node.

use std::collections::HashSet;

use crate::diagnostics::{self as d, Diagnostic};
use crate::diagservice::DiagService;

#[test]
fn diagnostic_messages() {
	let cases: Vec<(Diagnostic, &str, &str, bool)> = vec![
		(
			d::no_any(None),
			"noAny",
			"Using values of type `any` is not supported!\nSuggestion: Use `unknown` instead.",
			false,
		),
		(
			d::no_null_literal(None),
			"noNullLiteral",
			"`null` is not supported!\nSuggestion: Use `undefined` instead.",
			false,
		),
		(
			d::no_for_in_statement(None),
			"noForInStatement",
			"for-in loop statements are not supported!",
			false,
		),
		(
			d::no_var(None),
			"noVar",
			"`var` keyword is not supported!\nSuggestion: Use `let` or `const` instead.",
			false,
		),
		(
			d::no_getter_setter(None),
			"noGetterSetter",
			"Getters and Setters are not supported!\nMore information: https://github.com/roblox-ts/roblox-ts/issues/457",
			false,
		),
		(
			d::no_invalid_identifier(None),
			"noInvalidIdentifier",
			"Invalid Luau identifier!\nLuau identifiers must start with a letter and only contain letters, numbers, and underscores.\nReserved Luau keywords cannot be used as identifiers.",
			false,
		),
		(
			// Upstream's suggestion text genuinely lacks the closing backtick and trailing period.
			d::no_roblox_symbol_instanceof(None),
			"noRobloxSymbolInstanceof",
			"The `instanceof` operator can only be used on roblox-ts classes!\nSuggestion: Use `typeIs(myThing, \"TypeToCheck\") instead",
			false,
		),
		(
			d::no_equals_equals(None),
			"noEqualsEquals",
			"operator `==` is not supported!\nSuggestion: Use `===` instead.",
			false,
		),
		(
			d::no_var_args_macro_spread(None),
			"noVarArgsMacroSpread",
			"Macros which use variadic arguments do not support spread expressions!\nMore information: https://github.com/roblox-ts/roblox-ts/issues/1149",
			false,
		),
		(
			d::no_function_expression_name(None),
			"noFunctionExpressionName",
			"Function expression names are not supported!",
			false,
		),
		(
			d::truthy_change(None, "0, 0/0, \"\""),
			"truthyChange",
			"Value will be checked against 0, 0/0, \"\"",
			true,
		),
		(
			d::runtime_lib_used_in_replicated_first(None),
			"runtimeLibUsedInReplicatedFirst",
			"This statement would generate a call to the runtime library. The runtime library should not be used from ReplicatedFirst.",
			true,
		),
	];
	for (diagnostic, code, message, warning) in cases {
		assert_eq!(diagnostic.code, code);
		assert_eq!(diagnostic.message, message, "{code}");
		assert_eq!(diagnostic.warning, warning, "{code}");
	}
}

#[test]
fn diagnostic_context_messages() {
	// noRojoData: the suggestion line is included only when isPackage is true
	let diagnostic = d::no_rojo_data(None, "src/shared/module.ts", false);
	let mut want =
		"Could not find Rojo data. There is no $path in your Rojo config that covers src/shared/module.ts".to_owned();
	assert_eq!(diagnostic.message, want);
	let diagnostic = d::no_rojo_data(None, "src/shared/module.ts", true);
	want.push_str("\nSuggestion: Did you forget to add a custom npm scope to your default.project.json?");
	assert_eq!(diagnostic.message, want);

	let rbx_path = ["ReplicatedStorage", "node_modules", "foo"].map(str::to_owned);
	let diagnostic = d::no_package_import_without_scope(None, "node_modules/foo", &rbx_path);
	let want = "Imported package Roblox path is missing an npm scope!\n\
		Package path: node_modules/foo\n\
		Roblox path: ReplicatedStorage.node_modules.foo\n\
		Suggestion: You might need to update your \"node_modules\" in default.project.json to match:\n\
		\"node_modules\": {\n\t\"$className\": \"Folder\",\n\t\"@rbxts\": {\n\t\t\"$path\": \"node_modules/@rbxts\"\n\t}\n}";
	assert_eq!(diagnostic.message, want);
}

#[test]
fn spread_diagnostics() {
	let diagnostic = d::no_nested_spreads_in_assignment_patterns(None);
	assert_eq!(diagnostic.code, "noNestedSpreadsInAssignmentPatterns");
	assert_eq!(
		diagnostic.message,
		"Nesting spreads in assignment patterns is not supported!"
	);
	let diagnostic = d::no_rest_spreading_of_roblox_types(None);
	assert_eq!(diagnostic.code, "noRestSpreadingOfRobloxTypes");
	assert_eq!(diagnostic.message, "Operator `...` is not allowed on Roblox types!");
}

#[test]
fn diag_service_add_single_dedupe() {
	let mut service = DiagService::new();
	service.add(d::no_var(None));
	service.add(d::no_var(None)); // Add never dedupes
	service.add_single(d::no_any(None));
	service.add_single(d::no_any(None)); // second occurrence ignored
	service.add_single(d::no_var(None)); // a different code through AddSingle is still added

	let codes: Vec<&str> = service.flush().iter().map(|diagnostic| diagnostic.code).collect();
	assert_eq!(codes, ["noVar", "noVar", "noAny", "noVar"]);
}

#[test]
fn diag_service_flush_clears() {
	let mut service = DiagService::new();
	service.add_single(d::no_any(None));
	assert_eq!(service.flush().len(), 1);
	assert_eq!(service.flush().len(), 0);
	// Flush also resets the AddSingle dedupe set.
	service.add_single(d::no_any(None));
	assert_eq!(service.flush().len(), 1);
}

#[test]
fn diag_service_has_errors() {
	let mut service = DiagService::new();
	assert!(!service.has_errors(), "empty service must not have errors");
	service.add(d::truthy_change(None, "0"));
	assert!(!service.has_errors(), "warnings alone must not count as errors");
	service.add(d::no_any(None));
	assert!(service.has_errors(), "error diagnostic must be detected");
	service.flush();
	assert!(!service.has_errors(), "HasErrors must be false after Flush");
}

#[test]
fn add_diagnostic_with_cache() {
	let mut service = DiagService::new();
	let mut cache = HashSet::new();
	service.add_with_cache("keyA", d::no_any(None), &mut cache);
	service.add_with_cache("keyA", d::no_any(None), &mut cache); // deduped by the caller's cache
	service.add_with_cache("keyB", d::no_any(None), &mut cache); // a distinct key passes
	assert_eq!(service.flush().len(), 2);
}
