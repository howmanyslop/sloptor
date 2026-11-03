//! `sourcemap_lines_test.go`, `sourcemap_columns_test.go`, `sourcemap_acceptance_test.go`,
//! `sourcemap_fallback_test.go` and `sourcemap_paths_test.go`. The tables are the Go tests',
//! verbatim.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use rotor_tsrs::sourcemap::{Mapping, decode_mappings};

use super::output::{build, patch_out_dir};
use super::vector_math::build_audit_project;
use super::{Temp, copy_dir, testdata, write_project};
use crate::ProjectOptions;
use crate::source_trace_sourcemap::rewrite_source_map_paths;

/// The decoded mappings of a source map's JSON, by generated line.
fn decode_source_map_lines(map_json: &str) -> BTreeMap<u32, Vec<Mapping>> {
	let map: serde_json::Value = serde_json::from_str(map_json).expect("the map parses");
	let mut decoder = decode_mappings(map["mappings"].as_str().unwrap_or_default());
	let mut lines: BTreeMap<u32, Vec<Mapping>> = BTreeMap::new();
	for mapping in decoder.values() {
		lines.entry(mapping.generated_line).or_default().push(mapping);
	}
	assert!(
		decoder.error().is_none(),
		"mappings do not decode: {:?}",
		decoder.error()
	);
	lines
}

/// `assertSourceMapContract`
fn assert_source_map_contract(text: &str, map_json: &str, source: &str) {
	let fields: serde_json::Map<String, serde_json::Value> = serde_json::from_str(map_json).expect("the map parses");
	assert_eq!(fields.len(), 5, "source map fields, want exactly five: {fields:?}");
	for key in ["version", "file", "sources", "sourcesContent", "mappings"] {
		assert!(fields.contains_key(key), "source map missing {key}");
	}
	assert!(
		fields["version"] == 3
			&& fields["sources"].as_array().is_some_and(|sources| sources.len() == 1)
			&& fields["file"].as_str().is_some_and(|file| !file.is_empty()),
		"invalid source map metadata: {fields:?}"
	);
	assert_eq!(
		fields["sourcesContent"],
		serde_json::json!([source]),
		"sourcesContent, want original bytes"
	);
	let lines: Vec<&str> = text.strip_suffix('\n').unwrap_or(text).split('\n').collect();
	let mut export_close = None;
	for (line, content) in lines.iter().enumerate() {
		if *content == "return {" {
			export_close = (line + 1..lines.len()).find(|&end| lines[end] == "}");
		}
	}
	for (line, mappings) in decode_source_map_lines(map_json) {
		let line = line as usize;
		assert!(line < lines.len(), "segment on nonexistent Luau line {}", line + 1);
		assert_eq!(
			mappings[0].generated_character,
			0,
			"Luau line {}: missing column-zero segment",
			line + 1
		);
		for pair in mappings.windows(2) {
			let (previous, current) = (&pair[0], &pair[1]);
			assert!(
				current.generated_character > previous.generated_character,
				"Luau line {}: duplicate or unordered generated columns: {mappings:?}",
				line + 1
			);
			assert!(
				!(current.source_line == previous.source_line && current.source_character == previous.source_character),
				"Luau line {}: redundant adjacent source positions: {mappings:?}",
				line + 1
			);
		}
		let content = lines[line];
		assert!(
			!(content.starts_with("--")
				|| content.starts_with("local TS = ")
				|| content == "local exports = {}"
				|| content == "return nil"
				|| content == "return exports"
				|| content == "return {"
				|| export_close == Some(line)),
			"compiler header/export wrapper at Luau line {} is mapped: {content:?}",
			line + 1
		);
	}
}

