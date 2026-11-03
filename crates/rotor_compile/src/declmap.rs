//! `declmap.go`.
//!
//! Keeps a `.d.ts.map` honest after the module specifiers in its `.d.ts` were spliced to a
//! different length. Only each segment's generated column moves; every other field is copied
//! through verbatim.

use std::collections::HashMap;

use crate::declpaths::TextEdit;
use crate::source_trace::utf16_position;
use crate::source_trace_sourcemap::append_vlq;

/// `base64VLQAlphabet`
pub(crate) const BASE64_VLQ_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `utf8ByteOrderMark`: what `emitBOM` prepends to an emitted file.
const UTF8_BYTE_ORDER_MARK: &str = "\u{feff}";

/// `shiftDeclarationMapColumns`: `decl_text` is the text as emitted, before the splices: the
/// map's coordinate system.
pub(crate) fn shift_declaration_map_columns(map_text: &str, decl_text: &str, edits: &[TextEdit]) -> String {
	// `mappings` is a plain JSON string of base64 VLQ, nothing JSON escapes, so splicing that
	// one value keeps every other byte of the map exactly as tsrs wrote it.
	const KEY: &str = "\"mappings\":\"";
	// `emitBOM` prepends the mark after the map is generated, so drop it from both sides.
	let (decl_text, bom_offset) = decl_text
		.strip_prefix(UTF8_BYTE_ORDER_MARK)
		.map_or((decl_text, 0), |rest| (rest, UTF8_BYTE_ORDER_MARK.len()));
	let shifts = declaration_column_shifts(decl_text, edits, bom_offset);
	if shifts.is_empty() {
		return map_text.to_owned();
	}
	let Some(start) = map_text.find(KEY) else {
		return map_text.to_owned();
	};
	let start = start + KEY.len();
	let Some(end) = map_text[start..].find('"') else {
		return map_text.to_owned();
	};
	let end = start + end;
	let shifted = shift_mapping_columns(&map_text[start..end], &shifts);
	format!("{}{shifted}{}", &map_text[..start], &map_text[end..])
}

/// `columnShift`: every mapping on a line at or past `column` moves by `delta` UTF-16 units.
#[derive(Clone, Copy, Debug)]
struct ColumnShift {
	column: i64,
	delta: i64,
}

fn to_i64(value: usize) -> i64 {
	i64::try_from(value).unwrap_or(i64::MAX)
}

/// `declarationColumnShifts`: the edits as per-line UTF-16 column shifts.
fn declaration_column_shifts(
	decl_text: &str,
	edits: &[TextEdit],
	bom_offset: usize,
) -> HashMap<usize, Vec<ColumnShift>> {
	let mut shifts: HashMap<usize, Vec<ColumnShift>> = HashMap::new();
	for edit in edits {
		let (Some(start), Some(end)) = (edit.start.checked_sub(bom_offset), edit.end.checked_sub(bom_offset)) else {
			continue;
		};
		let Some((start_line, start_column)) = utf16_position(decl_text, start) else {
			continue;
		};
		// A module specifier is a single-line token; anything else is left alone.
		let Some((end_line, end_column)) = utf16_position(decl_text, end) else {
			continue;
		};
		if end_line != start_line {
			continue;
		}
		let delta = to_i64(utf16_length(&edit.text)) - (to_i64(end_column) - to_i64(start_column));
		if delta == 0 {
			continue;
		}
		shifts.entry(start_line).or_default().push(ColumnShift {
			column: to_i64(end_column),
			delta,
		});
	}
	shifts
}

/// `shiftMappingColumns`: rewrites every segment's generated column, tracking the delta-encoded
/// running column on both sides.
fn shift_mapping_columns(mappings: &str, shifts: &HashMap<usize, Vec<ColumnShift>>) -> String {
	let bytes = mappings.as_bytes();
	let mut builder = String::with_capacity(mappings.len() + mappings.len() / 8);
	let mut line = 0;
	let mut previous = 0i64;
	let mut shifted = 0i64;
	let mut index = 0;
	while index < bytes.len() {
		match bytes[index] {
			b';' => {
				builder.push(';');
				index += 1;
				line += 1;
				previous = 0;
				shifted = 0;
				continue;
			}
			b',' => {
				builder.push(',');
				index += 1;
				continue;
			}
			_ => {}
		}
		let mut segment_end = index;
		while segment_end < bytes.len() && bytes[segment_end] != b',' && bytes[segment_end] != b';' {
			segment_end += 1;
		}
		let Some((value, next)) = decode_base64_vlq(bytes, index) else {
			// Unparseable mappings are not worth failing an emit over.
			builder.push_str(&mappings[index..]);
			return builder;
		};
		let column = previous + value;
		let adjusted = column + shift_for(shifts.get(&line).map_or(&[][..], Vec::as_slice), column);
		append_vlq(&mut builder, adjusted - shifted);
		builder.push_str(&mappings[next..segment_end]);
		previous = column;
		shifted = adjusted;
		index = segment_end;
	}
	builder
}

/// `shiftFor`: the shifts on a line at or before `column`.
fn shift_for(line_shifts: &[ColumnShift], column: i64) -> i64 {
	line_shifts
		.iter()
		.filter(|shift| column >= shift.column)
		.map(|shift| shift.delta)
		.sum()
}

/// `decodeBase64VLQ`: the value at `index` and the index after it.
pub(crate) fn decode_base64_vlq(text: &[u8], mut index: usize) -> Option<(i64, usize)> {
	let mut result = 0i64;
	let mut shift = 0u32;
	while index < text.len() {
		let digit = BASE64_VLQ_ALPHABET.iter().position(|&byte| byte == text[index])?;
		let digit = to_i64(digit);
		index += 1;
		result |= (digit & 31).checked_shl(shift)?;
		if digit & 32 == 0 {
			if result & 1 != 0 {
				return Some((-(result >> 1), index));
			}
			return Some((result >> 1, index));
		}
		shift += 5;
	}
	None
}

/// `utf16Length`
pub(crate) fn utf16_length(text: &str) -> usize {
	text.chars().map(char::len_utf16).sum()
}
