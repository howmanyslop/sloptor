//! `luautypes_typefunction_test.go`. Go reads every body off the whole rendered file, which
//! needs slice ii; those ports wait for it. The bodies, fallbacks and prelude are also checked
//! here straight from the registry, so the translator is covered now.

use std::collections::HashMap;

use super::super::luautypes::types;
use super::super::luautypes_alias::entry_statements;
use super::super::luautypes_typefunction_prelude::require_prelude;
use super::analyze::{analyze_luau_strict, find_luau_analyzer, luau_error_lines};
use super::{assert_lines_in_order, render_luau_types_losses, with_fixture};
use crate::tests::render_list;

const HELPER_DISTRIBUTE_TEXT: &str = r#"	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
"#;

const HELPER_JOIN_TEXT: &str = r"	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
";

const HELPER_WITHOUT_NIL_TEXT: &str = r#"	local function withoutnil(value: type): type
		local parts = {}
		for _, component in distribute(value) do
			if not component:is("nil") then
				table.insert(parts, component)
			end
		end
		return join(parts)
	end
"#;

const HELPER_KEYS_TEXT: &str = r#"	local function keys(value: type): type
		if not value:is("table") then
			error("keyof: expected a table type")
		end
		local parts = {}
		for key in value:properties() do
			table.insert(parts, key)
		end
		return join(parts)
	end
"#;

const HELPER_INDEX_OF_TEXT: &str = r#"	local function indexof(object: type, key: type): type
		if key:is("union") then
			local parts = {}
			for _, component in key:components() do
				table.insert(parts, indexof(object, component))
			end
			return join(parts)
		end
		if not object:is("table") then
			error("indexed access: expected a table type")
		end
		local value = object:readproperty(key)
		if not value then
			error("indexed access: the property does not exist")
		end
		return value
	end
"#;

const HELPER_TEMPLATE_TEXT: &str = r#"	local function template(parts: { string | type }): type
		local text = ""
		for _, part in parts do
			if type(part) == "string" then
				text = text .. part
			elseif part:is("singleton") then
				text = text .. tostring(part:value())
			else
				return types.string
			end
		end
		return types.singleton(text)
	end
"#;

const HELPER_RETURN_OF_TEXT: &str = r"	local function returnof(fn: type): type
		local returns = fn:returns()
		if returns.head and returns.head[1] then
			return returns.head[1]
		end
		if returns.tail then
			return returns.tail
		end
		return types.singleton(nil)
	end
";

/// `partial2Body`: the plan's worked example, verbatim.
const PARTIAL2_BODY: &str = r#"export type function Partial2(T: type)
	if not T:is("table") then
		return T
	end
	local result = types.copy(T)
	for key, prop in T:properties() do
		if prop.read then
			result:setreadproperty(key, types.optional(prop.read))
		end
		if prop.write then
			result:setwriteproperty(key, types.optional(prop.write))
		end
	end
	return result
end
"#;

