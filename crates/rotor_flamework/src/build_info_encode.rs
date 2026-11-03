//! `build_info_encode.go`: each build-info field's JSON, as `json.Encoder` with HTML escaping off
//! writes it.

use std::collections::BTreeMap;

use crate::build_info::{BuildClass, BuildInfo, BuildMetadata, GoStrings};
use crate::json_syntax::{self, compact, format_float};

impl BuildInfo {
	/// `fieldJSON`: the field's JSON, or `None` when the field is absent.
	pub(crate) fn field_json(&self, name: &str) -> Option<String> {
		match name {
			"version" => Some(format_float(self.version)),
			"flameworkVersion" => Some(json_syntax::string(&self.flamework_version, false)),
			"identifierPrefix" => self.prefix.as_deref().map(|prefix| json_syntax::string(prefix, false)),
			"salt" => self.salt.as_deref().map(|salt| json_syntax::string(salt, false)),
			"metadata" => self.metadata.as_ref().map(marshal_metadata),
			"stringHashes" => self
				.string_hashes
				.as_ref()
				.map(super::build_info_codec::OrderedStringMap::marshal_json),
			"identifiers" => Some(self.identifiers.marshal_json()),
			"classes" => self.classes.as_deref().map(marshal_classes),
			_ => {
				let raw = self.extras.get(name)?;
				// The encoder compacts a RawMessage, without HTML escaping.
				Some(String::from_utf8_lossy(&compact(raw, false).unwrap_or_else(|| raw.clone())).into_owned())
			}
		}
	}
}

/// `BuildMetadata` through the encoder: `RuntimeConfig.MarshalJSON` (already HTML-escaped by its
/// own `json.Marshal`), then `globs`.
pub(crate) fn marshal_metadata(metadata: &BuildMetadata) -> String {
	let mut out = String::from("{");
	let first = metadata.config.as_ref().is_none_or(|config| {
		out.push_str("\"config\":");
		out.push_str(&config.marshal_json());
		false
	});
	if let Some(globs) = &metadata.globs {
		if !first {
			out.push(',');
		}
		out.push_str("\"globs\":{");
		let mut first_glob = true;
		for (key, map) in [("paths", &globs.paths), ("origins", &globs.origins)] {
			let Some(map) = map else {
				continue;
			};
			if !first_glob {
				out.push(',');
			}
			first_glob = false;
			json_syntax::write_string(&mut out, key, false);
			out.push(':');
			out.push_str(&marshal_string_slices_map(map, false));
		}
		out.push('}');
	}
	out.push('}');
	out
}

/// A `map[string][]string`: sorted keys, nil slices as `null`.
pub(crate) fn marshal_string_slices_map(map: &BTreeMap<String, GoStrings>, escape_html: bool) -> String {
	let mut out = String::from("{");
	for (index, (key, values)) in map.iter().enumerate() {
		if index > 0 {
			out.push(',');
		}
		json_syntax::write_string(&mut out, key, escape_html);
		out.push(':');
		match values {
			None => out.push_str("null"),
			Some(values) => {
				out.push('[');
				for (value_index, value) in values.iter().enumerate() {
					if value_index > 0 {
						out.push(',');
					}
					json_syntax::write_string(&mut out, value, escape_html);
				}
				out.push(']');
			}
		}
	}
	out.push('}');
	out
}

fn marshal_classes(classes: &[BuildClass]) -> String {
	let mut out = String::from("[");
	for (index, class) in classes.iter().enumerate() {
		if index > 0 {
			out.push(',');
		}
		let mut fields: Vec<String> = Vec::new();
		if !class.file_path.is_empty() {
			fields.push(format!("\"filePath\":{}", json_syntax::string(&class.file_path, false)));
		}
		if !class.internal_id.is_empty() {
			fields.push(format!(
				"\"internalId\":{}",
				json_syntax::string(&class.internal_id, false)
			));
		}
		if !class.decorators.is_empty() {
			let decorators: Vec<String> = class
				.decorators
				.iter()
				.map(|decorator| {
					let mut parts = Vec::new();
					if !decorator.name.is_empty() {
						parts.push(format!("\"name\":{}", json_syntax::string(&decorator.name, false)));
					}
					if !decorator.internal_id.is_empty() {
						parts.push(format!(
							"\"internalId\":{}",
							json_syntax::string(&decorator.internal_id, false)
						));
					}
					format!("{{{}}}", parts.join(","))
				})
				.collect();
			fields.push(format!("\"decorators\":[{}]", decorators.join(",")));
		}
		out.push('{');
		out.push_str(&fields.join(","));
		out.push('}');
	}
	out.push(']');
	out
}
