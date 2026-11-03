//! `sourcemap_test.go` and `sourcemap_bench_test.go`.
//!
//! Go builds these states over a parsed `/test.ts` with no program; the map only reads the
//! arena and the file's name and text, so these render through [`SourceMapRenderer`] over a
//! mechanics state's arena. Go's `TestSourcePositionAnchorsPrereqsAndStatements` swaps the
//! package-level `TransformStatement` dispatch, which Rust cannot, and its allocation-budget
//! test measures Go allocations; both are not ported.

use std::time::Instant;

use rotor_luau::{Origin, Position, element};
use rotor_rojo::{JsonValue, parse_json};
use rotor_tsrs::ast::SourceFileParseOptions;
use rotor_tsrs::tscore::ScriptKind;
use rotor_tsrs::{parser, scanner, tspath};

use crate::extras::sourcemap::SourceMapRenderer;
use crate::prelude::*;
use crate::tests::with_test_state;
use crate::util::{node_end, node_pos};

const FIXTURE_TEXT: &str = "if (true) {\n\tuse(1);\n}\n";

fn position(line: u32, column: u32) -> Position {
	Position::new(line, column)
}

fn set_origin(ast: &mut Ast, node: NodeId, start: Position, closing: Option<Position>) {
	let origin = ast.add_origin(Origin {
		start,
		closing,
		..Origin::default()
	});
	ast.set_origin(node, origin);
}

fn render(ast: &Ast, statements: Statements, source_text: &str) -> (String, String) {
	SourceMapRenderer { ast }
		.render(statements, "/test.ts", source_text)
		.expect("the statements render")
}

/// Go's `sourceMapFixture`: `if true then use(1) end` with statement anchors.
fn source_map_fixture(ast: &mut Ast) -> Statements {
	let use_id = ast.id("use");
	let one = ast.num(1.0);
	let args = ast.list_from([one]);
	let call = ast.call(use_id, args);
	let inner = ast.call_statement(call);
	set_origin(ast, inner, position(1, 1), None);
	let condition = ast.bool(true);
	let body = ast.list_from([inner]);
	let block = ast.if_statement(condition, body, None);
	set_origin(ast, block, position(0, 0), Some(position(2, 0)));
	ast.list_from([block])
}

fn parse_map(map_json: &str) -> Vec<(String, JsonValue)> {
	match parse_json(map_json.as_bytes()) {
		JsonValue::Object(members) => members,
		other => panic!("the map is not a JSON object: {other:?}"),
	}
}

fn member<'a>(members: &'a [(String, JsonValue)], key: &str) -> &'a JsonValue {
	members
		.iter()
		.find(|(name, _)| name == key)
		.map_or_else(|| panic!("the map has no {key}"), |(_, value)| value)
}

fn mappings(map_json: &str) -> String {
	member(&parse_map(map_json), "mappings")
		.as_str()
		.expect("mappings is a string")
		.to_owned()
}

/// Go's `assertSourceMapLines`: the zero-based `line:column` source of each generated line's
/// column-0 segment, or `-`.
fn assert_source_map_lines(text: &str, map_json: &str, want: &[&str]) {
	let lines: Vec<&str> = text.strip_suffix('\n').unwrap_or(text).split('\n').collect();
	assert_eq!(
		lines.len(),
		want.len(),
		"rendered {} lines, want {}:\n{text}",
		lines.len(),
		want.len()
	);
	let mut got = vec!["-".to_owned(); lines.len()];
	let encoded = mappings(map_json);
	let mut decoder = rotor_tsrs::sourcemap::decode_mappings(&encoded);
	let segments: Vec<_> = decoder.values().collect();
	assert!(decoder.error().is_none(), "invalid mappings: {:?}", decoder.error());
	for mapping in segments {
		let line = usize::try_from(mapping.generated_line).expect("a line index");
		assert!(line < lines.len(), "mapping beyond rendered text: {mapping:?}");
		if mapping.generated_character != 0 {
			continue; // Statement-level fixtures assert their line anchors only.
		}
		assert!(
			mapping.is_source_mapping(),
			"unexpected generated-only mapping: {mapping:?}"
		);
		assert_eq!(got[line], "-", "multiple segments on line {line}");
		got[line] = format!(
			"{}:{}",
			mapping.source_line.unwrap_or_default(),
			mapping.source_character.unwrap_or_default()
		);
	}
	for (index, want) in want.iter().enumerate() {
		assert_eq!(
			got[index],
			*want,
			"line {} {:?}: source = {}, want {want}",
			index + 1,
			lines[index],
			got[index]
		);
	}
}

