//! `rbxts_options.go`: the tsconfig `rbxts` key (`getTsConfigProjectOptions`). Parent `extends`
//! blocks merge first, the child overrides them, and paths resolve where they are declared.

use std::collections::HashSet;

use rotor_support::{gopath, gostr, logservice};
use serde_json::{Map, Value};

use crate::compile::Error;

/// `RbxtsOptions`: the validated partial option set a tsconfig declares. `None` is an absent
/// field, distinct from `false`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RbxtsOptions {
	/// `allowCommentDirectives`
	pub allow_comment_directives: Option<bool>,
	/// `includePath`, resolved against the declaring config.
	pub include_path: Option<String>,
	/// `logTruthyChanges`
	pub log_truthy_changes: Option<bool>,
	/// `luau`
	pub luau: Option<bool>,
	/// `luauTypes`
	pub luau_types: Option<bool>,
	/// `nativeTypes`
	pub native_types: Option<bool>,
	/// `noInclude`
	pub no_include: Option<bool>,
	/// `optimizedArrayAppends`
	pub optimized_array_appends: Option<bool>,
	/// `optimizedLoops`
	pub optimized_loops: Option<bool>,
	/// `rojo`, resolved against the declaring config.
	pub rojo: Option<String>,
	/// `type`: `game`, `model` or `package`.
	pub project_type: Option<String>,
	/// The configs of the chain that declared options, when read with the chain.
	pub config_files: Vec<String>,
}

/// `ReadRbxtsOptions`
///
/// # Errors
///
/// An unreadable or unparsable config, an `extends` cycle or bad `extends`, or an invalid
/// `rbxts` value.
pub fn read_rbxts_options(ts_config_path: &str) -> Result<Option<RbxtsOptions>, Error> {
	read_rbxts_options_from(ts_config_path, &mut HashSet::new(), None)
}

/// `ReadRbxtsOptionsWithChain`: [`read_rbxts_options`] and every config of the chain it read.
///
/// # Errors
///
/// See [`read_rbxts_options`].
pub fn read_rbxts_options_with_chain(ts_config_path: &str) -> Result<(Option<RbxtsOptions>, Vec<String>), Error> {
	let mut chain = Vec::new();
	let options = read_rbxts_options_from(ts_config_path, &mut HashSet::new(), Some(&mut chain))?;
	Ok((options, chain))
}

fn read_rbxts_options_from(
	ts_config_path: &str,
	visited: &mut HashSet<String>,
	mut chain: Option<&mut Vec<String>>,
) -> Result<Option<RbxtsOptions>, Error> {
	let normalized = gopath::abs(ts_config_path)
		.map_err(|error| Error::message(format!("resolve tsconfig {}: {error}", gostr::quote(ts_config_path))))?;
	let normalized = gopath::clean(&normalized);
	if !visited.insert(normalized.clone()) {
		return Err(Error::message(format!("tsconfig extends cycle at {normalized}")));
	}
	if let Some(chain) = chain.as_deref_mut() {
		chain.push(normalized.clone());
	}

	let data = match std::fs::read(&normalized) {
		Ok(data) => data,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
		Err(error) => {
			return Err(Error::message(format!(
				"read tsconfig {}: {}",
				gostr::quote(&normalized),
				gostr::path_error("open", &normalized, &error)
			)));
		}
	};
	let text = rotor_tsrs::tsconfig::strip_jsonc(&String::from_utf8_lossy(&data));
	let config: Value = serde_json::from_str(&text)
		.map_err(|error| Error::message(format!("failed to parse tsconfig at {normalized}: {error}")))?;

	let inherited = if let Some(Value::String(extends)) = config.get("extends") {
		let parent =
			rotor_tsrs::tsconfig::resolve_extends_path(&gopath::dir(&normalized), extends).map_err(Error::Message)?;
		read_rbxts_options_from(&parent, visited, chain.as_deref_mut())?
	} else {
		None
	};

	let Some(raw) = config.get("rbxts") else {
		return Ok(inherited);
	};
	let Value::Object(object) = raw else {
		return Err(Error::message(format!(
			"tsconfig \"rbxts\" field at {normalized} must be an object"
		)));
	};
	let current = parse_rbxts_options(object, &normalized)?;
	let mut result = merge_rbxts_options(inherited, Some(current));
	if let (Some(chain), Some(result)) = (chain, result.as_mut()) {
		result.config_files.clone_from(chain);
	}
	Ok(result)
}

/// `rbxtsOptionKeys`
const RBXTS_OPTION_KEYS: [&str; 11] = [
	"allowCommentDirectives",
	"includePath",
	"logTruthyChanges",
	"luau",
	"luauTypes",
	"nativeTypes",
	"noInclude",
	"optimizedArrayAppends",
	"optimizedLoops",
	"rojo",
	"type",
];

