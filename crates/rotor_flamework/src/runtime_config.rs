//! `runtime_config.go`: `flamework.json`, the runtime configuration.

use std::collections::BTreeMap;

use rotor_support::gopath;

use crate::json_syntax::{self, Value, compact, validate_json_syntax};
use crate::{Error, ErrorKind};

/// `RuntimeConfigFileName`
pub const RUNTIME_CONFIG_FILE_NAME: &str = "flamework.json";

/// `RuntimeLogLevel`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeLogLevel {
	/// `none`
	None,
	/// `verbose`
	Verbose,
}

impl RuntimeLogLevel {
	/// The level's text.
	#[must_use]
	pub const fn as_str(self) -> &'static str {
		match self {
			Self::None => "none",
			Self::Verbose => "verbose",
		}
	}
}

/// `RuntimeConfig`: the runtime configuration exposed through `flamework.json`. `None` fields
/// are absent, distinct from explicit zero values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeConfig {
	/// `logLevel`
	pub log_level: Option<RuntimeLogLevel>,
	/// `profiling`
	pub profiling: Option<bool>,
	/// `disableDependencyWarnings`
	pub disable_dependency_warnings: Option<bool>,
	pub(crate) additional: BTreeMap<String, Vec<u8>>,
}

/// `LoadRuntimeConfig`: `<root>/flamework.json`. The flag tells an absent file from a present
/// empty object.
///
/// # Errors
///
/// The file cannot be read or is invalid.
pub fn load_runtime_config(root: &str) -> Result<(RuntimeConfig, bool), Error> {
	let config_path = gopath::join(&[root, RUNTIME_CONFIG_FILE_NAME]);
	let data = match std::fs::read(&config_path) {
		Ok(data) => data,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((RuntimeConfig::default(), false)),
		Err(error) => {
			return Err(
				Error::io("open", &config_path, &error).wrap(&format!("flamework: read {RUNTIME_CONFIG_FILE_NAME}"))
			);
		}
	};
	Ok((parse_runtime_config(&data)?, true))
}

fn invalid(rest: &str) -> Error {
	Error::kinded(ErrorKind::InvalidRuntimeConfig, rest)
}

/// `parseRuntimeConfig`
pub(crate) fn parse_runtime_config(data: &[u8]) -> Result<RuntimeConfig, Error> {
	validate_json_syntax(data).map_err(|error| error.under(ErrorKind::InvalidRuntimeConfig))?;
	let document = json_syntax::parse(data).map_err(|_| invalid(": expected JSON object"))?;
	if !matches!(document, Value::Object(_)) {
		return Err(invalid(": expected JSON object"));
	}
	let mut config = RuntimeConfig::default();
	let mut members = document.map_members();
	// Go ranges over a map; with several invalid fields its error is whichever comes first in
	// map order. Sorting the keys makes it deterministic.
	members.sort_by(|left, right| left.key.cmp(&right.key));
	for member in members {
		match member.key.as_str() {
			"logLevel" => {
				let level = match &member.value {
					Value::String(text) if text == "none" => RuntimeLogLevel::None,
					Value::String(text) if text == "verbose" => RuntimeLogLevel::Verbose,
					_ => return Err(invalid(": logLevel must be none or verbose")),
				};
				config.log_level = Some(level);
			}
			"profiling" => {
				let Value::Bool(value) = member.value else {
					return Err(invalid(": profiling must be boolean"));
				};
				config.profiling = Some(value);
			}
			"disableDependencyWarnings" => {
				let Value::Bool(value) = member.value else {
					return Err(invalid(": disableDependencyWarnings must be boolean"));
				};
				config.disable_dependency_warnings = Some(value);
			}
			_ => {
				config.additional.insert(member.key.clone(), member.raw.clone());
			}
		}
	}
	Ok(config)
}

impl RuntimeConfig {
	/// `MarshalJSON`: additional properties kept, keys sorted, as `json.Marshal` writes a
	/// `map[string]json.RawMessage`.
	#[must_use]
	pub fn marshal_json(&self) -> String {
		let mut properties: BTreeMap<String, Vec<u8>> = self.additional.clone();
		if let Some(level) = self.log_level {
			properties.insert("logLevel".to_owned(), format!("\"{}\"", level.as_str()).into_bytes());
		}
		if let Some(profiling) = self.profiling {
			properties.insert("profiling".to_owned(), profiling.to_string().into_bytes());
		}
		if let Some(disable) = self.disable_dependency_warnings {
			properties.insert("disableDependencyWarnings".to_owned(), disable.to_string().into_bytes());
		}
		let mut out = String::from("{");
		for (index, (name, raw)) in properties.iter().enumerate() {
			if index > 0 {
				out.push(',');
			}
			json_syntax::write_string(&mut out, name, true);
			out.push(':');
			out.push_str(&String::from_utf8_lossy(
				&compact(raw, true).unwrap_or_else(|| raw.clone()),
			));
		}
		out.push('}');
		out
	}
}

/// `MarshalRuntimeConfigArtifact`: deterministic `include/flamework/config.json` bytes, and
/// whether there is anything to write.
#[must_use]
pub fn marshal_runtime_config_artifact(
	game: Option<&RuntimeConfig>,
	packages: &BTreeMap<String, RuntimeConfig>,
) -> Option<Vec<u8>> {
	if game.is_none() && packages.is_empty() {
		return None;
	}
	let mut out = String::from("{");
	if let Some(game) = game {
		out.push_str("\"game\":");
		out.push_str(&game.marshal_json());
		out.push(',');
	}
	out.push_str("\"packages\":{");
	for (index, (name, config)) in packages.iter().enumerate() {
		if index > 0 {
			out.push(',');
		}
		json_syntax::write_string(&mut out, name, true);
		out.push(':');
		out.push_str(&config.marshal_json());
	}
	out.push_str("}}");
	Some(out.into_bytes())
}
