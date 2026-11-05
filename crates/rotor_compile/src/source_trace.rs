//! `source_trace.go`: traces from a transformer's reprinted source back to the text on disk, so
//! a diagnostic's position indexes the file the user sees.

use std::collections::HashMap;
use std::sync::Arc;

use crate::compile::{DiagnosticInfo, line_col_in};
use crate::incremental_manifest::normalize_source_file_path;

/// A 0-based line and UTF-16 column (`transformer.SourcePosition`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourcePosition {
	/// The 0-based line.
	pub line: usize,
	/// The 0-based UTF-16 column.
	pub column: usize,
}

/// `traceMapping`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TraceMapping {
	generated_line: u32,
	generated_column: u32,
	source_line: u32,
	source_column: u32,
}

/// `sourceTraceMap`: one reprinted file's mappings back to its original text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceTraceMap {
	file_name: String,
	text: String,
	mappings: Vec<TraceMapping>,
}

impl SourceTraceMap {
	/// `newSourceTraceMap`: parses a V3 source map's JSON.
	///
	/// # Errors
	///
	/// The map does not parse, is not version 3, or its mappings do not decode.
	pub fn new(raw: &str, file_name: &str, text: &str) -> Result<Self, String> {
		let (version, mappings) =
			decode_raw_source_map(raw).map_err(|error| format!("parse transformer trace map: {error}"))?;
		Self::from_mappings(version, &mappings, file_name, text)
	}

	/// `newSourceTraceMapFromMappings`
	///
	/// # Errors
	///
	/// The version is not 3 or the mappings do not decode.
	pub fn from_mappings(version: i64, mappings: &str, file_name: &str, text: &str) -> Result<Self, String> {
		if version != 3 {
			return Err(format!("parse transformer trace map: version = {version}, want 3"));
		}
		let mut trace = Self {
			file_name: file_name.to_owned(),
			text: text.to_owned(),
			mappings: Vec::new(),
		};
		// The decoder keeps generated-position order, which `original_position` searches.
		let mut decoder = rotor_tsrs::sourcemap::decode_mappings(mappings);
		for mapping in decoder.values() {
			if !mapping.is_source_mapping() {
				continue;
			}
			let (Some(source_line), Some(source_column)) = (mapping.source_line, mapping.source_character) else {
				continue;
			};
			let limit = i32::MAX.unsigned_abs();
			if mapping.generated_line > limit
				|| mapping.generated_character > limit
				|| source_line > limit
				|| source_column > limit
			{
				return Err("parse transformer trace map: coordinate exceeds 32-bit range".to_owned());
			}
			trace.mappings.push(TraceMapping {
				generated_line: mapping.generated_line,
				generated_column: mapping.generated_character,
				source_line,
				source_column,
			});
		}
		if let Some(error) = decoder.error() {
			return Err(format!("parse transformer trace map mappings: {error}"));
		}
		Ok(trace)
	}

	/// A trace with `mappings` as (generated line, generated column, source line, source column),
	/// Go's struct literal in tests.
	#[cfg(test)]
	pub(crate) fn for_test(file_name: &str, text: &str, mappings: &[(u32, u32, u32, u32)]) -> Self {
		Self {
			file_name: file_name.to_owned(),
			text: text.to_owned(),
			mappings: mappings
				.iter()
				.map(
					|&(generated_line, generated_column, source_line, source_column)| TraceMapping {
						generated_line,
						generated_column,
						source_line,
						source_column,
					},
				)
				.collect(),
		}
	}

	/// How many mappings the trace holds.
	#[cfg(test)]
	pub(crate) const fn mapping_count(&self) -> usize {
		self.mappings.len()
	}

	/// `OriginalSourceFileName`
	#[must_use]
	pub fn original_source_file_name(&self) -> &str {
		&self.file_name
	}

	/// `OriginalSourceText`
	#[must_use]
	pub fn original_source_text(&self) -> &str {
		&self.text
	}

