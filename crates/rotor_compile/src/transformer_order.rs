//! `transformer_order.go`: the effective transformer plugins of a tsconfig chain. `plugins` is an
//! array option, so `extends` replaces it: the nearest config that declares it wins.

use std::collections::HashSet;

use rotor_support::{gopath, gostr};

use crate::compile::Error;
use crate::gojson::{self, Value};

/// `transformerPluginConfig`: a plugin's `transform` and its raw JSON entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransformerPluginConfig {
	/// The `transform` specifier.
	pub transform: String,
	/// The entry exactly as written.
	pub raw: String,
}

impl TransformerPluginConfig {
	/// `marshalJSON`: the raw entry, or `{"transform": ...}` without one.
	#[must_use]
	pub fn marshal_json(&self) -> String {
		if !self.raw.is_empty() {
			return self.raw.clone();
		}
		gojson::Object::new().string("transform", &self.transform).finish()
	}
}

/// `effectiveTransformerPlugins`
///
/// # Errors
///
/// A config of the chain does not read or parse, an `extends` cycle or bad `extends`, or a
/// malformed `compilerOptions` or `plugins`.
pub fn effective_transformer_plugins(config_path: &str) -> Result<Vec<TransformerPluginConfig>, Error> {
	resolve_transformer_plugins(&gopath::clean(config_path), &mut HashSet::new()).map(|(plugins, _)| plugins)
}

/// The raw JSON text of a value, re-encoded.
fn encode(value: &Value) -> String {
	match value {
		Value::Null => "null".to_owned(),
		Value::Bool(flag) => flag.to_string(),
		Value::Number(text) => text.clone(),
		Value::String(text) => gojson::string(text),
		Value::Array(elements) => gojson::array(elements.iter().map(encode)),
		Value::Object(members) => {
			let mut object = gojson::Object::new();
			for (key, member) in members {
				object = object.raw(key, &encode(member));
			}
			object.finish()
		}
	}
}

/// `resolveTransformerPlugins`: the plugins and whether this chain declared `plugins`.
fn resolve_transformer_plugins(
	config_path: &str,
	active: &mut HashSet<String>,
) -> Result<(Vec<TransformerPluginConfig>, bool), Error> {
	let abs_path = gopath::abs(config_path)
		.map_err(|error| Error::message(format!("resolve tsconfig path {}: {error}", gostr::quote(config_path))))?;
	let abs_path = gopath::clean(&abs_path);
	if !active.insert(abs_path.clone()) {
		return Err(Error::message(format!("{abs_path}: circular tsconfig extends chain")));
	}
	let result = resolve_transformer_plugins_at(&abs_path, active);
	active.remove(&abs_path);
	result
}

fn resolve_transformer_plugins_at(
	abs_path: &str,
	active: &mut HashSet<String>,
) -> Result<(Vec<TransformerPluginConfig>, bool), Error> {
	let data = std::fs::read(abs_path).map_err(|error| {
		Error::message(format!(
			"{abs_path}: read extended tsconfig: {}",
			gostr::path_error("open", abs_path, &error)
		))
	})?;
	let text = rotor_tsrs::tsconfig::strip_jsonc(&String::from_utf8_lossy(&data));
	let root = gojson::parse(text.as_bytes())
		.map_err(|error| Error::message(format!("{abs_path}: parse tsconfig: {error}")))?;

	let mut inherited = Vec::new();
	let mut inherited_declared = false;
	let extends = match root.get("extends") {
		None | Some(Value::Null) => Vec::new(),
		Some(Value::String(single)) => vec![single.clone()],
		Some(Value::Array(multiple)) => multiple
			.iter()
			.map(|entry| entry.as_str().map(str::to_owned))
			.collect::<Option<Vec<_>>>()
			.ok_or_else(|| Error::message(format!("{abs_path}: extends must be a string or array of strings")))?,
		Some(_) => {
			return Err(Error::message(format!(
				"{abs_path}: extends must be a string or array of strings"
			)));
		}
	};
	for extended in extends {
		let base_path =
			rotor_tsrs::tsconfig::resolve_extends_path(&gopath::dir(abs_path), &extended).map_err(Error::Message)?;
		let (plugins, declared) = resolve_transformer_plugins(&base_path, active)?;
		if declared {
			inherited = plugins;
			inherited_declared = true;
		}
	}

	let Some(compiler_options) = root.get("compilerOptions") else {
		return Ok((inherited, inherited_declared));
	};
	if !matches!(compiler_options, Value::Object(_) | Value::Null) {
		return Err(Error::message(format!("{abs_path}: compilerOptions must be an object")));
	}
	let Some(plugins) = compiler_options.get("plugins") else {
		return Ok((inherited, inherited_declared));
	};
	let entries: &[Value] = match plugins {
		Value::Array(entries) => entries,
		Value::Null => &[],
		_ => {
			return Err(Error::message(format!(
				"{abs_path}: compilerOptions.plugins must be an array"
			)));
		}
	};
	let plugins = entries
		.iter()
		.filter_map(|entry| {
			let transform = entry.field("transform")?.as_str()?;
			(!transform.is_empty()).then(|| TransformerPluginConfig {
				transform: transform.to_owned(),
				raw: encode(entry),
			})
		})
		.collect();
	Ok((plugins, true))
}

/// `splitTransformerPlugins`: the plugins up to and including the `after` anchor, and the rest.
///
/// # Errors
///
/// `after` names native Flamework itself, no plugin, or more than one.
pub fn split_transformer_plugins(
	plugins: &[TransformerPluginConfig],
	after: &str,
) -> Result<(Vec<TransformerPluginConfig>, Vec<TransformerPluginConfig>), String> {
	if after.is_empty() {
		return Ok((Vec::new(), plugins.to_vec()));
	}
	if after == "rbxts-transformer-flamework" {
		return Err(format!(
			"flamework.after cannot anchor native Flamework to itself: {}",
			gostr::quote(after)
		));
	}
	let matches: Vec<usize> = plugins
		.iter()
		.enumerate()
		.filter(|(_, plugin)| plugin.transform == after)
		.map(|(index, _)| index)
		.collect();
	match matches.as_slice() {
		[] => Err(format!(
			"flamework.after {} does not match an effective tsconfig transformer plugin",
			gostr::quote(after)
		)),
		[anchor] => Ok((plugins[..=*anchor].to_vec(), plugins[anchor + 1..].to_vec())),
		_ => Err(format!(
			"flamework.after {} matches {} effective tsconfig transformer plugins; anchor must be unique",
			gostr::quote(after),
			matches.len()
		)),
	}
}