#[test]
fn source_position_uses_utf16_columns() {
	let file = parse_test_file("const value = \"😀\"; value;");
	let second_statement = file.statements().nodes()[1];
	for maps in [false, true] {
		with_test_state(|state| {
			state.source_map_enabled = maps;
			let got = state.original_source_position(Some(second_statement), false);
			assert_eq!(got, Some(position(0, 20)), "maps={maps}");
		});
	}
}

fn parse_test_file(text: &str) -> P<SourceFile> {
	parser::parse_source_file(
		SourceFileParseOptions {
			file_name: "/test.ts".to_owned(),
			path: tspath::Path::from("/test.ts"),
			..SourceFileParseOptions::default()
		},
		text,
		ScriptKind::TS,
	)
}

/// Go's `GetECMALineAndUTF16CharacterOfPosition`, which also accepts an offset inside a
/// character: Go's string range decodes each stray byte as one replacement code unit. The tsrs
/// counterpart slices a Rust string there, so the oracle counts the bytes itself.
fn utf16_oracle(file: P<SourceFile>, offset: u32) -> Position {
	let (line, column) = scanner::get_ecma_line_and_byte_offset_of_position(&*file, offset);
	let end = usize::try_from(offset).expect("an offset");
	let start = end - usize::try_from(column).expect("a column");
	let bytes = &file.text().as_bytes()[start..end];
	let mut units = 0;
	let mut index = 0;
	while index < bytes.len() {
		let width = match bytes[index] {
			0x00..=0x7f => 1,
			0xc0..=0xdf => 2,
			0xe0..=0xef => 3,
			0xf0..=0xf7 => 4,
			_ => 0,
		};
		let character = bytes
			.get(index..index + width)
			.filter(|_| width > 0)
			.and_then(|sequence| std::str::from_utf8(sequence).ok())
			.and_then(|sequence| sequence.chars().next());
		if let Some(character) = character {
			units += character.len_utf16();
			index += width;
		} else {
			units += 1;
			index += 1;
		}
	}
	position(
		u32::try_from(line).expect("a line"),
		u32::try_from(units).expect("a column"),
	)
}

/// Each parent's end is queried before its children, and one state serves files with identical
/// paths and different text, so a cached index must follow source-file identity. Go's third
/// source holds an invalid UTF-8 byte, which a Rust string cannot.
fn check_source_positions_match_utf16_oracle(maps: bool) {
	let sources = [
		"const a = 1; print(a);\r\nconst b = 2;\rprint(b);\n",
		"\u{FEFF}const café = \"😀é中\"; print(café);\r\nconst 𝒜 = café;\u{2028}print(𝒜);\u{2029}print(café);",
		"const text = \"a\u{FFFD}😀\"; print(text);",
	];
	with_test_state(|state| {
		state.source_map_enabled = maps;
		for source in sources {
			let file = parse_test_file(source);
			let mut nodes = vec![file.as_node()];
			while let Some(node) = nodes.pop() {
				for use_end in [false, true] {
					let offset = if use_end {
						u32::try_from(node_pos(node).max(node_end(node) - 1)).expect("an offset")
					} else {
						scanner::get_token_pos_of_node(node, file, false)
					};
					let want = utf16_oracle(file, offset);
					let got = state.original_source_position(Some(node), use_end);
					assert_eq!(got, Some(want), "maps={maps} end={use_end} position={offset}");
				}
				let mut children = Vec::new();
				node.for_each_child(&mut |child| {
					children.push(child);
					false
				});
				nodes.extend(children.into_iter().rev());
			}
		}
	});
}

#[test]
fn source_positions_match_utf16_oracle_maps_on() {
	check_source_positions_match_utf16_oracle(true);
}

/// The maps-off path reaches offsets inside a multi-byte character (`end - 1` of `café`), which
/// Go's `UTF16Len` counts byte by byte.
#[test]
fn source_positions_match_utf16_oracle_maps_off() {
	check_source_positions_match_utf16_oracle(false);
}

