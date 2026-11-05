//! `project_references.go`: a config's project references, read without TypeScript.
//!
//! It also tells whether the config is a coordinator (a solution config with references and no
//! root files of its own) and lists the configs of its `extends` chain.

use std::collections::HashSet;

use rotor_support::{gopath, gostr};

use crate::compile::Error;
use crate::gojson::{self, Value};

/// `readProjectReferencePaths`: the reference config paths, whether the config is a
/// coordinator, and every config of its `extends` chain.
///
/// # Errors
///
/// The config, or a config it extends, does not read or parse.
pub(crate) fn read_project_reference_paths(ts_config_path: &str) -> Result<(Vec<String>, bool, Vec<String>), Error> {
	let config = read_project_config(ts_config_path)?;
	let mut paths = Vec::new();
	if let Some(Value::Array(references)) = config.get("references") {
		for reference in references {
			let Value::Object(_) = reference else {
				continue;
			};
			let Some(Value::String(path)) = reference.get("path") else {
				continue;
			};
			let mut path = path.clone();
			if !gopath::is_abs(&path) {
				path = gopath::join(&[&gopath::dir(ts_config_path), &gopath::from_slash(&path)]);
			}
			if go_ext(&path) != ".json" {
				path = gopath::join(&[&path, "tsconfig.json"]);
			}
			paths.push(gopath::clean(&path));
		}
	}
	let file_specs = read_effective_project_file_specs(ts_config_path, &mut HashSet::new())?;
	let empty_files = file_specs.files.present && file_specs.files.valid && file_specs.files.len == 0;
	let empty_include = !file_specs.include.present || file_specs.include.valid && file_specs.include.len == 0;
	let coordinator = !paths.is_empty() && empty_files && empty_include;
	Ok((paths, coordinator, file_specs.config_paths))
}

/// Go's `filepath.Ext`.
fn go_ext(path: &str) -> &str {
	let separator = gopath::NATIVE.separator();
	for (index, character) in path.char_indices().rev() {
		if character == separator || character == '/' && cfg!(windows) {
			break;
		}
		if character == '.' {
			return &path[index..];
		}
	}
	""
}

/// `projectFileSpec`: a root file spec (`files` or `include`), as declared.
#[derive(Clone, Copy, Default)]
struct ProjectFileSpec {
	len: usize,
	present: bool,
	valid: bool,
}

/// `projectFileSpecs`
#[derive(Default)]
struct ProjectFileSpecs {
	files: ProjectFileSpec,
	include: ProjectFileSpec,
	config_paths: Vec<String>,
}

/// `readEffectiveProjectFileSpecs`: TypeScript's config inheritance for the root file specs.
/// `files` and `include` inherit independently; with several bases, a later base overrides only
/// what it declares.
fn read_effective_project_file_specs(
	ts_config_path: &str,
	visiting: &mut HashSet<String>,
) -> Result<ProjectFileSpecs, Error> {
	let normalized = gopath::abs(ts_config_path).map_err(|error| Error::message(error.to_string()))?;
	let normalized = gopath::clean(&normalized);
	if visiting.contains(&normalized) {
		return Err(Error::message(format!("tsconfig extends cycle at {normalized}")));
	}
	visiting.insert(normalized.clone());
	let result = read_effective_project_file_specs_at(&normalized, visiting);
	visiting.remove(&normalized);
	result
}

fn read_effective_project_file_specs_at(
	normalized: &str,
	visiting: &mut HashSet<String>,
) -> Result<ProjectFileSpecs, Error> {
	let config = read_project_config(normalized)?;
	let mut result = ProjectFileSpecs {
		config_paths: vec![normalized.to_owned()],
		..ProjectFileSpecs::default()
	};
	for extends in project_extends(&config, normalized)? {
		let parent = resolve_extended_config(normalized, &extends)?;
		let parent_specs = read_effective_project_file_specs(&parent, visiting)?;
		if parent_specs.files.present {
			result.files = parent_specs.files;
		}
		if parent_specs.include.present {
			result.include = parent_specs.include;
		}
		result.config_paths.extend(parent_specs.config_paths);
	}
	if let Some(value) = config.get("files") {
		result.files = project_file_spec_from(value);
	}
	if let Some(value) = config.get("include") {
		result.include = project_file_spec_from(value);
	}
	Ok(result)
}