/// `compileWithSourceMap`: builds a package fixture with source maps, then without, and checks
/// the maps changed no Luau bytes.
fn compile_with_source_map(name: &str, source: &str) -> (String, String) {
	let temp = write_project(&format!("@scope/source-map-{name}"), "");
	if name != "main" {
		fs::remove_file(temp.join("src/main.ts")).expect("main.ts removes");
	}
	temp.write(&format!("src/{name}.ts"), source);
	let tsconfig = fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
	patch_out_dir(&temp, r#", "sourceMap": true"#);
	let result = build(&temp.dir(), &ProjectOptions::default());
	assert!(result.diagnostics.is_empty(), "diagnostics: {:?}", result.diagnostics);
	let read = |suffix: &str| fs::read_to_string(temp.join(&format!("out/{name}{suffix}"))).expect("an output reads");
	let (luau, map_json) = (read(".luau"), read(".luau.map"));
	assert_source_map_contract(&luau, &map_json, source);
	temp.write("tsconfig.json", tsconfig);
	build(&temp.dir(), &ProjectOptions::default());
	assert_eq!(luau, read(".luau"), "enabling source maps changed Luau bytes");
	(luau, map_json)
}

/// `annotateSourceMapSegments`: 1-based lines and 0-based UTF-16 columns.
fn annotate_source_map_segments(luau_text: &str, map_json: &str, column_zero_only: bool) -> String {
	let mut segments: BTreeMap<u32, Vec<String>> = BTreeMap::new();
	for (line, mappings) in decode_source_map_lines(map_json) {
		for mapping in mappings {
			if column_zero_only && mapping.generated_character != 0 {
				continue;
			}
			let segment = match (
				mapping.is_source_mapping(),
				mapping.source_line,
				mapping.source_character,
			) {
				(true, Some(source_line), Some(source_character)) => {
					format!(
						"{}->{}:{source_character}",
						mapping.generated_character,
						source_line + 1
					)
				}
				_ => format!("{}->?", mapping.generated_character),
			};
			segments.entry(line).or_default().push(segment);
		}
	}
	let lines: Vec<&str> = luau_text.strip_suffix('\n').unwrap_or(luau_text).split('\n').collect();
	let annotations: Vec<String> = (0..lines.len())
		.map(|line| {
			let joined = u32::try_from(line)
				.ok()
				.and_then(|line| segments.get(&line))
				.map(|segments| segments.join(" "))
				.unwrap_or_default();
			if joined.is_empty() { "-".to_owned() } else { joined }
		})
		.collect();
	let segment_width = annotations.iter().map(String::len).max().unwrap_or(1).max(1);
	let line_width = lines.len().to_string().len().max(2);
	let mut annotated = String::new();
	for (line, text) in lines.iter().enumerate() {
		let text = if text.starts_with("-- Compiled with sloptor v") {
			"-- Compiled with sloptor vX"
		} else {
			text
		};
		let _ = writeln!(
			annotated,
			"{:>line_width$} {:<segment_width$} | {text}",
			line + 1,
			annotations[line]
		);
	}
	annotated
}

#[test]
#[expect(clippy::too_many_lines, reason = "the Go test's table")]
fn source_map_lines() {
	let crlf = r"declare function register(callback: () => void): number;
declare function use(value: unknown): void;

register(() => {
	use(1);
});
const handle = register(() => {
	use(2);
});
use(handle);
"
	.replace('\n', "\r\n");
	let cases: Vec<(&str, &str, &str)> = vec![
		(
			"header",
			r"declare class Base {}
export let current: unknown = undefined;
export function isBase(value: unknown) {
	return value instanceof Base;
}
",
			r" 1 -      | -- Compiled with sloptor vX
 2 -      | local TS = _G[script]
 3 -      | local exports = {}
 4 0->2:0 | exports.current = nil
 5 0->3:0 | local function isBase(value)
 6 0->4:1 | 	return TS.instanceof(value, Base)
 7 0->5:0 | end
 8 0->3:0 | exports.isBase = isBase
 9 -      | return exports
",
		),
		(
			"call-args",
			r"declare function register(callback: () => void): number;
declare function use(value: unknown): void;

register(() => {
	use(1);
});
const handle = register(() => {
	use(2);
});
use(handle);
",
			r" 1 -       | -- Compiled with sloptor vX
 2 0->4:0  | register(function()
 3 0->5:1  | 	use(1)
 4 0->6:0  | end)
 5 0->7:0  | local handle = register(function()
 6 0->8:1  | 	use(2)
 7 0->9:0  | end)
 8 0->10:0 | use(handle)
 9 -       | return nil
",
		),
		(
			"method-call",
			r#"declare const signal: { Connect(callback: () => void): void };
declare function wrap(callback: () => void): () => void;

signal.Connect(() => {
	print("fired");
});
print(wrap(() => {
	print("inner");
}));
"#,
			r#" 1 -      | -- Compiled with sloptor vX
 2 0->4:0 | signal:Connect(function()
 3 0->5:1 | 	print("fired")
 4 0->6:0 | end)
 5 0->7:0 | print(wrap(function()
 6 0->8:1 | 	print("inner")
 7 0->9:0 | end))
 8 -      | return nil
"#,
		),
		(
			"duplicate-callbacks",
			r#"declare function both(first: () => void, second: () => void): void;
both(() => {
	print("same");
}, () => {
	print("same");
});
"#,
			r#" 1 -      | -- Compiled with sloptor vX
 2 0->2:0 | both(function()
 3 0->3:1 | 	print("same")
 4 0->4:0 | end, function()
 5 0->5:1 | 	print("same")
 6 0->6:0 | end)
 7 -      | return nil
"#,
		),
		(
			"returned-function",
			r"export function makeCounter() {
	let count = 0;
	return () => {
		count += 1;
		return count;
	};
}
",
			r" 1 -      | -- Compiled with sloptor vX
 2 0->1:0 | local function makeCounter()
 3 0->2:1 | 	local count = 0
 4 0->3:1 | 	return function()
 5 0->4:2 | 		count += 1
 6 0->5:2 | 		return count
 7 0->6:1 | 	end
 8 0->7:0 | end
 9 -      | return {
10 0->1:0 | 	makeCounter = makeCounter,
11 -      | }
",
		),
		(
			"object-methods",
			r#"export function makeApi() {
	return {
		greet(name: string) {
			return "hi " + name;
		},
		run: () => {
			print("run");
		},
	};
}
"#,
			r#" 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function makeApi()
 3 0->2:1  | 	return {
 4 0->3:2  | 		greet = function(self, name)
 5 0->4:3  | 			return "hi " .. name
 6 0->5:2  | 		end,
 7 0->6:2  | 		run = function()
 8 0->7:3  | 			print("run")
 9 0->8:2  | 		end,
10 0->2:8  | 	}
11 0->10:0 | end
12 -       | return {
13 0->1:0  | 	makeApi = makeApi,
14 -       | }
"#,
		),
		(
			"concise-arrow",
			r"declare function register(callback: () => unknown): number;
declare function use(value: unknown): unknown;
register(() => use(1));
",
			r" 1 -       | -- Compiled with sloptor vX
 2 0->3:0  | register(function()
 3 0->3:15 | 	return use(1)
 4 0->3:20 | end)
 5 -       | return nil
",
		),
		(
			"loops",
			r"export function spin(limit: number) {
	let i = 0;
	do {
		i += 1;
	} while (i < limit);
	return i;
}
",
			r" 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function spin(limit)
 3 0->2:1  | 	local i = 0
 4 0->3:1  | 	repeat
 5 0->3:1  | 		do
 6 0->4:2  | 			i += 1
 7 0->3:1  | 		end
 8 0->5:20 | 	until not (i < limit)
 9 0->6:1  | 	return i
10 0->7:0  | end
11 -       | return {
12 0->1:0  | 	spin = spin,
13 -       | }
",
		),
		(
			"elseif",
			r"export function sign(n: number) {
	if (n > 0) {
		return 1;
	} else if (n < 0) {
		return -1;
	} else {
		return 0;
	}
}
",
			r" 1 -      | -- Compiled with sloptor vX
 2 0->1:0 | local function sign(n)
 3 0->2:1 | 	if n > 0 then
 4 0->3:2 | 		return 1
 5 0->4:8 | 	elseif n < 0 then
 6 0->5:2 | 		return -1
 7 0->4:8 | 	else
 8 0->7:2 | 		return 0
 9 0->8:1 | 	end