#[test]
fn render_with_source_map() {
	let source_text = "const a = 1;\r\nconst b = 2;\r\n";
	with_test_state(|state| {
		let ast = &mut state.ast;
		let a = ast.id("a");
		let one = ast.num(1.0);
		let first = ast.variable_declaration(a, Some(one.into()));
		let b = ast.id("b");
		let two = ast.num(2.0);
		let second = ast.variable_declaration(b, Some(two.into()));
		let statements = ast.list_from([first, second]);
		set_origin(ast, first, position(0, 0), Some(position(0, 11)));
		set_origin(ast, second, position(1, 0), Some(position(1, 11)));

		let (text, map_json) = render(ast, statements, source_text);
		assert_eq!(text, "local a = 1\nlocal b = 2\n", "text");
		let members = parse_map(&map_json);
		let names: Vec<&str> = members.iter().map(|(name, _)| name.as_str()).collect();
		assert_eq!(
			names,
			["version", "file", "sources", "sourcesContent", "mappings"],
			"map fields"
		);
		assert_eq!(member(&members, "version").as_i64(), Some(3), "version");
		assert_eq!(member(&members, "file").as_str(), Some("/test.luau"), "file");
		let sources: Vec<Option<&str>> = member(&members, "sources")
			.as_array()
			.expect("sources is an array")
			.iter()
			.map(JsonValue::as_str)
			.collect();
		assert_eq!(sources, [Some("/test.ts")], "sources");
		let contents: Vec<Option<&str>> = member(&members, "sourcesContent")
			.as_array()
			.expect("sourcesContent is an array")
			.iter()
			.map(JsonValue::as_str)
			.collect();
		assert_eq!(contents, [Some(source_text)], "sourcesContent");
		assert_eq!(mappings(&map_json), "AAAA;AACA", "mappings");
		let encoded = mappings(&map_json);
		let mut decoder = rotor_tsrs::sourcemap::decode_mappings(&encoded);
		let _ = decoder.values().count();
		assert!(decoder.error().is_none(), "mappings must be valid VLQ");
	});
}

#[test]
fn render_with_source_map_uses_end_anchor_for_block_end() {
	with_test_state(|state| {
		let ast = &mut state.ast;
		let value = ast.id("value");
		let one = ast.num(1.0);
		let inner = ast.variable_declaration(value, Some(one.into()));
		let condition_value = ast.bool(true);
		let body = ast.list_from([inner]);
		let condition = ast.if_statement(condition_value, body, None);
		let statements = ast.list_from([condition]);
		set_origin(ast, condition, position(0, 0), Some(position(2, 0)));
		set_origin(ast, inner, position(1, 1), None);

		let (text, map_json) = render(ast, statements, "if (true) {\n\tconst value = 1;\n}\n");
		assert_eq!(text, "if true then\n\tlocal value = 1\nend\n", "text");
		assert_eq!(mappings(&map_json), "AAAA;AACC;AACD", "mappings");
	});
}

fn fallback_call(ast: &mut Ast) -> NodeId {
	let fallback = ast.id("fallback");
	let args = ast.new_list();
	let call = ast.call(fallback, args);
	ast.call_statement(call)
}