/// `parseRbxtsOptions`. Unknown keys warn, in key order (Go ranges over its map).
fn parse_rbxts_options(raw: &Map<String, Value>, config_path: &str) -> Result<RbxtsOptions, Error> {
	for key in raw.keys() {
		if !RBXTS_OPTION_KEYS.contains(&key.as_str()) {
			logservice::warn(&format!(
				"Unknown \"rbxts\" option \"{key}\" in {config_path} (ignored)"
			));
		}
	}

	let mut result = RbxtsOptions {
		allow_comment_directives: rbxts_boolean(raw, "allowCommentDirectives", config_path)?,
		log_truthy_changes: rbxts_boolean(raw, "logTruthyChanges", config_path)?,
		luau: rbxts_boolean(raw, "luau", config_path)?,
		luau_types: rbxts_boolean(raw, "luauTypes", config_path)?,
		no_include: rbxts_boolean(raw, "noInclude", config_path)?,
		native_types: rbxts_boolean(raw, "nativeTypes", config_path)?,
		optimized_array_appends: rbxts_boolean(raw, "optimizedArrayAppends", config_path)?,
		optimized_loops: rbxts_boolean(raw, "optimizedLoops", config_path)?,
		include_path: rbxts_path(raw, "includePath", config_path)?,
		rojo: rbxts_path(raw, "rojo", config_path)?,
		..RbxtsOptions::default()
	};
	if let Some(value) = raw.get("type") {
		let Value::String(text) = value else {
			return Err(rbxts_type_error(
				config_path,
				"type",
				"\"game\", \"model\" or \"package\"",
				value,
			));
		};
		if !matches!(text.as_str(), "game" | "model" | "package") {
			return Err(Error::message(format!(
				"invalid \"rbxts\" config in {config_path}: type must be \"game\", \"model\" or \"package\" (was {})",
				gostr::quote(text)
			)));
		}
		result.project_type = Some(text.clone());
	}
	Ok(result)
}

/// `rbxtsBoolean`
fn rbxts_boolean(raw: &Map<String, Value>, key: &str, config_path: &str) -> Result<Option<bool>, Error> {
	match raw.get(key) {
		None => Ok(None),
		Some(Value::Bool(value)) => Ok(Some(*value)),
		Some(value) => Err(rbxts_type_error(config_path, key, "a boolean", value)),
	}
}

/// `rbxtsPath`: a string, resolved against the declaring config's directory when relative.
fn rbxts_path(raw: &Map<String, Value>, key: &str, config_path: &str) -> Result<Option<String>, Error> {
	match raw.get(key) {
		None => Ok(None),
		Some(Value::String(text)) => {
			if !text.is_empty() && !gopath::is_abs(text) {
				return Ok(Some(gopath::join(&[
					&gopath::dir(config_path),
					&gopath::from_slash(text),
				])));
			}
			Ok(Some(text.clone()))
		}
		Some(value) => Err(rbxts_type_error(config_path, key, "a string", value)),
	}
}

/// `rbxtsTypeError`
fn rbxts_type_error(config_path: &str, key: &str, expected: &str, value: &Value) -> Error {
	Error::message(format!(
		"invalid \"rbxts\" config in {config_path}: {key} must be {expected} (was {})",
		rbxts_value_description(value)
	))
}

/// `rbxtsValueDescription`
fn rbxts_value_description(value: &Value) -> String {
	match value {
		Value::String(text) => gostr::quote(text),
		Value::Number(_) => "a number".to_owned(),
		Value::Bool(_) => "a boolean".to_owned(),
		Value::Null => "null".to_owned(),
		Value::Array(_) => "a []interface {}".to_owned(),
		Value::Object(_) => "a map[string]interface {}".to_owned(),
	}
}

/// `mergeRbxtsOptions`: the overlay's declared fields over the base's.
#[must_use]
pub fn merge_rbxts_options(base: Option<RbxtsOptions>, overlay: Option<RbxtsOptions>) -> Option<RbxtsOptions> {
	let Some(mut result) = base else {
		return overlay;
	};
	let Some(overlay) = overlay else {
		return Some(result);
	};
	macro_rules! layer {
		($($field:ident),*) => {
			$(if overlay.$field.is_some() {
				result.$field = overlay.$field;
			})*
		};
	}
	layer!(
		allow_comment_directives,
		include_path,
		log_truthy_changes,
		luau,
		luau_types,
		native_types,
		no_include,
		optimized_array_appends,
		optimized_loops,
		rojo,
		project_type
	);
	Some(result)
}