10 0->9:0 | end
11 -      | return {
12 0->1:0 | 	sign = sign,
13 -      | }
",
		),
		(
			"else-if-prereq",
			r#"export function label(a: string | undefined, b: string) {
	if (a === "x") {
		return 1;
	} else if ((a ?? b) === "y") {
		return 2;
	}
	return 0;
}
"#,
			r#" 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function label(a, b)
 3 0->2:1  | 	if a == "x" then
 4 0->3:2  | 		return 1
 5 0->2:1  | 	else
 6 0->4:8  | 		local _condition = a
 7 0->4:8  | 		if _condition == nil then
 8 0->4:18 | 			_condition = b
 9 0->6:1  | 		end
10 0->4:8  | 		if _condition == "y" then
11 0->5:2  | 			return 2
12 0->6:1  | 		end
13 0->6:1  | 	end
14 0->7:1  | 	return 0
15 0->8:0  | end
16 -       | return {
17 0->1:0  | 	label = label,
18 -       | }
"#,
		),
		(
			"switch",
			r#"export function kind(n: number) {
	let result = "none";
	switch (n) {
		case 1:
			result = "one";
		case 2:
			result = "low";
			break;
		default:
			result = "high";
	}
	return result;
}
"#,
			r#" 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function kind(n)
 3 0->2:1  | 	local result = "none"
 4 0->3:1  | 	repeat
 5 0->3:1  | 		local _fallthrough = false
 6 0->4:2  | 		if n == 1 then
 7 0->5:3  | 			result = "one"
 8 0->4:2  | 			_fallthrough = true
 9 0->5:17 | 		end