	/// `OriginalPosition`: the source position of the last mapping at or before `position`.
	#[must_use]
	pub fn original_position(&self, position: SourcePosition) -> Option<SourcePosition> {
		let index = self.mappings.partition_point(|mapping| {
			let line = mapping.generated_line as usize;
			!(line > position.line || line == position.line && mapping.generated_column as usize > position.column)
		});
		let mapping = self.mappings.get(index.checked_sub(1)?)?;
		Some(SourcePosition {
			line: mapping.source_line as usize,
			column: mapping.source_column as usize,
		})
	}
}

/// `composeSourceTraceMaps`: `outer` traced through `inner`.
#[must_use]
pub fn compose_source_trace_maps(
	outer: Option<Arc<SourceTraceMap>>,
	inner: Option<Arc<SourceTraceMap>>,
) -> Option<Arc<SourceTraceMap>> {
	let Some(outer) = outer else {
		return inner;
	};
	let Some(inner) = inner else {
		return Some(outer);
	};
	let mut composed = SourceTraceMap {
		file_name: inner.file_name.clone(),
		text: inner.text.clone(),
		mappings: Vec::with_capacity(outer.mappings.len()),
	};
	for mapping in &outer.mappings {
		let Some(original) = inner.original_position(SourcePosition {
			line: mapping.source_line as usize,
			column: mapping.source_column as usize,
		}) else {
			continue;
		};
		composed.mappings.push(TraceMapping {
			generated_line: mapping.generated_line,
			generated_column: mapping.generated_column,
			source_line: u32::try_from(original.line).unwrap_or(u32::MAX),
			source_column: u32::try_from(original.column).unwrap_or(u32::MAX),
		});
	}
	Some(Arc::new(composed))
}

/// `diagnosticTraces`: a compile's transformer traces, keyed by `normalizeSourceFilePath`. Empty
/// for a project with no transformer plugins, where the compiled text is the text on disk.
#[derive(Clone, Debug, Default)]
pub struct DiagnosticTraces(pub HashMap<String, Arc<SourceTraceMap>>);

impl DiagnosticTraces {
	/// Whether there are no traces.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	/// The trace of a file, by any spelling of its path.
	#[must_use]
	pub fn get(&self, path: &str) -> Option<&Arc<SourceTraceMap>> {
		self.0.get(&normalize_source_file_path(path))
	}

	/// `remap`: rewrites a diagnostic whose position indexes the reprinted `generated` text so it
	/// indexes the original file instead.
	#[must_use]
	pub fn remap(&self, mut info: DiagnosticInfo, generated: &str) -> DiagnosticInfo {
		if self.0.is_empty() || info.file_name.is_empty() {
			return info;
		}
		let Some(trace) = self.get(&info.file_name) else {
			return info;
		};
		let Some((line, column)) = utf16_position(generated, info.offset) else {
			return info;
		};
		let Some(original) = trace.original_position(SourcePosition { line, column }) else {
			return info;
		};
		let Some(offset) = byte_offset_of(&trace.text, original.line, original.column) else {
			return info;
		};
		info.offset = offset;
		(info.line, info.col) = line_col_in(&trace.text, offset);
		// The span is measured in the reprinted text; nothing guarantees it fits what is left of
		// the original file.
		let remaining = trace.text.len() - offset;
		info.len = info.len.min(remaining);
		info
	}

	/// `remapAll`: [`DiagnosticTraces::remap`] over diagnostics from the same generated text.
	#[must_use]
	pub fn remap_all(&self, infos: Vec<DiagnosticInfo>, generated: &str) -> Vec<DiagnosticInfo> {
		if self.0.is_empty() {
			return infos;
		}
		infos.into_iter().map(|info| self.remap(info, generated)).collect()
	}
}

/// `utf16Position`: a byte offset as the 0-based line and UTF-16 column a source map records.
#[must_use]
pub fn utf16_position(text: &str, offset: usize) -> Option<(usize, usize)> {
	if offset > text.len() {
		return None;
	}
	let bytes = text.as_bytes();
	let mut line = 0;
	let mut line_start = 0;
	for (index, &byte) in bytes[..offset].iter().enumerate() {
		if byte == b'\n' {
			line += 1;
			line_start = index + 1;
		}
	}
	let column = rotor_luau::utf8::runes(&bytes[line_start..offset])
		.map(|(rune, _)| rotor_luau::utf8::utf16_units(rune) as usize)
		.sum();
	Some((line, column))
}

