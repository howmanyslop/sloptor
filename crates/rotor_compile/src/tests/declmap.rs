//! `declmap_test.go`

use crate::declmap::{decode_base64_vlq, shift_declaration_map_columns};
use crate::declpaths::{TextEdit, apply_text_edits};
use crate::source_trace_sourcemap::append_vlq;

const UTF8_BYTE_ORDER_MARK: &str = "\u{FEFF}";

/// `encodeGeneratedColumns`
fn encode_generated_columns(lines: &[Vec<i64>]) -> String {
	let mut builder = String::new();
	for (line_index, columns) in lines.iter().enumerate() {
		if line_index > 0 {
			builder.push(';');
		}
		let mut previous = 0;
		for (column_index, &column) in columns.iter().enumerate() {
			if column_index > 0 {
				builder.push(',');
			}
			append_vlq(&mut builder, column - previous);
			append_vlq(&mut builder, 0);
			append_vlq(&mut builder, 0);
			append_vlq(&mut builder, 0);
			previous = column;
		}
	}
	builder
}

/// `decodeGeneratedColumns`
fn decode_generated_columns(mappings: &str) -> Vec<Vec<i64>> {
	let bytes = mappings.as_bytes();
	let mut lines: Vec<Vec<i64>> = vec![Vec::new()];
	let mut previous = 0;
	let mut index = 0;
	while index < bytes.len() {
		match bytes[index] {
			b';' => {
				lines.push(Vec::new());
				previous = 0;
				index += 1;
				continue;
			}
			b',' => {
				index += 1;
				continue;
			}
			_ => {}
		}
		let Some((value, mut next)) = decode_base64_vlq(bytes, index) else {
			panic!("undecodable mappings at {index}: {mappings:?}");
		};
		previous += value;
		if let Some(line) = lines.last_mut() {
			line.push(previous);
		}
		while next < bytes.len() && bytes[next] != b',' && bytes[next] != b';' {
			next += 1;
		}
		index = next;
	}
	lines
}

/// `wrapMappings`
fn wrap_mappings(mappings: &str) -> String {
	format!(
		r#"{{"version":3,"file":"main.d.ts","sourceRoot":"","sources":["../src/main.ts"],"names":[],"mappings":"{mappings}"}}"#
	)
}

/// `declarationMapMappings`
fn declaration_map_mappings(map_text: &str) -> &str {
	const KEY: &str = r#""mappings":""#;
	let Some(start) = map_text.find(KEY) else {
		panic!("declaration map has no mappings field: {map_text}");
	};
	let start = start + KEY.len();
	let Some(end) = map_text[start..].find('"') else {
		panic!("declaration map mappings field is unterminated: {map_text}");
	};
	&map_text[start..start + end]
}

fn edit(start: usize, end: usize, text: &str) -> TextEdit {
	TextEdit {
		start,
		end,
		text: text.to_owned(),
	}
}

/// A case's name, declaration text, edits, input columns and wanted columns.
type Case<'a> = (&'a str, &'a str, Vec<TextEdit>, Vec<Vec<i64>>, Vec<Vec<i64>>);

