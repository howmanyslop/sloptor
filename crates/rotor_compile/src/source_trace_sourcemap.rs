//! `source_trace_sourcemap.go`: rewriting a rendered file's V3 source map, its paths relative to
//! the output and its positions through a transformer trace.

use rotor_tsrs::tspath;

use crate::declmap::BASE64_VLQ_ALPHABET;
use crate::gojson;
use crate::source_trace::{SourcePosition, SourceTraceMap};

/// `rawSourceMap`
struct RawSourceMap {
	version: i64,
	file: String,
	sources: Option<Vec<String>>,
	sources_content: Option<Vec<String>>,
	mappings: String,
}

fn string_list(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
	let array = value?.as_array()?;
	Some(
		array
			.iter()
			.map(|value| value.as_str().unwrap_or_default().to_owned())
			.collect(),
	)
}

impl RawSourceMap {
	fn parse(raw: &str) -> Result<Self, String> {
		let value: serde_json::Value =
			serde_json::from_str(raw).map_err(|error| format!("parse Luau source map: {error}"))?;
		let source_map = Self {
			version: value.get("version").and_then(serde_json::Value::as_i64).unwrap_or(0),
			file: value
				.get("file")
				.and_then(serde_json::Value::as_str)
				.unwrap_or_default()
				.to_owned(),
			sources: string_list(value.get("sources")),
			sources_content: string_list(value.get("sourcesContent")),
			mappings: value
				.get("mappings")
				.and_then(serde_json::Value::as_str)
				.unwrap_or_default()
				.to_owned(),
		};
		if source_map.version != 3 {
			return Err(format!(
				"parse Luau source map: version = {}, want 3",
				source_map.version
			));
		}
		Ok(source_map)
	}

	/// Go's `json.Marshal(rawSourceMap)`.
	fn encode(&self) -> String {
		let list = |values: &Option<Vec<String>>| {
			values
				.as_ref()
				.map_or_else(|| "null".to_owned(), |values| gojson::string_array(values))
		};
		gojson::Object::new()
			.int("version", self.version)
			.string("file", &self.file)
			.raw("sources", &list(&self.sources))
			.raw("sourcesContent", &list(&self.sources_content))
			.string("mappings", &self.mappings)
			.finish()
	}
}

/// `rewriteSourceMapPaths`: `file` becomes the output's base name and each source relative to
/// the output's directory, as TypeScript's source-map generator does.
pub(crate) fn rewrite_source_map_paths(raw: &str, generated_file_name: &str) -> Result<String, String> {
	let mut source_map = RawSourceMap::parse(raw)?;
	let generated_file_name = tspath::normalize_path(generated_file_name);
	let generated_directory = tspath::get_directory_path(&generated_file_name);
	let options = tspath::ComparePathsOptions {
		use_case_sensitive_file_names: rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names(),
		current_directory: String::new(),
	};
	source_map.file = tspath::get_base_file_name(&generated_file_name);
	if let Some(sources) = &mut source_map.sources {
		for source in sources {
			*source = tspath::get_relative_path_to_directory_or_url(
				&generated_directory,
				&tspath::normalize_path(source),
				true,
				&options,
			);
		}
	}
	Ok(source_map.encode())
}

/// `emittedMapping`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EmittedMapping {
	pub(crate) generated_line: usize,
	pub(crate) generated_column: usize,
	pub(crate) source: Option<SourcePosition>,
}

/// `rewriteSourceMapWithTrace`: maps every source position through `trace` to the original file.
pub(crate) fn rewrite_source_map_with_trace(raw: &str, trace: &SourceTraceMap) -> Result<String, String> {
	let mut source_map = RawSourceMap::parse(raw)?;
	let mut mappings: Vec<EmittedMapping> = Vec::new();
	let mut decoder = rotor_tsrs::sourcemap::decode_mappings(&source_map.mappings);
	for mapping in decoder.values() {
		let mut emitted = EmittedMapping {
			generated_line: mapping.generated_line as usize,
			generated_column: mapping.generated_character as usize,
			source: None,
		};
		if mapping.is_source_mapping()
			&& let (Some(line), Some(column)) = (mapping.source_line, mapping.source_character)
		{
			emitted.source = trace.original_position(SourcePosition {
				line: line as usize,
				column: column as usize,
			});
		}
		if let Some(previous) = mappings.last()
			&& previous.generated_line == emitted.generated_line
			&& previous.source == emitted.source
		{
			continue;
		}
		mappings.push(emitted);
	}
	if let Some(error) = decoder.error() {
		return Err(format!("parse Luau source map mappings: {error}"));
	}
	source_map.sources = Some(vec![trace.original_source_file_name().to_owned()]);
	source_map.sources_content = Some(vec![trace.original_source_text().to_owned()]);
	source_map.mappings = encode_emitted_mappings(&mappings);
	Ok(source_map.encode())
}

fn to_i64(value: usize) -> i64 {
	i64::try_from(value).unwrap_or(i64::MAX)
}

/// `encodeEmittedMappings`
pub(crate) fn encode_emitted_mappings(mappings: &[EmittedMapping]) -> String {
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
		append_vlq(
			&mut builder,
			to_i64(mapping.generated_column) - to_i64(last_generated_column),
		);
		last_generated_column = mapping.generated_column;
		if let Some(source) = mapping.source {
			append_vlq(&mut builder, 0);
			append_vlq(&mut builder, to_i64(source.line) - to_i64(last_source_line));
			append_vlq(&mut builder, to_i64(source.column) - to_i64(last_source_column));
			last_source_line = source.line;
			last_source_column = source.column;
		}
		first_mapping_on_line = false;
	}
	builder
}

/// `appendVLQ`
pub(crate) fn append_vlq(builder: &mut String, value: i64) {
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