/// `byteOffsetOf`: [`utf16_position`]'s inverse.
#[must_use]
pub fn byte_offset_of(text: &str, line: usize, column: usize) -> Option<usize> {
	let bytes = text.as_bytes();
	let mut offset = 0;
	for _ in 0..line {
		let next = bytes[offset..].iter().position(|&byte| byte == b'\n')?;
		offset += next + 1;
	}
	let mut units = 0;
	let mut index = offset;
	for (rune, width) in rotor_luau::utf8::runes(&bytes[offset..]) {
		if units >= column {
			return Some(index);
		}
		if rune == Some('\n') {
			return None;
		}
		units += rotor_luau::utf8::utf16_units(rune) as usize;
		index += width;
	}
	(units >= column).then_some(text.len())
}

#[cfg(test)]
#[path = "tests/source_trace.rs"]
mod tests;

/// `json.Unmarshal` into Go's `rawSourceMap`: its version and mappings, or Go's error. A syntax
/// error comes first; then the first member of the wrong type, in document order, though Go
/// decodes the rest; a later duplicate member wins.
fn decode_raw_source_map(raw: &str) -> Result<(i64, String), String> {
	use crate::gojson::Value;

	const fn kind(value: &Value) -> &'static str {
		match value {
			Value::Null => "null",
			Value::Bool(_) => "bool",
			Value::Number(_) => "number",
			Value::String(_) => "string",
			Value::Array(_) => "array",
			Value::Object(_) => "object",
		}
	}
	fn type_error(found: &str, field: &str, of: &str) -> String {
		format!("json: cannot unmarshal {found} into Go struct field rawSourceMap.{field} of type {of}")
	}
	fn string_error(value: &Value, field: &str) -> Option<String> {
		(!matches!(value, Value::Null | Value::String(_))).then(|| type_error(kind(value), field, "string"))
	}

	const FIELDS: [&str; 5] = ["version", "file", "sources", "sourcesContent", "mappings"];
	let value = crate::gojson::parse(raw.as_bytes())?;
	let members = match value {
		Value::Null => return Ok((0, String::new())),
		Value::Object(members) => members,
		other => {
			return Err(format!(
				"json: cannot unmarshal {} into Go value of type compile.rawSourceMap",
				kind(&other)
			));
		}
	};
	let mut version = 0;
	let mut mappings = String::new();
	let mut first_error = None;
	for (key, value) in &members {
		let Some(field) = FIELDS
			.iter()
			.find(|field| **field == key)
			.or_else(|| FIELDS.iter().find(|field| folds_to(key, field)))
		else {
			continue;
		};
		let error = match (*field, value) {
			(_, Value::Null) => None,
			("version", Value::Number(literal)) => literal.parse::<i64>().map_or_else(
				|_| Some(type_error(&format!("number {literal}"), field, "int")),
				|number| {
					version = number;
					None
				},
			),
			("version", other) => Some(type_error(kind(other), field, "int")),
			("mappings", Value::String(text)) => {
				text.clone_into(&mut mappings);
				None
			}
			("sources" | "sourcesContent", Value::Array(items)) => {
				items.iter().find_map(|item| string_error(item, field))
			}
			("sources" | "sourcesContent", other) => Some(type_error(kind(other), field, "[]string")),
			(_, other) => string_error(other, field),
		};
		if first_error.is_none() {
			first_error = error;
		}
	}
	first_error.map_or(Ok((version, mappings)), Err)
}

/// Whether Go's `json` matches the member `key` to the ASCII field name `field`: equal under
/// Unicode simple case folding, where of the non-ASCII letters only `ſ` folds to a letter of these
/// names.
fn folds_to(key: &str, field: &str) -> bool {
	let mut key = key.chars();
	for expected in field.chars() {
		let Some(actual) = key.next() else {
			return false;
		};
		let actual = if actual == '\u{17F}' { 's' } else { actual };
		if !actual.eq_ignore_ascii_case(&expected) {
			return false;
		}
	}
	key.next().is_none()
}