10 0->6:2  | 		if _fallthrough or n == 2 then
11 0->7:3  | 			result = "low"
12 0->8:3  | 			break
13 0->8:8  | 		end
14 0->10:3 | 		result = "high"
15 0->11:1 | 	until true
16 0->12:1 | 	return result
17 0->13:0 | end
18 -       | return {
19 0->1:0  | 	kind = kind,
20 -       | }
"#,
		),
		(
			"try",
			r#"declare function risky(): void;
export function attempt() {
	try {
		risky();
	} catch (e) {
		print(e);
	} finally {
		print("done");
	}
}
"#,
			r#" 1 -       | -- Compiled with sloptor vX
 2 -       | local TS = _G[script]
 3 0->2:0  | local function attempt()
 4 0->3:1  | 	TS.try(function()
 5 0->4:2  | 		risky()
 6 0->5:1  | 	end, function(e)
 7 0->6:2  | 		print(e)
 8 0->7:1  | 	end, function()
 9 0->8:2  | 		print("done")
10 0->9:1  | 	end)
11 0->10:0 | end
12 -       | return {
13 0->2:0  | 	attempt = attempt,
14 -       | }
"#,
		),
		(
			"parameters",
			r#"export function greet(name = "world", { loud }: { loud: boolean }) {
	print(name, loud);
}
"#,
			r#" 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function greet(name, _param)
 3 0->1:22 | 	if name == nil then
 4 0->1:29 | 		name = "world"
 5 0->1:35 | 	end
 6 0->1:38 | 	local loud = _param.loud
 7 0->2:1  | 	print(name, loud)
 8 0->3:0  | end
 9 -       | return {
10 0->1:0  | 	greet = greet,
11 -       | }
"#,
		),
		(
			"class-methods",
			r"export class Counter {
	private count = 0;
	increment(step = 1) {
		this.count += step;
		return this.count;
	}
}
",
			r#" 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local Counter
 3 0->1:0  | do
 4 0->1:0  | 	Counter = setmetatable({}, {
 5 0->1:0  | 		__tostring = function()
 6 0->1:0  | 			return "Counter"
 7 0->1:0  | 		end,
 8 0->1:0  | 	})
 9 0->1:0  | 	Counter.__index = Counter
10 0->1:0  | 	function Counter.new(...)
11 0->1:0  | 		local self = setmetatable({}, Counter)
12 0->1:0  | 		return self:constructor(...) or self
13 0->1:0  | 	end
14 0->1:0  | 	function Counter:constructor()
15 0->2:1  | 		self.count = 0
16 0->1:0  | 	end
17 0->3:1  | 	function Counter:increment(step)
18 0->3:11 | 		if step == nil then
19 0->3:18 | 			step = 1
20 0->3:18 | 		end
21 0->4:2  | 		self.count += step
22 0->5:2  | 		return self.count
23 0->6:1  | 	end
24 0->7:0  | end
25 -       | return {
26 0->1:0  | 	Counter = Counter,
27 -       | }
"#,
		),
		(
			"class-ctor-static",
			r#"export class Point {
	static origin = 0;
	static {
		print("init");
	}
	constructor(public x: number) {
		print(x);
	}
}
"#,
			r#" 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local Point
 3 0->1:0  | do
 4 0->1:0  | 	Point = setmetatable({}, {
 5 0->1:0  | 		__tostring = function()
 6 0->1:0  | 			return "Point"
 7 0->1:0  | 		end,
 8 0->1:0  | 	})
 9 0->1:0  | 	Point.__index = Point