#[test]
fn render_with_source_map_covers_nested_synthetic_gaps() {
	with_test_state(|state| {
		let ast = &mut state.ast;
		let x = ast.str("x");
		let one = ast.num(1.0);
		let field = ast.map_field(x, one);
		let fields = ast.list_from([field]);
		let table = ast.map(fields);
		let record = ast.id("record");
		let record_declaration = ast.variable_declaration(record, Some(table.into()));
		let text_id = ast.id("text");
		let continuation = ast.str("first\ncontinuation");
		let text_declaration = ast.variable_declaration(text_id, Some(continuation.into()));
		let inner_call = fallback_call(ast);
		let inner_body = ast.list_from([inner_call]);
		let inner_do = ast.do_statement(inner_body);
		let then_body = ast.list_from([record_declaration, text_declaration, inner_do]);
		let else_call = fallback_call(ast);
		let else_body = ast.list_from::<element::Statement>([else_call]);
		let condition_value = ast.bool(true);
		let condition = ast.if_statement(condition_value, then_body, Some(else_body.into()));
		set_origin(ast, condition, position(20, 4), None);
		let middle_body = ast.list_from([condition]);
		let middle = ast.do_statement(middle_body);
		let outer_body = ast.list_from([middle]);
		let outer = ast.do_statement(outer_body);
		set_origin(ast, outer, position(10, 2), Some(position(90, 1)));
		let nil = ast.nil();
		let return_statement = ast.return_statement(nil);
		let statements = ast.list_from([outer, return_statement]);

		let (text, map_json) = render(ast, statements, FIXTURE_TEXT);
		let want_text = "do\n\tdo\n\t\tif true then\n\t\t\tlocal record = {\n\t\t\t\tx = 1,\n\t\t\t}\n\t\t\tlocal text = [[first\ncontinuation]]\n\t\t\tdo\n\t\t\t\tfallback()\n\t\t\tend\n\t\telse\n\t\t\tfallback()\n\t\tend\n\tend\nend\nreturn nil\n";
		assert_eq!(text, want_text, "text");
		assert_source_map_lines(
			&text,
			&map_json,
			&[
				"10:2", "10:2", "20:4", "20:4", "20:4", "20:4", "20:4", "20:4", "20:4", "20:4", "20:4", "20:4", "20:4",
				"20:4", "10:2", "90:1", "-",
			],
		);
	});
}

#[test]
fn render_with_source_map_other_starts() {
	with_test_state(|state| {
		let ast = &mut state.ast;
		let value = ast.num(1.0);
		set_origin(ast, value, position(33, 2), None);
		let x = ast.str("x");
		let nested_field = ast.map_field(x, value);
		let nested_fields = ast.list_from([nested_field]);
		let nested = ast.map(nested_fields);
		set_origin(ast, nested, position(30, 3), None);
		let key = ast.str("first");
		set_origin(ast, key, position(21, 1), None);
		let first = ast.map_field(key, nested);
		set_origin(ast, first, position(20, 1), None);
		let second_key = ast.str("second");
		let second_value = ast.num(2.0);
		set_origin(ast, second_key, position(41, 4), None);
		set_origin(ast, second_value, position(42, 4), None);
		let third_value = ast.num(3.0);
		set_origin(ast, third_value, position(43, 5), None);
		let second = ast.map_field(second_key, second_value);
		let third_key = ast.str("third");
		let third = ast.map_field(third_key, third_value);
		let fields = ast.list_from([first, second, third]);
		let table = ast.map(fields);
		let return_statement = ast.return_statement(table);
		let statements = ast.list_from([return_statement]);

		let (text, map_json) = render(ast, statements, FIXTURE_TEXT);
		let want_text = "return {\n\tfirst = {\n\t\tx = 1,\n\t},\n\tsecond = 2,\n\tthird = 3,\n}\n";
		assert_eq!(text, want_text, "text");
		// Field and key share a start column: the outer field wins. The key on the second line
		// beats its later value; any other start beats coverage.
		assert_source_map_lines(&text, &map_json, &["-", "20:1", "33:2", "30:3", "41:4", "43:5", "-"]);
	});
}

#[test]
fn render_with_source_map_leaves_tagged_comments_unmapped() {
	with_test_state(|state| {
		let ast = &mut state.ast;
		let single = ast.comment("single");
		let multiline = ast.comment("first\r\n\r\nsecond");
		set_origin(ast, single, position(20, 0), None);
		set_origin(ast, multiline, position(30, 0), None);
		let empty = ast.new_list();
		let inner = ast.do_statement(empty);
		let outer_body = ast.list_from([single, multiline, inner]);
		let outer = ast.do_statement(outer_body);
		set_origin(ast, outer, position(10, 0), Some(position(11, 0)));
		let statements = ast.list_from([outer]);

		let (text, map_json) = render(ast, statements, FIXTURE_TEXT);
		let want_text = "do\n\t--single\n\t--[[\n\t\tfirst\n\t\t\n\t\tsecond\n\t]]\n\tdo\n\tend\nend\n";
		assert_eq!(text, want_text, "text");
		assert_source_map_lines(
			&text,
			&map_json,
			&["10:0", "-", "-", "-", "-", "-", "-", "10:0", "10:0", "11:0"],
		);
	});
}

