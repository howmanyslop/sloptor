//! `build_info_codec.go`: decoding `flamework.build` and encoding it in field order.

use std::collections::{BTreeMap, HashMap};

use crate::build_info::{BuildClass, BuildDecorator, BuildGlobs, BuildInfo, BuildMetadata, GoStringSlicesMap};
use crate::json_syntax::{self, Value, type_error, validate_json_syntax};
use crate::runtime_config::parse_runtime_config;
use crate::{Error, ErrorKind};

/// `orderedStringMap`: a string map that encodes in insertion order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrderedStringMap {
	pub(crate) order: Vec<String>,
	pub(crate) values: HashMap<String, String>,
}

impl OrderedStringMap {
	pub(crate) fn set(&mut self, key: &str, value: &str) {
		if !self.values.contains_key(key) {
			self.order.push(key.to_owned());
		}
		self.values.insert(key.to_owned(), value.to_owned());
	}

	/// `MarshalJSON`, without HTML escaping.
	pub(crate) fn marshal_json(&self) -> String {
		let mut out = String::from("{");
		for (index, key) in self.order.iter().enumerate() {
			if index > 0 {
				out.push(',');
			}
			json_syntax::write_string(&mut out, key, false);
			out.push(':');
			json_syntax::write_string(&mut out, self.values.get(key).map_or("", String::as_str), false);
		}
		out.push('}');
		out
	}
}

fn invalid(rest: &str) -> Error {
	Error::kinded(ErrorKind::InvalidBuildInfo, rest)
}

impl BuildInfo {
	/// `MarshalOrderedJSON`: the build info in field order, tab-indented, without a trailing
	/// newline.
	///
	/// # Errors
	///
	/// An extra field holds invalid JSON.
	pub fn marshal_ordered_json(&self) -> Result<Vec<u8>, Error> {
		let mut compact = String::from("{");
		let mut written = 0;
		for name in &self.field_order {
			let Some(value) = self.field_json(name) else {
				continue;
			};
			if written > 0 {
				compact.push(',');
			}
			json_syntax::write_string(&mut compact, name, false);
			compact.push(':');
			compact.push_str(&value);
			written += 1;
		}
		compact.push('}');
		json_syntax::indent(compact.as_bytes(), "", "\t")
			.ok_or_else(|| Error::msg("flamework: indent build info: invalid character in build info"))
	}

	pub(crate) fn decode(&mut self, data: &[u8]) -> Result<(), Error> {
		validate_json_syntax(data).map_err(|error| error.under(ErrorKind::InvalidBuildInfo))?;
		let document = json_syntax::parse(data).map_err(|message| invalid(&format!(": {message}")))?;
		let Value::Object(members) = document else {
			return Err(invalid(": expected object"));
		};
		self.field_order.clear();
		self.identifiers = OrderedStringMap::default();
		self.reverse_identifiers.clear();
		let mut required = [("version", false), ("flameworkVersion", false), ("identifiers", false)];
		for member in &members {
			self.ensure_field(&member.key);
			if let Some(entry) = required.iter_mut().find(|(name, _)| *name == member.key) {
				entry.1 = true;
			}
			self.decode_field(&member.key, &member.value, &member.raw)?;
		}
		// Go ranges over a map here; the order only picks which missing field is reported.
		for (name, present) in required {
			if !present {
				return Err(invalid(&format!(": missing {name}")));
			}
		}
		Ok(())
	}

	fn decode_field(&mut self, name: &str, value: &Value, raw: &[u8]) -> Result<(), Error> {
		match name {
			"version" => {
				reject_null(value, name)?;
				match value {
					Value::Number(number) => {
						self.version = number
							.parse::<f64>()
							.ok()
							.filter(|parsed| parsed.is_finite())
							.ok_or_else(|| field_error(name, &type_error(value, "float64")))?;
					}
					_ => return Err(field_error(name, &type_error(value, "float64"))),
				}
				Ok(())
			}
			"flameworkVersion" => {
				reject_null(value, name)?;
				self.flamework_version = decode_string_value(value).map_err(|message| field_error(name, &message))?;
				Ok(())
			}
			"identifierPrefix" => {
				reject_null(value, name)?;
				self.prefix = Some(decode_string_value(value).map_err(|message| field_error(name, &message))?);
				Ok(())
			}
			"salt" => {
				reject_null(value, name)?;
				self.salt = Some(decode_string_value(value).map_err(|message| field_error(name, &message))?);
				Ok(())
			}
			"metadata" => {
				let metadata = decode_build_metadata(value);
				match metadata {
					Ok(metadata) => {
						self.metadata = Some(metadata);
						Ok(())
					}
					Err(error) => {
						self.metadata = None;
						Err(error)
					}
				}
			}
			"classes" => {
				reject_null(value, name)?;
				self.classes = Some(decode_classes(value).map_err(|message| field_error(name, &message))?);
				Ok(())
			}
			"identifiers" => {
				let values = decode_ordered_string_map(value)?;
				for internal_id in &values.order {
					if let Some(id) = values.values.get(internal_id) {
						// Go ranges over a map; with duplicate identifiers its reverse entry is
						// whichever comes last in map order. This takes source order.
						self.reverse_identifiers.insert(id.clone(), internal_id.clone());
					}
				}
				self.identifiers = values;
				Ok(())
			}
			"stringHashes" => {
				self.string_hashes = Some(decode_ordered_string_map(value)?);
				Ok(())
			}
			_ => {
				self.extras.insert(name.to_owned(), raw.to_vec());
				Ok(())
			}
		}
	}
}