10 0->1:0  | 	function Point.new(...)
11 0->1:0  | 		local self = setmetatable({}, Point)
12 0->1:0  | 		return self:constructor(...) or self
13 0->1:0  | 	end
14 0->6:1  | 	function Point:constructor(x)
15 0->6:13 | 		self.x = x
16 0->7:2  | 		print(x)
17 0->8:1  | 	end
18 0->2:1  | 	Point.origin = 0
19 0->3:1  | 	do
20 0->4:2  | 		print("init")
21 0->5:1  | 	end
22 0->9:0  | end
23 -       | return {
24 0->1:0  | 	Point = Point,
25 -       | }
"#,
		),
		(
			"nullish",
			r"export function pick(a: string | undefined, b: string) {
	const value = a ?? b;
	return value;
}
",
			r" 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function pick(a, b)
 3 0->2:1  | 	local _condition = a
 4 0->2:1  | 	if _condition == nil then
 5 0->2:20 | 		_condition = b
 6 0->2:21 | 	end
 7 0->2:1  | 	local value = _condition
 8 0->3:1  | 	return value
 9 0->4:0  | end
10 -       | return {
11 0->1:0  | 	pick = pick,
12 -       | }
",
		),
		(
			"continuation",
			r#"const config = {
	retries: 3,
	label: "x",
};
print(config);
"#,
			r#" 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local config = {
 3 0->2:1  | 	retries = 3,
 4 0->3:1  | 	label = "x",
 5 0->1:15 | }
 6 0->5:0  | print(config)
 7 -       | return nil
"#,
		),
		(
			"unicode",
			r#"const face = "😀"; print(face);
"#,
			r#" 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local face = "😀"
 3 0->1:19 | print(face)
 4 -       | return nil
"#,
		),
		(
			"crlf",
			&crlf,
			r" 1 -       | -- Compiled with sloptor vX
 2 0->4:0  | register(function()
 3 0->5:1  | 	use(1)
 4 0->6:0  | end)
 5 0->7:0  | local handle = register(function()
 6 0->8:1  | 	use(2)
 7 0->9:0  | end)
 8 0->10:0 | use(handle)
 9 -       | return nil
",
		),
	];
	for (name, source, want) in cases {
		let (luau, map_json) = compile_with_source_map(name, source);
		assert_eq!(
			annotate_source_map_segments(&luau, &map_json, true),
			want,
			"{name}: source map lines"
		);
	}
}