fn empty_function(ast: &mut Ast, tag_statement: bool) -> Statements {
	let params = ast.new_list();
	let body = ast.new_list();
	let function = ast.function_expression(params, false, body);
	set_origin(ast, function, position(7, 0), Some(position(8, 0)));
	let fn_id = ast.id("fn");
	let statement = ast.variable_declaration(fn_id, Some(function.into()));
	if tag_statement {
		set_origin(ast, statement, position(2, 0), None);
	}
	ast.list_from([statement])
}

fn repeat(ast: &mut Ast, tag_closing: bool) -> Statements {
	let condition = ast.bool(true);
	set_origin(ast, condition, position(30, 0), None);
	let body = ast.new_list();
	let statement = ast.repeat_statement(condition, body);
	set_origin(ast, statement, position(20, 0), tag_closing.then(|| position(21, 0)));
	ast.list_from([statement])
}

#[test]
fn render_with_source_map_rule_priority() {
	type Build = fn(&mut Ast) -> Statements;
	let tests: [(&str, Build, &str, &[&str]); 4] = [
		(
			"statement before closing and other",
			|ast| empty_function(ast, true),
			"local fn = function() end\n",
			&["2:0"],
		),
		(
			"closing before other",
			|ast| empty_function(ast, false),
			"local fn = function() end\n",
			&["8:0"],
		),
		(
			"until closing before condition start",
			|ast| repeat(ast, true),
			"repeat\nuntil true\n",
			&["20:0", "21:0"],
		),
		(
			"missing closing allows other before coverage",
			|ast| repeat(ast, false),
			"repeat\nuntil true\n",
			&["20:0", "30:0"],
		),
	];
	for (name, build, want_text, positions) in tests {
		with_test_state(|state| {
			let statements = build(&mut state.ast);
			let (text, map_json) = render(&state.ast, statements, FIXTURE_TEXT);
			assert_eq!(text, want_text, "{name}: text");
			assert_source_map_lines(&text, &map_json, positions);
		});
	}
}

fn same_callback(ast: &mut Ast, start: u32, body: u32, closing: u32) -> NodeId {
	let same = ast.id("same");
	let args = ast.new_list();
	let call = ast.call(same, args);
	let statement = ast.call_statement(call);
	set_origin(ast, statement, position(body, 1), None);
	let params = ast.new_list();
	let statements = ast.list_from([statement]);
	let function = ast.function_expression(params, false, statements);
	set_origin(ast, function, position(start, 0), Some(position(closing, 0)));
	function
}

#[test]
fn render_with_source_map_repeated_occurrences() {
	with_test_state(|state| {
		let ast = &mut state.ast;
		let first = same_callback(ast, 1, 2, 3);
		let second = same_callback(ast, 4, 5, 6);
		let consume = ast.id("consume");
		let args = ast.list_from([first, second]);
		let expression = ast.call(consume, args);
		// Reuse the exact same node as a third argument, beyond the identical text of the first
		// two callbacks. Every occurrence must retain its own range.
		let call_args = ast.get::<rotor_luau::CallExpression>(expression).expect("a call").args;
		ast.list_push(call_args, first);
		let statement = ast.call_statement(expression);
		set_origin(ast, statement, Position::default(), None);
		let statements = ast.list_from([statement]);

		let (text, map_json) = render(ast, statements, FIXTURE_TEXT);
		let want_text = "consume(function()\n\tsame()\nend, function()\n\tsame()\nend, function()\n\tsame()\nend)\n";
		assert_eq!(text, want_text, "text");
		// Closing the preceding callback outranks the next callback's start.
		assert_source_map_lines(&text, &map_json, &["0:0", "2:1", "3:0", "5:1", "6:0", "2:1", "3:0"]);
	});
}

