//! The transformer's diagnostics (`diagnostics.go`), one factory per upstream factory of
//! `Shared/diagnostics.ts`, in its source order, then rotor's own. Message text is byte-exact.

use rotor_support::gostr::quote;
use rotor_tsrs::P;
use rotor_tsrs::ast::{Kind, Node};

/// One transformer diagnostic (`Diagnostic`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
	/// The upstream factory name, such as `noAny`.
	pub code: &'static str,
	/// The message, its parts joined with `\n`.
	pub message: String,
	/// The node the diagnostic points at, if any.
	pub node: Option<P<Node>>,
	/// Whether this is a warning rather than an error.
	pub warning: bool,
}

const REPO_URL: &str = "https://github.com/roblox-ts/roblox-ts";

/// `suggestion()`: prefixes `Suggestion: ` (upstream's color is presentation only).
fn suggestion(text: &str) -> String {
	format!("Suggestion: {text}")
}

/// `issue()`: the GitHub issue link line.
#[must_use]
pub fn issue(id: u32) -> String {
	format!("More information: {REPO_URL}/issues/{id}")
}

fn error_diag(code: &'static str, node: impl Into<Option<P<Node>>>, messages: &[&str]) -> Diagnostic {
	Diagnostic {
		code,
		message: messages.join("\n"),
		node: node.into(),
		warning: false,
	}
}

fn warning_diag(code: &'static str, node: impl Into<Option<P<Node>>>, messages: &[&str]) -> Diagnostic {
	Diagnostic {
		code,
		message: messages.join("\n"),
		node: node.into(),
		warning: true,
	}
}

