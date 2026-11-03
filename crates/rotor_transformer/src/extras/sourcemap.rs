//! `sourcemap.go`: the deterministic V3 source map of a rendered file.

use std::fmt::Write as _;

use rotor_luau::render::{Range, RenderError, render_ast_with_positions_bytes};
use rotor_luau::{Origin, Position};
use rotor_support::gopath;

use crate::prelude::*;

const BASE64_VLQ_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `sourceMapMapping`
#[derive(Clone, Copy)]
struct SourceMapMapping {
	generated_line: u32,
	generated_column: u32,
	source_line: u32,
	source_column: u32,
}

impl State<'_> {
	/// `RenderWithSourceMap`: renders `statements` once and returns the text with its
	/// deterministic V3 source map. The text is bytes, as in Go: a `$file` string literal may
	/// not be valid UTF-8.
	///
	/// # Errors
	///
	/// When the statements do not render.
	pub fn render_with_source_map(
		&self,
		statements: Statements,
		source_file: P<SourceFile>,
	) -> Result<(Vec<u8>, String), RenderError> {
		SourceMapRenderer { ast: &self.ast }.render(statements, source_file.file_name(), source_file.text())
	}
}

/// The rendering half of `RenderWithSourceMap`, over the arena alone.
pub(super) struct SourceMapRenderer<'a> {
	pub(super) ast: &'a Ast,
}

impl SourceMapRenderer<'_> {
	/// Renders `statements` with the map of a source named `file_name` holding `source_text`.
	pub(super) fn render(
		&self,
		statements: Statements,
		file_name: &str,
		source_text: &str,
	) -> Result<(Vec<u8>, String), RenderError> {
		let (text, ranges) = render_ast_with_positions_bytes(self.ast, statements)?;
		let newlines = text.split(|&byte| byte == b'\n').count() - 1;
		let mut positions: Vec<Option<Position>> = vec![None; newlines + 1];
		for range in &ranges {
			let kind = self.ast.kind(range.node);
			if kind == LuauKind::Comment {
				continue;
			}
			if let Some(origin) = self.origin_of(range)
				&& !origin.start_unset
				&& kind.is_statement()
			{
				fill(&mut positions, range.start.line, origin.start);
			}
		}
		for range in &ranges {
			if let Some(origin) = self.origin_of(range)
				&& let Some(closing) = range.closing
				&& let Some(origin_closing) = origin.closing
			{
				fill(&mut positions, closing.line, origin_closing);
			}
		}
		// Ranges start in text order, with parents before children on a tie.
		for range in &ranges {
			if let Some(origin) = self.origin_of(range)
				&& !origin.start_unset
				&& !self.ast.kind(range.node).is_statement()
			{
				fill(&mut positions, range.start.line, origin.start);
			}
		}
		self.add_source_map_coverage(&text, &ranges, &mut positions);
		let mut mappings: Vec<SourceMapMapping> = Vec::with_capacity(positions.len());
		for (line, position) in positions.iter().enumerate() {
			if let Some(position) = position {
				mappings.push(SourceMapMapping {
					generated_line: u32::try_from(line).unwrap_or(u32::MAX),
					generated_column: 0,
					source_line: position.line,
					source_column: position.column,
				});
			}
		}
		// Parents precede children at a shared start, so the last start wins. Operator anchors
		// are appended last and take priority on a tie.
		for range in &ranges {
			if let Some(origin) = self.origin_of(range)
				&& !self.ast.kind(range.node).is_statement()
				&& !origin.start_unset
				&& range.start.column > 0
			{
				mappings.push(SourceMapMapping {
					generated_line: range.start.line,
					generated_column: range.start.column,
					source_line: origin.start.line,
					source_column: origin.start.column,
				});
			}
		}
		for range in &ranges {
			if let Some(origin) = self.origin_of(range)
				&& let Some(origin_operator) = origin.operator
				&& let Some(operator) = range.operator
				&& operator.column > 0
			{
				mappings.push(SourceMapMapping {
					generated_line: operator.line,
					generated_column: operator.column,
					source_line: origin_operator.line,
					source_column: origin_operator.column,
				});
			}
		}
		mappings.sort_by_key(|mapping| (mapping.generated_line, mapping.generated_column));
		let mut kept: Vec<SourceMapMapping> = Vec::with_capacity(mappings.len());
		for (index, mapping) in mappings.iter().enumerate() {
			if let Some(next) = mappings.get(index + 1)
				&& next.generated_line == mapping.generated_line
				&& next.generated_column == mapping.generated_column
			{
				continue;
			}
			if let Some(previous) = kept.last()
				&& previous.generated_line == mapping.generated_line
				&& previous.source_line == mapping.source_line
				&& previous.source_column == mapping.source_column
			{
				continue;
			}
			kept.push(*mapping);
		}
		let encoded = encode_source_map(file_name, source_text, &kept);
		Ok((text, encoded))
	}

	fn origin_of(&self, range: &Range) -> Option<Origin> {
		self.ast.origin(range.node).map(|origin| *self.ast.origin_data(origin))
	}

	/// `addSourceMapCoverage`: a still-unmapped non-blank line takes the origin of the innermost
	/// range open at its first non-blank character, unless that range is a comment.
	fn add_source_map_coverage(&self, text: &[u8], ranges: &[Range], positions: &mut [Option<Position>]) {
		struct OpenRange {
			end: Position,
			origin: Option<Origin>,
			comment: bool,
		}
		let mut stack: Vec<OpenRange> = Vec::new();
		let mut next_range = 0;
		for (line_index, line) in text.split(|&byte| byte == b'\n').enumerate() {
			if positions[line_index].is_some() {
				continue;
			}
			// Go ranges over the line's runes: an invalid byte is one `RuneError`, never a space.
			let Some(column) =
				rotor_luau::utf8::runes(line).position(|(rune, _)| !rune.is_some_and(char::is_whitespace))
			else {
				continue;
			};
			let point = Position::new(
				u32::try_from(line_index).unwrap_or(u32::MAX),
				u32::try_from(column).unwrap_or(u32::MAX),
			);
			while next_range < ranges.len() && !position_before(point, ranges[next_range].start) {
				let range = &ranges[next_range];
				next_range += 1;
				while stack.last().is_some_and(|open| !position_before(range.start, open.end)) {
					stack.pop();
				}
				let mut origin = self.origin_of(range).filter(|origin| !origin.start_unset);
				if origin.is_none()
					&& let Some(open) = stack.last()
				{
					origin = open.origin;
				}
				stack.push(OpenRange {
					end: range.end,
					origin,
					comment: self.ast.kind(range.node) == LuauKind::Comment,
				});
			}
			while stack.last().is_some_and(|open| !position_before(point, open.end)) {
				stack.pop();
			}
			if let Some(open) = stack.last()
				&& !open.comment
				&& let Some(origin) = open.origin
			{
				positions[line_index] = Some(origin.start);
			}
		}
	}
}