#[test]
fn render_with_source_map_solves_callback_temporaries() {
	with_test_state(|state| {
		let ast = &mut state.ast;
		let first = ast.temp_id("value");
		let second = ast.temp_id("value");
		let one = ast.num(1.0);
		let first_declaration = ast.variable_declaration(first, Some(one.into()));
		let two = ast.num(2.0);
		let second_declaration = ast.variable_declaration(second, Some(two.into()));
		let use_id = ast.id("use");
		let use_args = ast.list_from([first, second]);
		let use_call = ast.call(use_id, use_args);
		let use_statement = ast.call_statement(use_call);
		let body = ast.list_from([first_declaration, second_declaration, use_statement]);
		let params = ast.new_list();
		let function = ast.function_expression(params, false, body);
		set_origin(ast, function, position(10, 0), Some(position(20, 0)));
		let object = ast.id("object");
		let method_args = ast.list_from([function]);
		let method_call = ast.method_call("consume", object, method_args);
		let handle = ast.id("handle");
		let statement = ast.variable_declaration(handle, Some(method_call.into()));
		let statements = ast.list_from([statement]);

		let (text, map_json) = render(ast, statements, FIXTURE_TEXT);
		let want_text = "local handle = object:consume(function()\n\tlocal _value = 1\n\tlocal _value_1 = 2\n\tuse(_value, _value_1)\nend)\n";
		assert_eq!(text, want_text, "text");
		assert_source_map_lines(&text, &map_json, &["10:0", "10:0", "10:0", "10:0", "20:0"]);
	});
}

#[test]
fn render_with_source_map_recomputes_mutated_layout() {
	with_test_state(|state| {
		let statements = source_map_fixture(&mut state.ast);
		let (_, before) = render(&state.ast, statements, FIXTURE_TEXT);
		let block = state.ast.list_to_vec(statements)[0];
		let body = state
			.ast
			.get::<rotor_luau::IfStatement>(block)
			.expect("an if")
			.statements;
		let comment = state.ast.comment("added\nlines");
		state.ast.list_unshift(body, comment);
		let (text, got) = render(&state.ast, statements, FIXTURE_TEXT);
		assert_ne!(
			got, before,
			"adding multiline output must move the following source-map anchors"
		);

		let fresh_statements = source_map_fixture(&mut state.ast);
		let fresh_block = state.ast.list_to_vec(fresh_statements)[0];
		let fresh_body = state
			.ast
			.get::<rotor_luau::IfStatement>(fresh_block)
			.expect("an if")
			.statements;
		let fresh_comment = state.ast.comment("added\nlines");
		state.ast.list_unshift(fresh_body, fresh_comment);
		let (want_text, want) = render(&state.ast, fresh_statements, FIXTURE_TEXT);
		assert_eq!(
			(&text, &got),
			(&want_text, &want),
			"mutated render differs from a fresh one"
		);
		assert_source_map_lines(&text, &got, &["0:0", "-", "-", "-", "-", "1:1", "2:0"]);
	});
}

/// `BenchmarkRenderWithSourceMap`, as a skipped timing test: run it with `--ignored`.
#[test]
#[ignore = "a benchmark (Go's BenchmarkRenderWithSourceMap)"]
fn bench_render_with_source_map() {
	fn anchor_all(ast: &mut Ast, statements: Statements) {
		for statement in ast.list_to_vec(statements) {
			set_origin(ast, statement, position(1, 0), None);
		}
	}
	fn measure(name: &str, ast: &Ast, statements: Statements) {
		const RUNS: u32 = 100;
		let start = Instant::now();
		for _ in 0..RUNS {
			let _ = render(ast, statements, FIXTURE_TEXT);
		}
		let elapsed = start.elapsed() / RUNS;
		let _ = (name, elapsed);
	}
	with_test_state(|state| {
		for depth in [16, 64, 128] {
			let ast = &mut state.ast;
			let value = ast.id("value");
			let one = ast.num(1.0);
			let mut statement = ast.variable_declaration(value, Some(one.into()));
			for _ in 0..depth {
				let body = ast.list_from([statement]);
				statement = ast.do_statement(body);
			}
			let statements = ast.list_from([statement]);
			anchor_all(ast, statements);
			measure(&format!("nested_{depth}"), &state.ast, statements);
		}
		for kind in ["declarations", "calls", "if_body"] {
			let ast = &mut state.ast;
			let mut statements = ast.new_list();
			for _ in 0..128 {
				let statement = if kind == "calls" {
					let use_id = ast.id("use");
					let args = ast.new_list();
					let call = ast.call(use_id, args);
					ast.call_statement(call)
				} else {
					let value = ast.temp_id("value");
					let one = ast.num(1.0);
					ast.variable_declaration(value, Some(one.into()))
				};
				ast.list_push(statements, statement);
			}
			if kind == "if_body" {
				let condition = ast.bool(true);
				let block = ast.if_statement(condition, statements, None);
				statements = ast.list_from([block]);
			}
			anchor_all(ast, statements);
			measure(kind, &state.ast, statements);
		}
	});
}