/// `readProjectConfig`: the config as `json.Unmarshal` into a `map[string]any` reads it, after
/// stripping comments and trailing commas. `null` reads as an empty map.
pub(crate) fn read_project_config(ts_config_path: &str) -> Result<Value, Error> {
	let data = std::fs::read(ts_config_path)
		.map_err(|error| Error::message(gostr::path_error("open", ts_config_path, &error)))?;
	let text = rotor_tsrs::tsconfig::strip_jsonc(&String::from_utf8_lossy(&data));
	let parse_error = |detail: &str| Error::message(format!("failed to parse tsconfig at {ts_config_path}: {detail}"));
	match gojson::parse(text.as_bytes()).map_err(|error| parse_error(&error))? {
		Value::Null => Ok(Value::Object(Vec::new())),
		value @ Value::Object(_) => Ok(value),
		other => Err(parse_error(&format!(
			"json: cannot unmarshal {} into Go value of type map[string]interface {{}}",
			go_json_kind(&other)
		))),
	}
}

/// The JSON kind `json.UnmarshalTypeError` names.
const fn go_json_kind(value: &Value) -> &'static str {
	match value {
		Value::Null => "null",
		Value::Bool(_) => "bool",
		Value::Number(_) => "number",
		Value::String(_) => "string",
		Value::Array(_) => "array",
		Value::Object(_) => "object",
	}
}

/// `projectExtends`
fn project_extends(config: &Value, ts_config_path: &str) -> Result<Vec<String>, Error> {
	match config.get("extends") {
		None => Ok(Vec::new()),
		Some(Value::String(single)) => Ok(vec![single.clone()]),
		Some(Value::Array(entries)) => entries
			.iter()
			.map(|entry| {
				entry.as_str().map(str::to_owned).ok_or_else(|| {
					Error::message(format!(
						"tsconfig extends at {ts_config_path} must contain only strings"
					))
				})
			})
			.collect(),
		Some(_) => Err(Error::message(format!(
			"tsconfig extends at {ts_config_path} must be a string or array of strings"
		))),
	}
}

/// `projectFileSpecFrom`
const fn project_file_spec_from(value: &Value) -> ProjectFileSpec {
	match value {
		Value::Array(values) => ProjectFileSpec {
			len: values.len(),
			present: true,
			valid: true,
		},
		_ => ProjectFileSpec {
			len: 0,
			present: true,
			valid: false,
		},
	}
}

/// `resolveExtendedConfig` (`transformer_order.go`): a package specifier resolves like
/// TypeScript's `extends`; a path resolves against the config, `.json` appended when the bare
/// path is not a file.
#[expect(
	clippy::case_sensitive_file_extension_comparisons,
	reason = "Go appends .json unless the path ends with it exactly"
)]
pub(crate) fn resolve_extended_config(config_path: &str, extended: &str) -> Result<String, Error> {
	if extended.is_empty() {
		return Err(Error::message(format!("{config_path}: extends path is empty")));
	}
	let quoted = gostr::quote(extended);
	if !gopath::is_abs(extended) && !extended.starts_with('.') {
		return rotor_tsrs::tsconfig::resolve_extends_path(&gopath::dir(config_path), extended)
			.map_err(|_| Error::message(format!("{config_path}: resolve extends {quoted}: config not found")));
	}
	let mut path = if gopath::is_abs(extended) {
		extended.to_owned()
	} else {
		gopath::join(&[&gopath::dir(config_path), &gopath::from_slash(extended)])
	};
	match std::fs::metadata(&path) {
		Ok(metadata) if metadata.is_file() => return Ok(gopath::clean(&path)),
		Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
			return Err(Error::message(format!(
				"{config_path}: resolve extends {quoted}: {}",
				gostr::path_error("stat", &path, &error)
			)));
		}
		_ => {}
	}
	if !path.ends_with(".json") {
		path.push_str(".json");
	}
	if let Err(error) = std::fs::metadata(&path) {
		return Err(Error::message(format!(
			"{config_path}: resolve extends {quoted}: {}",
			gostr::path_error("stat", &path, &error)
		)));
	}
	Ok(gopath::clean(&path))
}