/// `typeFunctionBodies`: the complete expected blocks, keyed by alias.
#[expect(clippy::too_many_lines, reason = "the Go table, kept whole")]
fn type_function_bodies() -> Vec<(&'static str, String)> {
	vec![
		("Partial2", PARTIAL2_BODY.to_owned()),
		// `-?` strips nil through the withoutnil helper.
		(
			"Required2",
			format!(
				"export type function Required2(T: type)\n{HELPER_DISTRIBUTE_TEXT}{HELPER_JOIN_TEXT}{HELPER_WITHOUT_NIL_TEXT}{}",
				r#"	if not T:is("table") then
		return T
	end
	local result = types.copy(T)
	for key, prop in T:properties() do
		if prop.read then
			result:setreadproperty(key, withoutnil(prop.read))
		end
		if prop.write then
			result:setwriteproperty(key, withoutnil(prop.write))
		end
	end
	return result
end
"#
			),
		),
		// `readonly` keeps the read slot and clears the write slot.
		(
			"Readonly2",
			r#"export type function Readonly2(T: type)
	if not T:is("table") then
		return T
	end
	local result = types.copy(T)
	for key, prop in T:properties() do
		local current = prop.read or prop.write
		if current then
			result:setreadproperty(key, current)
			result:setwriteproperty(key, nil)
		end
	end
	return result
end
"#
			.to_owned(),
		),
		// A conditional over a naked T distributes; `(infer U)[]` is the number indexer's read
		// result.
		(
			"Elem",
			format!(
				"export type function Elem(T: type)\n{HELPER_DISTRIBUTE_TEXT}{HELPER_JOIN_TEXT}{}",
				r#"	local parts = {}
	for _, item in distribute(T) do
		local indexer = if item:is("table") then item:indexer() else nil
		if indexer and indexer.index == types.number then
			table.insert(parts, indexer.readresult)
		else
			table.insert(parts, item)
		end
	end
	return join(parts)
end
"#
			),
		),
		// A mapped type over a key union builds a new table; the `as` clause is a template over
		// the key, and `() => void` an empty function type.
		(
			"Handlers",
			format!(
				"export type function Handlers(T: type)\n{HELPER_DISTRIBUTE_TEXT}{HELPER_TEMPLATE_TEXT}{}",
				r#"	local result = types.newtable()
	for _, key in distribute(T) do
		local name = template({ "on", key })
		local value = types.newfunction({}, {})
		if name:is("singleton") then
			result:setproperty(name, value)
		elseif name:is("string") or name:is("number") then
			result:setindexer(name, value)
		elseif not name:is("never") then
			error("Handlers: a mapped key must be a string literal, string or number")
		end
	end
	return result
end
"#
			),
		),
		// A static extends type is a subtyping test; literal branches are singletons.
		(
			"Stringy",
			format!(
				"export type function Stringy(T: type)\n{HELPER_DISTRIBUTE_TEXT}{HELPER_JOIN_TEXT}{}",
				r#"	local parts = {}
	for _, item in distribute(T) do
		if item:issubtypeof(types.string) then
			table.insert(parts, types.singleton("yes"))
		else
			table.insert(parts, types.singleton("no"))
		end
	end
	return join(parts)
end
"#
			),
		),
		// `(...args: any) => infer R` binds R to the function's return.
		(
			"Returned",
			format!(
				"export type function Returned(T: type)\n{HELPER_DISTRIBUTE_TEXT}{HELPER_JOIN_TEXT}{HELPER_RETURN_OF_TEXT}{}",
				r#"	local parts = {}
	for _, item in distribute(T) do
		if item:is("function") then
			local R = returnof(item)
			table.insert(parts, R)
		else
			table.insert(parts, types.never)
		end
	end
	return join(parts)
end
"#
			),
		),
		// A template literal body is one template call.
		(
			"Greeting",
			format!(
				"export type function Greeting(T: type)\n{HELPER_TEMPLATE_TEXT}{}",
				r#"	return template({ "hello ", T })
end
"#
			),
		),
		// `keyof T` and `T[K]` outside a mapped type go through helpers.
		(
			"Keys",
			format!(
				"export type function Keys(T: type)\n{HELPER_DISTRIBUTE_TEXT}{HELPER_JOIN_TEXT}{HELPER_KEYS_TEXT}{}",
				r"	local parts = {}
	for _, item in distribute(T) do
		if item:issubtypeof(types.string) then
			table.insert(parts, types.never)
		else
			table.insert(parts, keys(item))
		end
	end
	return join(parts)
end
"
			),
		),
		(
			"Lookup",
			format!(
				"export type function Lookup(T: type, K: type)\n{HELPER_DISTRIBUTE_TEXT}{HELPER_JOIN_TEXT}{HELPER_INDEX_OF_TEXT}{}",
				r"	local parts = {}
	for _, item in distribute(T) do
		if item:issubtypeof(types.string) then
			table.insert(parts, types.never)
		else
			table.insert(parts, indexof(item, K))
		end
	end
	return join(parts)
end
"
			),
		),
	]
}

/// `typeFunctionBlocks`: every rendered type function by name, with its text.
fn type_function_blocks(got: &str) -> HashMap<String, String> {
	let mut blocks = HashMap::new();
	let lines: Vec<&str> = got.split_inclusive('\n').collect();
	let mut index = 0;
	while index < lines.len() {
		let line = lines[index];
		let rest = line.strip_prefix("export ").unwrap_or(line);
		if let Some(rest) = rest.strip_prefix("type function ")
			&& let Some(open) = rest.find('(')
		{
			let name = rest[..open].to_owned();
			let mut block = String::new();
			while index < lines.len() {
				block.push_str(lines[index]);
				index += 1;
				if block.ends_with("\nend\n") || lines[index - 1] == "end\n" {
					break;
				}
			}
			blocks.insert(name, block);
			continue;
		}
		index += 1;
	}
	blocks
}

