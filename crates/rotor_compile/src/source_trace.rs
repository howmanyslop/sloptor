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
		let source_map: serde_json::Value =
			serde_json::from_str(raw).map_err(|error| format!("parse transformer trace map: {error}"))?;
		let version = source_map
			.get("version")
			.and_then(serde_json::Value::as_i64)
			.unwrap_or(0);
		let mappings = source_map
			.get("mappings")
			.and_then(serde_json::Value::as_str)
			.unwrap_or("");
		Self::from_mappings(version, mappings, file_name, text)
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