fn reject_null(value: &Value, name: &str) -> Result<(), Error> {
	if matches!(value, Value::Null) {
		return Err(invalid(&format!(": {name} must not be null")));
	}
	Ok(())
}

fn field_error(name: &str, message: &str) -> Error {
	invalid(&format!(": {name}: {message}"))
}

fn decode_string_value(value: &Value) -> Result<String, String> {
	match value {
		Value::String(text) => Ok(text.clone()),
		other => Err(type_error(other, "string")),
	}
}

/// `json.Unmarshal` into `[]BuildClass`: `null` elements are zero classes, `null` fields keep
/// their zero values, unknown fields are ignored, and keys match case-insensitively.
fn decode_classes(value: &Value) -> Result<Vec<BuildClass>, String> {
	let Value::Array(elements) = value else {
		return Err(type_error(value, "[]flamework.BuildClass"));
	};
	let mut classes = Vec::with_capacity(elements.len());
	let mut first_error: Option<String> = None;
	for element in elements {
		let mut class = BuildClass::default();
		match element {
			Value::Null => {}
			Value::Object(_) => {
				decode_struct_string(
					element,
					"filePath",
					"BuildClass",
					&mut class.file_path,
					&mut first_error,
				);
				decode_struct_string(
					element,
					"internalId",
					"BuildClass",
					&mut class.internal_id,
					&mut first_error,
				);
				match element.field("decorators") {
					None | Some(Value::Null) => {}
					Some(Value::Array(decorators)) => {
						for decorator in decorators {
							let mut entry = BuildDecorator::default();
							match decorator {
								Value::Null => {}
								Value::Object(_) => {
									decode_struct_string(
										decorator,
										"name",
										"BuildClass.decorators",
										&mut entry.name,
										&mut first_error,
									);
									decode_struct_string(
										decorator,
										"internalId",
										"BuildClass.decorators",
										&mut entry.internal_id,
										&mut first_error,
									);
								}
								other => {
									first_error.get_or_insert_with(|| {
										struct_field_error(other, "BuildClass.decorators", "flamework.BuildDecorator")
									});
								}
							}
							class.decorators.push(entry);
						}
					}
					Some(other) => {
						first_error.get_or_insert_with(|| {
							struct_field_error(other, "BuildClass.decorators", "[]flamework.BuildDecorator")
						});
					}
				}
			}
			other => {
				first_error.get_or_insert_with(|| type_error(other, "flamework.BuildClass"));
			}
		}
		classes.push(class);
	}
	first_error.map_or(Ok(classes), Err)
}

fn decode_struct_string(object: &Value, key: &str, owner: &str, target: &mut String, error: &mut Option<String>) {
	match object.field(key) {
		None | Some(Value::Null) => {}
		Some(Value::String(text)) => text.clone_into(target),
		Some(other) => {
			error.get_or_insert_with(|| struct_field_error(other, &format!("{owner}.{key}"), "string"));
		}
	}
}

fn struct_field_error(value: &Value, field: &str, go_type: &str) -> String {
	format!(
		"json: cannot unmarshal {} into Go struct field {field} of type {go_type}",
		value.kind_name()
	)
}

/// `decodeBuildMetadata`
fn decode_build_metadata(value: &Value) -> Result<BuildMetadata, Error> {
	if !matches!(value, Value::Object(_)) {
		return Err(invalid(": metadata must be object"));
	}
	let mut metadata = BuildMetadata::default();
	let members = value.map_members();
	if let Some(config) = members.iter().find(|member| member.key == "config") {
		let parsed = parse_runtime_config(&config.raw)
			.map_err(|error| invalid(&format!(": metadata config: {}", error.message())))?;
		metadata.config = Some(parsed);
	}
	if let Some(globs) = members.iter().find(|member| member.key == "globs") {
		metadata.globs = Some(decode_globs(&globs.value).ok_or_else(|| invalid(": metadata globs must be object"))?);
	}
	Ok(metadata)
}

fn decode_globs(value: &Value) -> Option<BuildGlobs> {
	if !matches!(value, Value::Object(_)) {
		return None;
	}
	Some(BuildGlobs {
		paths: decode_string_slices_map(value.field("paths"))?,
		origins: decode_string_slices_map(value.field("origins"))?,
	})
}

/// A `*map[string][]string` field: `None` (outer) on a type error.
fn decode_string_slices_map(value: Option<&Value>) -> Option<GoStringSlicesMap> {
	match value {
		None | Some(Value::Null) => Some(None),
		Some(Value::Object(_)) => {
			let mut map = BTreeMap::new();
			for member in value?.map_members() {
				let strings = match &member.value {
					Value::Null => None,
					Value::Array(elements) => {
						let mut strings = Vec::with_capacity(elements.len());
						for element in elements {
							match element {
								Value::String(text) => strings.push(text.clone()),
								Value::Null => strings.push(String::new()),
								_ => return None,
							}
						}
						Some(strings)
					}
					_ => return None,
				};
				map.insert(member.key.clone(), strings);
			}
			Some(Some(map))
		}
		Some(_) => None,
	}
}

/// `decodeOrderedStringMap`
fn decode_ordered_string_map(value: &Value) -> Result<OrderedStringMap, Error> {
	let Value::Object(members) = value else {
		return Err(invalid(": expected string map"));
	};
	let mut result = OrderedStringMap::default();
	for member in members {
		let text = match &member.value {
			Value::String(text) => text.clone(),
			Value::Null => String::new(),
			other => return Err(invalid(&format!(": string map value: {}", type_error(other, "string")))),
		};
		result.set(&member.key, &text);
	}
	Ok(result)
}