macro_rules! simple_errors {
	($($(#[$meta:meta])* $name:ident => $code:literal: [$($message:expr),+ $(,)?];)*) => {
		$(
			$(#[$meta])*
			#[must_use]
			pub fn $name(node: impl Into<Option<P<Node>>>) -> Diagnostic {
				error_diag($code, node, &[$(AsRef::<str>::as_ref(&$message)),+])
			}
		)*
	};
}

simple_errors! {
	/// `noInvalidIdentifier`
	no_invalid_identifier => "noInvalidIdentifier": [
		"Invalid Luau identifier!",
		"Luau identifiers must start with a letter and only contain letters, numbers, and underscores.",
		"Reserved Luau keywords cannot be used as identifiers.",
	];
	/// `noReservedIdentifier`
	no_reserved_identifier => "noReservedIdentifier": ["Cannot use identifier reserved for compiler internal usage."];
	/// `noReservedClassFields`
	no_reserved_class_fields => "noReservedClassFields": ["Cannot use class field reserved for compiler internal usage."];
	/// `noClassMetamethods`
	no_class_metamethods => "noClassMetamethods": ["Metamethods cannot be used in class definitions!"];
	/// `noForInStatement`
	no_for_in_statement => "noForInStatement": ["for-in loop statements are not supported!"];
	/// `noLabeledStatementsWithinTryCatch`: replaces upstream 3.0.0's blanket `noLabeledStatement`;
	/// byte-exact with roblox-ts PR #2928.
	no_labeled_statements_within_try_catch => "noLabeledStatementsWithinTryCatch": [
		"labels are not supported within try/catch blocks!",
	];
	/// `noDebuggerStatement`
	no_debugger_statement => "noDebuggerStatement": ["`debugger` is not supported!"];
	/// `noNullLiteral`
	no_null_literal => "noNullLiteral": ["`null` is not supported!", suggestion("Use `undefined` instead.")];
	/// `noPrivateIdentifier`
	no_private_identifier => "noPrivateIdentifier": ["Private identifiers are not supported!"];
	/// `noTypeOfExpression`
	no_type_of_expression => "noTypeOfExpression": [
		"`typeof` operator is not supported!",
		suggestion("Use `typeIs(value, type)` or `typeOf(value)` instead."),
	];
	/// `noRegex`
	no_regex => "noRegex": ["Regular expressions are not supported!"];
	/// `noBigInt`
	no_big_int => "noBigInt": ["BigInt literals are not supported!"];
	/// `noAny`
	no_any => "noAny": ["Using values of type `any` is not supported!", suggestion("Use `unknown` instead.")];
	/// `noVar`
	no_var => "noVar": ["`var` keyword is not supported!", suggestion("Use `let` or `const` instead.")];
	/// `noGetterSetter`
	no_getter_setter => "noGetterSetter": ["Getters and Setters are not supported!", issue(457)];
	/// `noAutoAccessorModifiers`
	no_auto_accessor_modifiers => "noAutoAccessorModifiers": [
		"Getters and Setters are not supported!",
		"The `accessor` keyword requires generating get/set accessors",
		issue(457),
	];
	/// `noEqualsEquals`
	no_equals_equals => "noEqualsEquals": ["operator `==` is not supported!", suggestion("Use `===` instead.")];
	/// `noExclamationEquals`
	no_exclamation_equals => "noExclamationEquals": [
		"operator `!=` is not supported!",
		suggestion("Use `!==` instead."),
	];
	/// `noEnumMerging`
	no_enum_merging => "noEnumMerging": ["Enum merging is not supported!"];
	/// `noNamespaceMerging`
	no_namespace_merging => "noNamespaceMerging": ["Namespace merging is not supported!"];
	/// `noNestedSpreadsInAssignmentPatterns`
	no_nested_spreads_in_assignment_patterns => "noNestedSpreadsInAssignmentPatterns": [
		"Nesting spreads in assignment patterns is not supported!",
	];
	/// `noRestSpreadingOfRobloxTypes`
	no_rest_spreading_of_roblox_types => "noRestSpreadingOfRobloxTypes": [
		"Operator `...` is not allowed on Roblox types!",
	];
	/// `noFunctionExpressionName`: upstream's error; rotor compiles named function expressions and
	/// never emits it.
	no_function_expression_name => "noFunctionExpressionName": ["Function expression names are not supported!"];
	/// `noPrecedingSpreadElement`
	no_preceding_spread_element => "noPrecedingSpreadElement": [
		"Spread element must come last in a list of arguments!",
	];
	/// `noLuaTupleDestructureAssignmentExpression`
	no_lua_tuple_destructure_assignment_expression => "noLuaTupleDestructureAssignmentExpression": [
		"Cannot destructure LuaTuple<T> expression outside of an ExpressionStatement!",
	];
	/// `noExportAssignmentLet`
	no_export_assignment_let => "noExportAssignmentLet": [
		"Cannot use `export =` on a `let` variable!",
		suggestion("Use `const` instead."),
	];
	/// `noGlobalThis`
	no_global_this => "noGlobalThis": ["`globalThis` is not supported!"];
	/// `noArguments`
	no_arguments => "noArguments": ["`arguments` is not supported!"];
	/// `noPrototype`
	no_prototype => "noPrototype": ["`prototype` is not supported!"];
	/// `noRobloxSymbolInstanceof`: upstream's suggestion lacks the closing backtick and period;
	/// kept byte-exact.
	no_roblox_symbol_instanceof => "noRobloxSymbolInstanceof": [
		"The `instanceof` operator can only be used on roblox-ts classes!",
		suggestion("Use `typeIs(myThing, \"TypeToCheck\") instead"),
	];
	/// `noNonNumberStringRelationOperator`
	no_non_number_string_relation_operator => "noNonNumberStringRelationOperator": [
		"Relation operators can only be used on number or string types!",
	];
	/// `noInstanceMethodCollisions`
	no_instance_method_collisions => "noInstanceMethodCollisions": [
		"Static methods cannot use the same name as instance methods!",
	];
	/// `noStaticMethodCollisions`
	no_static_method_collisions => "noStaticMethodCollisions": [
		"Instance methods cannot use the same name as static methods!",
	];
	/// `noUnaryPlus`
	no_unary_plus => "noUnaryPlus": ["Unary `+` is not supported!", suggestion("Use `tonumber(x)` instead.")];
	/// `noNonNumberUnaryMinus`
	no_non_number_unary_minus => "noNonNumberUnaryMinus": ["Unary `-` is only supported for number types!"];
	/// `noAwaitForOf`
	no_await_for_of => "noAwaitForOf": ["`await` is not supported in for-of loops!"];
	/// `noAsyncGeneratorFunctions`
	no_async_generator_functions => "noAsyncGeneratorFunctions": ["Async generator functions are not supported!"];
	/// `noNonStringModuleSpecifier`
	no_non_string_module_specifier => "noNonStringModuleSpecifier": ["Module specifiers must be a string literal."];
	/// `noIterableIteration`
	no_iterable_iteration => "noIterableIteration": [
		"Iterating on Iterable<T> is not supported! You must use a more specific type.",
	];
	/// `noMixedTypeCall`
	no_mixed_type_call => "noMixedTypeCall": [
		"Attempted to call a function with mixed types! All definitions must either be a method or a callback.",
	];
	/// `noIndexWithoutCall`
	no_index_without_call => "noIndexWithoutCall": [
		"Cannot index a method without calling it!",
		suggestion("Use the form `() => a.b()` instead of `a.b`."),
	];
	/// `noCommentDirectives`
	no_comment_directives => "noCommentDirectives": [
		"Usage of `@ts-ignore`, `@ts-expect-error`, and `@ts-nocheck` are not supported!",
		"roblox-ts needs type and symbol info to compile correctly.",
		suggestion("Consider using type assertions or `declare` statements."),
	];
	/// `noOptionalMacroCall`
	no_optional_macro_call => "noOptionalMacroCall": [
		"Macro methods can not be optionally called!",
		suggestion("Macros always exist. Use a normal call."),
	];
	/// `noConstructorMacroWithoutNew`
	no_constructor_macro_without_new => "noConstructorMacroWithoutNew": [
		"Cannot index a constructor macro without using the `new` operator!",
	];
	/// `noMacroExtends`
	no_macro_extends => "noMacroExtends": [
		"Cannot extend from a macro class!",
		suggestion("Store an instance of the macro class in a property."),
	];
	/// `noMacroUnion`
	no_macro_union => "noMacroUnion": ["Macro cannot be applied to a union type!"];
	/// `noMacroObjectSpread`
	no_macro_object_spread => "noMacroObjectSpread": [
		"Macro classes cannot be used in an object spread!",
		suggestion("Did you mean to use an array spread? `[ ...exp ]`"),
	];
	/// `noVarArgsMacroSpread`
	no_var_args_macro_spread => "noVarArgsMacroSpread": [
		"Macros which use variadic arguments do not support spread expressions!",
		issue(1149),
	];
	/// `noRangeMacroOutsideForOf`
	no_range_macro_outside_for_of => "noRangeMacroOutsideForOf": [
		"$range() macro is only valid as an expression of a for-of loop!",
	];
	/// `noTupleMacroOutsideReturn`
	no_tuple_macro_outside_return => "noTupleMacroOutsideReturn": [
		"$tuple() macro is only valid as an expression of a return statement!",
	];
	/// `noModuleSpecifierFile`
	no_module_specifier_file => "noModuleSpecifierFile": [
		"Could not find file for import. Did you forget to `npm install`?",
	];
	/// `noInvalidModule`
	no_invalid_module => "noInvalidModule": ["You can only use npm scopes that are listed in your typeRoots."];
	/// `noUnscopedModule`
	no_unscoped_module => "noUnscopedModule": ["You cannot use modules directly under node_modules."];
	/// `noNonModuleImport`
	no_non_module_import => "noNonModuleImport": ["Cannot import a non-ModuleScript!"];
	/// `noIsolatedImport`
	no_isolated_import => "noIsolatedImport": [
		"Attempted to import a file inside of an isolated container from outside!",
	];
	/// `noServerImport`
	no_server_import => "noServerImport": [
		"Cannot import a server file from a shared or client location!",
		suggestion("Move the file you want to import to a shared location."),
	];
	/// `noPrecedingJsxSpreadElement`
	no_preceding_jsx_spread_element => "noPrecedingJsxSpreadElement": [
		"JSX spread expression must come last in children!",
	];
	/// `expectedMethodGotFunction`
	expected_method_got_function => "expectedMethodGotFunction": [
		"Attempted to assign non-method where method was expected.",
	];
	/// `expectedFunctionGotMethod`
	expected_function_got_method => "expectedFunctionGotMethod": [
		"Attempted to assign method where non-method was expected.",
	];
	/// `noUnstableThisType`
	no_unstable_this_type => "noUnstableThisType": [
		"The generic this type can become void, changing whether a receiver is passed.",
		suggestion("Use this: void for callbacks, or constrain the receiver type to exclude void."),
	];
}

/// `noRojoData`: the formatted context line, plus the suggestion for packages.
#[must_use]
pub fn no_rojo_data(node: impl Into<Option<P<Node>>>, path: &str, is_package: bool) -> Diagnostic {
	let first = format!("Could not find Rojo data. There is no $path in your Rojo config that covers {path}");
	if is_package {
		let hint = suggestion("Did you forget to add a custom npm scope to your default.project.json?");
		return error_diag("noRojoData", node, &[&first, &hint]);
	}
	error_diag("noRojoData", node, &[&first])
}

/// `noPackageImportWithoutScope`
#[must_use]
pub fn no_package_import_without_scope(
	node: impl Into<Option<P<Node>>>,
	path: &str,
	rbx_path: &[String],
) -> Diagnostic {
	error_diag(
		"noPackageImportWithoutScope",
		node,
		&[
			"Imported package Roblox path is missing an npm scope!",
			&format!("Package path: {path}"),
			&format!("Roblox path: {}", rbx_path.join(".")),
			&suggestion(
				"You might need to update your \"node_modules\" in default.project.json to match:\n\
				\"node_modules\": {\n\t\"$className\": \"Folder\",\n\t\"@rbxts\": {\n\t\t\"$path\": \"node_modules/@rbxts\"\n\t}\n}",
			),
		],
	)
}

/// `incorrectFileName`: a project-level text diagnostic with no node.
#[must_use]
pub fn incorrect_file_name(original_file_name: &str, suggested_file_name: &str, full_path: &str) -> Diagnostic {
	error_diag(
		"incorrectFileName",
		None,
		&[
			&format!("Incorrect file name: `{original_file_name}`!"),
			&format!("Full path: {full_path}"),
			&suggestion(&format!("Change `{original_file_name}` to `{suggested_file_name}`.")),
		],
	)
}

/// `rojoPathInSrc`: upstream interpolates the paths raw inside literal quotes.
#[must_use]
pub fn rojo_path_in_src(partition_path: &str, suggested_path: &str) -> Diagnostic {
	error_diag(
		"rojoPathInSrc",
		None,
		&[
			"Invalid Rojo configuration. $path fields should be relative to out directory.",
			&suggestion(&format!(
				"Change the value of $path from \"{partition_path}\" to \"{suggested_path}\"."
			)),
		],
	)
}

/// `rotorNotYetSupported`: a construct upstream compiles but rotor has not ported yet.
#[must_use]
pub fn rotor_not_yet_supported(node: impl Into<Option<P<Node>>>, what: &str) -> Diagnostic {
	error_diag(
		"rotorNotYetSupported",
		node,
		&[&format!("rotor: {what} not yet supported")],
	)
}

/// `rotorNotYetPorted`: a construct the Go compiler handles whose transform the Rust port has not
/// reached yet (a Stage 4b slice stub). Rust-only; no Go output ever contains it.
#[must_use]
pub fn rotor_not_yet_ported(node: impl Into<Option<P<Node>>>, what: &str) -> Diagnostic {
	error_diag(
		"rotorNotYetPorted",
		node,
		&[&format!("rotor: not yet ported to Rust: {what}")],
	)
}

/// `rotorUnreachableHoistedFunction`
#[must_use]
pub fn rotor_unreachable_hoisted_function(node: P<Node>) -> Diagnostic {
	let name = node.name().map_or("", |name| name.text());
	error_diag(
		"rotorUnreachableHoistedFunction",
		node,
		&[
			&format!("Function {} is unreachable after a final statement.", quote(name)),
			"Declare the function before the first statement that uses it.",
		],
	)
}

/// `rotorUnknownLabel`: only reachable when TypeScript's own error was suppressed.
#[must_use]
pub fn rotor_unknown_label(node: P<Node>) -> Diagnostic {
	error_diag(
		"rotorUnknownLabel",
		node,
		&[&format!("rotor: no enclosing label named `{}`", node.text())],
	)
}

simple_errors! {
	/// `rotorLabeledStatementFlowControl`
	rotor_labeled_statement_flow_control => "rotorLabeledStatementFlowControl": [
		"rotor: a label on a non-loop statement cannot contain an unlabeled `break`/`continue` that targets an enclosing loop or switch",
		suggestion("Label the enclosing loop and use `break <label>` / `continue <label>` instead."),
	];
	/// `rotorLabeledContinueInDoWhile`
	rotor_labeled_continue_in_do_while => "rotorLabeledContinueInDoWhile": [
		"rotor: `continue <label>` cannot target a do/while loop whose condition needs temporaries — Luau forbids `continue` before those locals",
		suggestion("Move the condition into the loop body, or restructure as a `while` loop."),
	];
	/// `rotorNoProjectContext`
	rotor_no_project_context => "rotorNoProjectContext": [
		"rotor: imports require project context (no Rojo resolver attached)",
	];
	/// `rotorEnvNonLiteralArg`
	rotor_env_non_literal_arg => "rotorEnvNonLiteralArg": [
		"rotor: $env arguments must be string literals — the value is inlined at compile time",
		suggestion("Use `$env(\"NAME\")`, `$env(\"NAME\", \"fallback\")`, `$env.NAME`, or `$env[\"NAME\"]`."),
	];
	/// `rotorEnvBadUsage`
	rotor_env_bad_usage => "rotorEnvBadUsage": [
		"rotor: $env must be used as `$env(\"NAME\")`, `$env(\"NAME\", \"fallback\")`, `$env.NAME`, or `$env[\"NAME\"]`",
	];
	/// `rotorNameofInvalid`
	rotor_nameof_invalid => "rotorNameofInvalid": [
		"rotor: $nameof requires an expression ending in an identifier or property access",
		suggestion("Use `$nameof(foo)` or `$nameof(a.b.c)` — the trailing name is inlined as a string."),
	];
	/// `rotorNameofBadUsage`
	rotor_nameof_bad_usage => "rotorNameofBadUsage": ["rotor: $nameof must be called as `$nameof(expr)`"];
	/// `rotorKeysNoType`
	rotor_keys_no_type => "rotorKeysNoType": [
		"rotor: $keys requires a single type argument whose string keys are inlined",
		suggestion("Use `$keys<{ x: number; y: string }>()`."),
	];
	/// `rotorKeysBadUsage`
	rotor_keys_bad_usage => "rotorKeysBadUsage": ["rotor: $keys must be called as `$keys<T>()`"];
	/// `rotorFileNonLiteralArg`
	rotor_file_non_literal_arg => "rotorFileNonLiteralArg": [
		"rotor: $file path must be a string literal — the contents are read and inlined at compile time",
		suggestion("Use `$file(\"config.json\")`."),
	];
	/// `rotorFileBadUsage`
	rotor_file_bad_usage => "rotorFileBadUsage": [
		"rotor: $file must be called as `$file(\"path/to/file.json\")`",
	];
	/// `rotorFileNoResolver`
	rotor_file_no_resolver => "rotorFileNoResolver": [
		"rotor: $file requires project context (no file resolver attached)",
	];
}

/// `rotorFileNotFound`
#[must_use]
pub fn rotor_file_not_found(node: impl Into<Option<P<Node>>>, path: &str) -> Diagnostic {
	error_diag(
		"rotorFileNotFound",
		node,
		&[&format!("rotor: file {} does not exist", quote(path))],
	)
}

/// `rotorFileInvalidJSON`
#[must_use]
pub fn rotor_file_invalid_json(node: impl Into<Option<P<Node>>>, path: &str, message: &str) -> Diagnostic {
	error_diag(
		"rotorFileInvalidJSON",
		node,
		&[&format!("rotor: file {} is not valid JSON: {message}", quote(path))],
	)
}

/// `kindName`: tsgo's stringer name without its `Kind` prefix, as upstream's `getKindName`
/// prints it.
#[must_use]
pub fn kind_name(kind: Kind) -> String {
	format!("{kind:?}")
}

/// `truthyChange` (a warning).
#[must_use]
pub fn truthy_change(node: impl Into<Option<P<Node>>>, checks: &str) -> Diagnostic {
	warning_diag(
		"truthyChange",
		node,
		&[&format!("Value will be checked against {checks}")],
	)
}

/// `runtimeLibUsedInReplicatedFirst` (a warning).
#[must_use]
pub fn runtime_lib_used_in_replicated_first(node: Option<P<Node>>) -> Diagnostic {
	warning_diag(
		"runtimeLibUsedInReplicatedFirst",
		node,
		&[
			"This statement would generate a call to the runtime library. The runtime library should not be used from ReplicatedFirst.",
		],
	)
}