const PRELUDE_NAMES: &[&str] = &[
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

#[test]
fn luau_types_type_function_bodies() {
	let (got, _) = render_luau_types_losses("src/typefunctions.ts", true);
	let blocks = type_function_blocks(&got);
	for (name, want) in type_function_bodies() {
		assert_eq!(blocks.get(name).map(String::as_str), Some(want.as_str()), "{name}");
		assert_eq!(
			got.matches(&format!("type function {name}(")).count(),
			1,
			"{name} must be declared once"
		);
	}
	// A body that is only `T[keyof T]` stays an alias over the built-in type functions,
	// exported at its declaration.
	assert_lines_in_order(&got, &["export type Values<T> = index<T, keyof<T>>"]);
	assert!(
		!got.contains("type function Values"),
		"Values must not become a type function:\n{got}"
	);
}

/// The bodies of `typeFunctionBodies`, straight from the registry.
#[test]
fn luau_types_type_function_bodies_registry() {
	with_fixture("src/typefunctions.ts", |fixture| {
		fixture.state.is_luau_typed = true;
		let mut failures = Vec::new();
		for (name, want) in type_function_bodies() {
			let entry = fixture.alias(name);
			let statements = entry_statements(&mut types(fixture.state), entry);
			let got = render_list(fixture.state, statements);
			if got != want {
				failures.push(format!("{name} rendered:\n{got}\nwant:\n{want}"));
			}
		}
		let values = fixture.alias("Values");
		let statements = entry_statements(&mut types(fixture.state), values);
		assert_eq!(
			render_list(fixture.state, statements),
			"export type Values<T> = index<T, keyof<T>>\n"
		);
		assert!(failures.is_empty(), "{}", failures.join("\n"));
	});
}

#[test]
fn luau_types_type_function_fallbacks() {
	let (got, losses) = render_luau_types_losses("src/typefunctions.ts", true);
	// Each fallback is `any` under a comment naming what stopped the translation, with the loss
	// kind of the construct that failed.
	assert_lines_in_order(&got, FALLBACK_LINES);
	for name in ["Unwrap", "Renamed", "Boxed", "IsBox"] {
		assert!(
			!got.contains(&format!("type function {name}(")),
			"{name} must fall back to an alias:\n{got}"
		);
	}
	for want in [
		"InferType",
		"KeyRemap",
		"MappedType",
		"ConditionalType",
		"ConstraintDropped",
	] {
		assert!(
			losses.iter().any(|loss| loss == want),
			"losses {losses:?} must include {want}"
		);
	}
}

const FALLBACK_LINES: &[&str] = &[
	"-- luauTypes: Unwrap<T> spelled as any: `infer` is only translated in `T extends (infer U)[]` and `T extends (...args: any) => infer R`",
	"export type Unwrap<T> = any",
	"-- luauTypes: Renamed<T> spelled as any: `Uppercase` names a type outside the type function; type-function bodies see no outer names",
	"export type Renamed<T> = any",
	"-- luauTypes: Boxed<T> spelled as any: `Box` names a type outside the type function; type-function bodies see no outer names",
	"export type Boxed<T> = any",
	"-- luauTypes: IsBox<T> spelled as any: `Box` names a type outside the type function; type-function bodies see no outer names",
	"export type IsBox<T> = any",
];

/// The fallbacks of `TestLuauTypesTypeFunctionFallbacks`, straight from the registry.
#[test]
fn luau_types_type_function_fallbacks_registry() {
	with_fixture("src/typefunctions.ts", |fixture| {
		fixture.state.is_luau_typed = true;
		let mut got = String::new();
		for name in ["Unwrap", "Renamed", "Boxed", "IsBox"] {
			let entry = fixture.alias(name);
			let statements = entry_statements(&mut types(fixture.state), entry);
			got.push_str(&render_list(fixture.state, statements));
		}
		assert_lines_in_order(&got, FALLBACK_LINES);
		let kinds: Vec<&str> = super::super::luautypes::luau_type_losses(fixture.state)
			.iter()
			.map(|loss| loss.kind.name())
			.collect();
		for want in ["InferType", "KeyRemap", "MappedType", "ConditionalType"] {
			assert!(kinds.contains(&want), "losses {kinds:?} must include {want}");
		}
	});
}

#[test]
fn luau_types_type_function_call_sites() {
	let (got, _) = render_luau_types_losses("src/typefunctions.ts", true);
	// A complete translation keeps the call-site spelling at every use, closed arguments
	// included; a return annotation comes from the signature's resolved type. A fallback is
	// named at generic uses and expanded eagerly at closed ones. Built-in helpers used
	// generically name their prelude twin; used with closed arguments they expand.
	assert_lines_in_order(
		&got,
		&[
			"local function patch(config: Config, partial: Partial2<Config>): Config",
			"local function first(values: { number }): number",
			r#"local function handlers(): Handlers<"click" | "hover">"#,
			"local function describe(value: Stringy<string>): string",
			"local function useResult(value: Returned<() -> number>): number",
			r#"local function greet(value: Greeting<"world">): string"#,
			"local function required(config: Required2<Config>, frozen: Readonly2<Config>): number",
			r#"local function lookup(key: Keys<Config>, value: Lookup<Config, "port">): Values<Config>"#,
			"local function box(value: { host: Box<string>, port: Box<number> }): Box<string>",
			"local function unwrap<T>(value: Unwrap<T>): Unwrap<T>",
			"local function fill<T>(partial: __Partial<T>, defaults: T): T",
			"local function freeze<T>(value: T): __Readonly<T>",
			"local function needAll<T>(value: __Required<T>): __Required<T>",
			"local function only<T, K>(value: __Pick<T, K>): __Pick<T, K>",
			"local function without<T, K>(value: __Omit<T, K>): __Omit<T, K>",
			"local function record<K, V>(value: __Record<K, V>): __Record<K, V>",
			"local function exclude<T, U>(value: __Exclude<T, U>): __Exclude<T, U>",
			"local function extract<T, U>(value: __Extract<T, U>): __Extract<T, U>",
			"local function defined<T>(value: __NonNullable<T>): __NonNullable<T>",
			"local function returned<T>(value: __ReturnType<T>): __ReturnType<T>",
			"local function parameters<T>(value: __Parameters<T>): __Parameters<T>",
			"local function partialConfig(value: { host: string?, port: number? }): { host: string }",
		],
	);
}

#[test]
fn luau_types_type_function_prelude() {
	let (got, _) = render_luau_types_losses("src/typefunctions.ts", true);
	let blocks = type_function_blocks(&got);
	// Every helper used generically has one prelude twin, hoisted after the header (before the
	// file's own leading comment) and never exported.
	let comment = got.find("-- Type-level aliases:");
	for name in PRELUDE_NAMES {
		let block = blocks
			.get(&format!("__{name}"))
			.unwrap_or_else(|| panic!("prelude __{name} is missing:\n{got}"));
		assert!(!block.starts_with("export "), "prelude __{name} must not be exported");
		assert_eq!(
			got.matches(&format!("type function __{name}(")).count(),
			1,
			"prelude __{name} must be declared once"
		);
		let index = got.find(block.as_str());
		assert!(
			matches!((index, comment), (Some(index), Some(comment)) if index < comment),
			"prelude __{name} must be hoisted before the file comment"
		);
	}
	// The prelude twin of Partial is the worked Partial2 body under its own name.
	let want = PARTIAL2_BODY
		.trim_start_matches("export ")
		.replacen("Partial2", "__Partial", 1);
	assert_eq!(blocks.get("__Partial"), Some(&want));
}

/// The prelude twins straight from `requirePrelude`, as the prelude probe reads them.
fn prelude_blocks() -> String {
	with_fixture("src/typefunctions.ts", |fixture| {
		let mut prelude = String::new();
		for name in PRELUDE_NAMES {
			assert_eq!(require_prelude(fixture.state, name), format!("__{name}"));
			let entry = types(fixture.state)
				.b()
				.preludes
				.get(*name)
				.copied()
				.expect("a prelude entry");
			let statements = entry_statements(&mut types(fixture.state), entry);
			prelude.push_str(&render_list(fixture.state, statements));
		}
		prelude
	})
}

/// `__Partial` from `requirePrelude` is the worked Partial2 body under its own name.
#[test]
fn luau_types_type_function_prelude_registry() {
	let prelude = prelude_blocks();
	let blocks = type_function_blocks(&prelude);
	for name in PRELUDE_NAMES {
		let block = blocks
			.get(&format!("__{name}"))
			.unwrap_or_else(|| panic!("prelude __{name} is missing:\n{prelude}"));
		assert!(!block.starts_with("export "), "prelude __{name} must not be exported");
	}
	let want = PARTIAL2_BODY
		.trim_start_matches("export ")
		.replacen("Partial2", "__Partial", 1);
	assert_eq!(blocks.get("__Partial"), Some(&want));
}

#[test]
fn luau_types_type_functions_analyze_clean() {
	let (got, _) = render_luau_types_losses("src/typefunctions.ts", true);
	let Some(analyzer) = find_luau_analyzer() else {
		return;
	};
	if let Some(findings) = analyze_luau_strict(&analyzer, &got, true) {
		assert!(
			findings.is_empty(),
			"typefunctions.ts does not analyze clean:\n{}\n--- rendered ---\n{got}",
			findings.join("\n")
		);
	}
}

/// The analyzer evaluates a type function at each use: a probe of the worked Partial2 body
/// with one right and one wrong table must fail on the wrong one alone.
#[test]
fn luau_types_type_function_partial_probe() {
	let Some(analyzer) = find_luau_analyzer() else {
		return;
	};
	let source = PARTIAL2_BODY.to_owned()
		+ &[
			"type Config = { host: string, port: number }",
			"local ok: Partial2<Config> = {}",
			r#"local partial: Partial2<Config> = { host = "x" }"#,
			"local primitive: Partial2<number> = 1",
			"local bad: Partial2<Config> = { host = 1 }",
			r#"local bad2: Partial2<Config> = { port = "1" }"#,
			"return nil",
			"",
		]
		.join("\n");
	let Some(findings) = analyze_luau_strict(&analyzer, &source, false) else {
		return;
	};
	let body_lines = PARTIAL2_BODY.matches('\n').count();
	assert_eq!(
		luau_error_lines(&findings),
		vec![body_lines + 5, body_lines + 6],
		"{}",
		findings.join("\n")
	);
}

const PRELUDE_USES: &[&str] = &[
	"type Config = { host: string, port: number, read secret: string }",
	r#"local a: __Pick<Config, "host"> = { host = "x" }"#,
	r#"local b: __Omit<Config, "host" | "secret"> = { port = 1 }"#,
	r#"local c: __Record<"a" | "b", number> = { a = 1, b = 2 }"#,
	r#"local d: __Exclude<"a" | "b", "a"> = "b""#,
	r#"local e: __Extract<"a" | "b", "a"> = "a""#,
	r#"local f: __NonNullable<string?> = "s""#,
	"local g: __ReturnType<() -> number> = 1",
	r#"local h: __Parameters<(number, string) -> ()> = { 1, "s" }"#,
	r#"local i: __Required<{ host: string? }> = { host = "x" }"#,
	r#"local j: __Readonly<Config> = { host = "x", port = 1, secret = "s" }"#,
	"local k: __Partial<Config> = {}",
	"local l: __Record<string, boolean> = { anything = true }",
	// Deliberate misuses, one per line, from here on.
	r#"local bad1: __Pick<Config, "host"> = { port = 1 }"#,
	r#"local bad2: __Record<"a", number> = { a = "s" }"#,
	r#"local bad3: __Exclude<"a" | "b", "a"> = "a""#,
	r#"local bad4: __ReturnType<() -> number> = "s""#,
	"local bad5: __Required<{ host: string? }> = {}",
	r#"j.host = "y""#,
	"local bad7: __Partial<Config> = { host = 1 }",
	"return nil",
	"",
];

/// Every prelude body accepts the tables its TS helper would and rejects exactly the
/// deliberate misuses. Go reads the bodies off the rendered fixture; they are the registry's.
#[test]
fn luau_types_type_function_prelude_probe() {
	let Some(analyzer) = find_luau_analyzer() else {
		return;
	};
	let prelude = prelude_blocks();
	let source = prelude.clone() + &PRELUDE_USES.join("\n");
	let Some(findings) = analyze_luau_strict(&analyzer, &source, false) else {
		return;
	};
	let bad1 = PRELUDE_USES
		.iter()
		.position(|line| line.starts_with("local bad1:"))
		.expect("the first misuse");
	let first = prelude.matches('\n').count() + bad1 + 1;
	let want: Vec<usize> = (0..7).map(|offset| first + offset).collect();
	assert_eq!(luau_error_lines(&findings), want, "{}", findings.join("\n"));
}