#[test]
#[expect(clippy::too_many_lines, reason = "the Go test's table")]
fn source_map_columns() {
	let cases: Vec<(&str, &str, &str)> = vec![
		(
			"operators",
			r"export function check(a: boolean, b: boolean, c: boolean) {
	const ok = a &&
		b ||
		c;
	return ok;
}
",
			r" 1 -                                                | -- Compiled with sloptor vX
 2 0->1:0                                           | local function check(a, b, c)
 3 0->2:1 12->2:12 14->2:14 18->3:2 20->3:4 23->4:2 | 	local ok = a and b or c
 4 0->5:1 8->5:8                                    | 	return ok
 5 0->6:0                                           | end
 6 -                                                | return {
 7 0->1:0                                           | 	check = check,
 8 -                                                | }
",
		),
		(
			"nullish",
			r"export function pick(a: string | undefined, b: string) {
	const value = a ?? b;
	return value;
}
",
			r" 1 -               | -- Compiled with sloptor vX
 2 0->1:0          | local function pick(a, b)
 3 0->2:1 20->2:15 | 	local _condition = a
 4 0->2:1 4->2:17  | 	if _condition == nil then
 5 0->2:20         | 		_condition = b
 6 0->2:21         | 	end
 7 0->2:1 15->2:15 | 	local value = _condition
 8 0->3:1 8->3:8   | 	return value
 9 0->4:0          | end
10 -               | return {
11 0->1:0          | 	pick = pick,
12 -               | }
",
		),
		(
			"temporaries",
			r"export function bump(state: { value: number }) {
	const previous = state.value++;
	return previous;
}
",
			r" 1 -                       | -- Compiled with sloptor vX
 2 0->1:0                  | local function bump(state)
 3 0->2:1 19->2:18         | 	local _original = state.value
 4 0->2:1 1->2:18 13->2:29 | 	state.value += 1
 5 0->2:1 18->2:18         | 	local previous = _original
 6 0->3:1 8->3:8           | 	return previous
 7 0->4:0                  | end
 8 -                       | return {
 9 0->1:0                  | 	bump = bump,
10 -                       | }
",
		),
		(
			"continuation",
			r#"const config = {
	retries: 3,
	label: "x",
};
print(config);
"#,
			r#" 1 -               | -- Compiled with sloptor vX
 2 0->1:0 15->1:15 | local config = {
 3 0->2:1 11->2:10 | 	retries = 3,
 4 0->3:1 9->3:8   | 	label = "x",
 5 0->1:15         | }
 6 0->5:0 6->5:6   | print(config)
 7 -               | return nil
"#,
		),
		(
			"call-index",
			r"declare const store: { get(key: string): number; nested: { value: number } };
export function read(key: string) {
	return store.get(key) +
		store.nested.value;
}
",
			r" 1 -                                       | -- Compiled with sloptor vX
 2 0->2:0                                  | local function read(key)
 3 0->3:1 8->3:8 18->3:18 23->3:23 25->4:2 | 	return store:get(key) + store.nested.value
 4 0->5:0                                  | end
 5 -                                       | return {
 6 0->2:0                                  | 	read = read,
 7 -                                       | }
",
		),
		(
			"unicode-columns",
			"const face = \"😀\"; print(\"😀\", face);\n",
			r#" 1 -                        | -- Compiled with sloptor vX
 2 0->1:0 13->1:13          | local face = "😀"
 3 0->1:19 6->1:25 12->1:31 | print("😀", face)
 4 -                        | return nil
"#,
		),
	];
	for (name, source, want) in cases {
		let (text, source_map) = compile_with_source_map(name, source);
		assert_eq!(
			annotate_source_map_segments(&text, &source_map, false),
			want,
			"{name}: source map columns"
		);
	}
}