#[test]
fn shift_declaration_map_columns_cases() {
	let decl_text = "import { Value } from \"@alias/value\";\nexport declare const value: Value;\n";
	let specifier_start = decl_text.find("\"@alias/value\"").expect("the specifier");
	let specifier_end = specifier_start + "\"@alias/value\"".len();
	let bom_text = format!("{UTF8_BYTE_ORDER_MARK}{decl_text}");
	let bom = UTF8_BYTE_ORDER_MARK.len();

	let cases: Vec<Case<'_>> = vec![
		(
			"columns past a shortened specifier move left",
			decl_text,
			vec![edit(specifier_start, specifier_end, "\"./value\"")],
			vec![vec![0, 7, 26, 37, 38], vec![0, 7, 21]],
			vec![vec![0, 7, 26, 32, 33], vec![0, 7, 21]],
		),
		(
			"columns past a lengthened specifier move right",
			decl_text,
			vec![edit(specifier_start, specifier_end, "\"./deeply/nested/value\"")],
			vec![vec![0, 26, 37], vec![0, 21]],
			vec![vec![0, 26, 46], vec![0, 21]],
		),
		(
			"a same-length rewrite leaves the map alone",
			decl_text,
			vec![edit(specifier_start, specifier_end, "\"./aliased/xy\"")],
			vec![vec![0, 26, 37]],
			vec![vec![0, 26, 37]],
		),
		(
			"two rewrites on one line accumulate",
			"import \"@alias/a\";import \"@alias/b\";\n",
			vec![edit(7, 17, "\"./a\""), edit(25, 35, "\"./b\"")],
			vec![vec![0, 7, 18, 25, 36]],
			vec![vec![0, 7, 13, 20, 26]],
		),
		(
			"a byte order mark does not offset the columns",
			&bom_text,
			vec![edit(bom + specifier_start, bom + specifier_end, "\"./value\"")],
			vec![vec![0, 26, 37]],
			vec![vec![0, 26, 32]],
		),
		(
			"a non-BMP character before the specifier costs two columns",
			"/** \u{1F600} */ import \"@alias/value\";\n",
			vec![edit(12, 26, "\"./value\"")],
			vec![vec![0, 12, 26]],
			vec![vec![0, 12, 21]],
		),
		(
			"CRLF line endings do not shift the columns",
			"import \"@alias/value\";\r\nexport declare const value: number;\r\n",
			vec![edit(7, 21, "\"./value\"")],
			vec![vec![0, 7, 21], vec![0, 7]],
			vec![vec![0, 7, 16], vec![0, 7]],
		),
		(
			"empty generated lines are preserved",
			"import \"@alias/value\";\n\n\nexport declare const value: number;\n",
			vec![edit(7, 21, "\"./value\"")],
			vec![vec![0, 7, 21], vec![], vec![], vec![0, 7]],
			vec![vec![0, 7, 16], vec![], vec![], vec![0, 7]],
		),
		(
			"negative generated-column deltas survive the shift",
			"import \"@alias/value\";\n",
			vec![edit(7, 21, "\"./value\"")],
			vec![vec![0, 21, 7, 21, 0]],
			vec![vec![0, 16, 7, 16, 0]],
		),
		(
			"no edits leaves the map byte-identical",
			decl_text,
			Vec::new(),
			vec![vec![0, 26, 37]],
			vec![vec![0, 26, 37]],
		),
	];
	for (name, text, edits, input, want) in cases {
		let map_text = wrap_mappings(&encode_generated_columns(&input));
		let shifted = shift_declaration_map_columns(&map_text, text, &edits);
		let got = decode_generated_columns(declaration_map_mappings(&shifted));
		assert_eq!(got, want, "{name}: generated columns");
		assert_eq!(
			wrap_mappings(declaration_map_mappings(&shifted)),
			shifted,
			"{name}: fixup rewrote something other than the mappings field"
		);
	}
}

#[test]
fn shift_declaration_map_columns_tolerates_undecodable_mappings() {
	let decl_text = "import \"@alias/value\";\n";
	let edits = [edit(7, 21, "\"./value\"")];
	let shifted = shift_declaration_map_columns(&wrap_mappings("AAAA,!!!!"), decl_text, &edits);
	assert!(shifted.contains("!!!!"), "undecodable mappings were dropped: {shifted}");
}

#[test]
fn shift_declaration_map_columns_stays_inside_every_line() {
	let decl_text = "import { Value } from \"@alias/deeply/nested/value\";\nexport declare const value: Value;\n";
	let specifier_start = decl_text.find("\"@alias").expect("the specifier");
	let specifier_end = decl_text.find(';').expect("the semicolon");
	let edits = [edit(specifier_start, specifier_end, "\"./v\"")];
	let column = |offset: usize| i64::try_from(offset).expect("a column fits i64");
	let (start, end) = (column(specifier_start), column(specifier_end));
	let input = vec![vec![0, 7, 12, 14, 18, 20, start, end, end + 1], vec![0, 7, 21]];

	let shifted = shift_declaration_map_columns(&wrap_mappings(&encode_generated_columns(&input)), decl_text, &edits);
	let rewritten = apply_text_edits(decl_text, &edits);
	let lines: Vec<&str> = rewritten.split('\n').collect();
	for (line_index, columns) in decode_generated_columns(declaration_map_mappings(&shifted))
		.iter()
		.enumerate()
	{
		for &column in columns {
			assert!(
				usize::try_from(column).is_ok_and(|column| column <= lines[line_index].len()),
				"line {line_index} column {column} is outside {:?}",
				lines[line_index]
			);
		}
	}
}