/// The V3 map as Go's `json.Marshal` writes `sourceMapV3`.
fn encode_source_map(file_name: &str, source_text: &str, mappings: &[SourceMapMapping]) -> String {
	let output_file_name = format!(
		"{}.luau",
		file_name.strip_suffix(extension(file_name)).unwrap_or(file_name)
	);
	let mut encoded = String::from("{\"version\":3,\"file\":");
	write_json_string(&mut encoded, &output_file_name);
	encoded.push_str(",\"sources\":[");
	write_json_string(&mut encoded, file_name);
	encoded.push_str("],\"sourcesContent\":[");
	write_json_string(&mut encoded, source_text);
	encoded.push_str("],\"mappings\":");
	write_json_string(&mut encoded, &encode_source_map_mappings(mappings));
	encoded.push('}');
	encoded
}

fn fill(positions: &mut [Option<Position>], line: u32, position: Position) {
	if let Some(slot) = usize::try_from(line).ok().and_then(|line| positions.get_mut(line))
		&& slot.is_none()
	{
		*slot = Some(position);
	}
}

/// `sourceMapPositionBefore`
const fn position_before(a: Position, b: Position) -> bool {
	a.line < b.line || (a.line == b.line && a.column < b.column)
}

/// `filepath.Ext`: the suffix from the final dot of the final path element.
fn extension(path: &str) -> &str {
	let separator = gopath::NATIVE.separator();
	for (index, character) in path.char_indices().rev() {
		if character == '/' || character == separator {
			break;
		}
		if character == '.' {
			return &path[index..];
		}
	}
	""
}

/// Appends `text` as a JSON string the way Go's `encoding/json` marshals it, including its HTML
/// escaping of `<`, `>` and `&`.
fn write_json_string(out: &mut String, text: &str) {
	out.push('"');
	for character in text.chars() {
		match character {
			'"' => out.push_str("\\\""),
			'\\' => out.push_str("\\\\"),
			'\x08' => out.push_str("\\b"),
			'\x0c' => out.push_str("\\f"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\t' => out.push_str("\\t"),
			character
				if u32::from(character) < 0x20 || matches!(character, '<' | '>' | '&' | '\u{2028}' | '\u{2029}') =>
			{
				let _ = write!(out, "\\u{:04x}", u32::from(character));
			}
			character => out.push(character),
		}
	}
	out.push('"');
}

fn encode_source_map_mappings(mappings: &[SourceMapMapping]) -> String {
	let mut builder = String::new();
	let mut last_generated_line = 0;
	let mut last_generated_column = 0;
	let mut last_source_line = 0;
	let mut last_source_column = 0;
	let mut first_mapping_on_line = true;

	for mapping in mappings {
		while last_generated_line < mapping.generated_line {
			builder.push(';');
			last_generated_line += 1;
			last_generated_column = 0;
			first_mapping_on_line = true;
		}
		if !first_mapping_on_line {
			builder.push(',');
		}
		append_base64_vlq(
			&mut builder,
			i64::from(mapping.generated_column) - i64::from(last_generated_column),
		);
		append_base64_vlq(&mut builder, 0);
		append_base64_vlq(
			&mut builder,
			i64::from(mapping.source_line) - i64::from(last_source_line),
		);
		append_base64_vlq(
			&mut builder,
			i64::from(mapping.source_column) - i64::from(last_source_column),
		);
		last_generated_column = mapping.generated_column;
		last_source_line = mapping.source_line;
		last_source_column = mapping.source_column;
		first_mapping_on_line = false;
	}
	builder
}

fn append_base64_vlq(builder: &mut String, value: i64) {
	let mut value = if value < 0 { ((-value) << 1) | 1 } else { value << 1 };
	loop {
		let mut digit = value & 31;
		value >>= 5;
		if value > 0 {
			digit |= 32;
		}
		builder.push(char::from(BASE64_VLQ_ALPHABET[usize::try_from(digit).unwrap_or(0)]));
		if value == 0 {
			return;
		}
	}
}