#[test]
fn source_map_runtime_headers() {
	for project_type in ["game", "model", "package"] {
		let temp = Temp::new();
		copy_dir(
			Path::new(&testdata(&format!("runtimelib_{project_type}"))),
			&temp.join(""),
		);
		let sources = [
			(
				"main",
				"declare class Base {}\nexport let current: unknown = undefined;\nexport function isBase(value: unknown) {\n\treturn value instanceof Base;\n}\n",
			),
			("immutable", "export const value = 1;\n"),
			("empty", "export {};\n"),
		];
		for (name, source) in sources {
			temp.write(&format!("src/{name}.ts"), source);
		}
		patch_out_dir(&temp, r#", "sourceMap": true"#);
		let result = build(&temp.dir(), &ProjectOptions::default());
		assert!(
			result.diagnostics.is_empty(),
			"{project_type}: diagnostics: {:?}",
			result.diagnostics
		);
		for (name, source) in sources {
			let text = result
				.outputs
				.get(&format!("out/{name}.luau"))
				.map(|bytes| String::from_utf8_lossy(bytes).into_owned())
				.unwrap_or_default();
			if name == "main" {
				assert!(
					text.contains("local TS = ") && text.contains("local exports = {}"),
					"{project_type}: fixture did not exercise runtime import and exports table:\n{text}"
				);
			}
			let map_json = fs::read_to_string(temp.join(&format!("out/{name}.luau.map"))).expect("the map reads");
			assert_source_map_contract(&text, &map_json, source);
			let mappings = decode_source_map_lines(&map_json);
			for (line, content) in text.split('\n').enumerate() {
				if content.contains("exports.current = nil") || content.trim() == "value = value," {
					let first = u32::try_from(line)
						.ok()
						.and_then(|line| mappings.get(&line))
						.and_then(|line| line.first());
					assert!(
						first.is_some_and(|mapping| mapping.generated_character == 0 && mapping.is_source_mapping()),
						"{project_type}: export value at line {} lost its source mapping",
						line + 1
					);
				}
			}
		}
	}
}

#[test]
fn source_map_for_iterator_fallback_temp_order() {
	let temp = build_audit_project();
	let source = "declare const input: number | undefined;\n\
		for (const [key, value] of new Map<() => number, () => number>([\n\
		\x20 [() => { const _0 = 10; const n = input ?? 1; return _0 + n; },\n\
		\x20  () => input ?? 2],\n\
		])) print(key(), value());\n";
	temp.write("src/main.ts", source);
	let config = fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
	let want = concat!(
		"for key, value in {\n",
		"\t[function()\n\t\tlocal _0 = 10\n\t\tlocal _1 = input\n",
		"\t\tif _1 == nil then\n\t\t\t_1 = 1\n\t\tend\n",
		"\t\tlocal n = _1\n\t\treturn _0 + n\n\tend] = function()\n",
		"\t\tlocal _0 = input\n\t\tif _0 == nil then\n\t\t\t_0 = 2\n\t\tend\n",
		"\t\treturn _0\n\tend,\n} do\n\tprint(key(), value())\nend\nreturn nil\n",
	);
	for maps in [false, true] {
		let text_config = if maps {
			config.replacen(r#""outDir": "out""#, r#""outDir": "out", "sourceMap": true"#, 1)
		} else {
			config.clone()
		};
		temp.write("tsconfig.json", text_config);
		let result = build(&temp.dir(), &ProjectOptions::default());
		assert!(
			result.diagnostics.is_empty(),
			"build diagnostics: {:?}",
			result.diagnostics
		);
		let text = fs::read_to_string(temp.join("out/main.luau")).expect("main.luau reads");
		let body = text.split_once('\n').map_or("", |(_, body)| body);
		assert_eq!(body, want, "maps={maps}: iterator callback bytes");
		if maps {
			let map_json = fs::read_to_string(temp.join("out/main.luau.map")).expect("the map reads");
			assert_source_map_contract(&text, &map_json, source);
			let mappings = decode_source_map_lines(&map_json);
			for (line, content) in text.split('\n').enumerate() {
				let source_line = if content.contains("local _1 = input") {
					Some(2)
				} else if content.contains("local _0 = input") {
					Some(3)
				} else {
					None
				};
				if let Some(source_line) = source_line {
					let first = u32::try_from(line)
						.ok()
						.and_then(|line| mappings.get(&line))
						.and_then(|line| line.first());
					assert!(
						first.is_some_and(|mapping| mapping.generated_character == 0
							&& mapping.is_source_mapping()
							&& mapping.source_line == Some(source_line)),
						"callback temporary on Luau line {}: want TS line {}, got {first:?}",
						line + 1,
						source_line + 1
					);
				}
			}
		}
	}
}

#[test]
fn rewrite_source_map_paths_cases() {
	let tests = [
		(
			"same Windows volume",
			r"D:\checkout\packages\core\out\core\action-state.luau",
			r"D:\checkout\packages\core\src\core\action-state.ts",
			"action-state.luau",
			"../../src/core/action-state.ts",
		),
		(
			"different Windows volume",
			r"D:\checkout\out\main.luau",
			r"C:\sources\main.ts",
			"main.luau",
			"file:///C:/sources/main.ts",
		),
	];
	for (name, generated_file, source_file, want_file, want_source) in tests {
		let raw = format!(
			r#"{{"version":3,"file":"wrong.luau","sources":[{}],"sourcesContent":["source"],"mappings":"AAAA"}}"#,
			serde_json::Value::from(source_file)
		);
		let rewritten = rewrite_source_map_paths(&raw, generated_file).unwrap_or_else(|error| panic!("{error}"));
		let map: serde_json::Value = serde_json::from_str(&rewritten).expect("the rewritten map parses");
		assert_eq!(map["file"], want_file, "{name}: file");
		assert_eq!(map["sources"], serde_json::json!([want_source]), "{name}: sources");
	}
}
