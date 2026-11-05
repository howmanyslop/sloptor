//! `incremental_plugin_salt.go`: each effective transformer plugin's identity (its config, its
//! package version, and its entry file's hash), so upgrading a transformer invalidates the
//! incremental manifest.

use rotor_support::gopath;

use crate::compile::Error;
use crate::gojson::{self, Value};
use crate::package_layout::hex_sha256;
use crate::packageexports::{read_package_manifest, resolve_package_exports};
use crate::transformer_order::{TransformerPluginConfig, effective_transformer_plugins};

/// `transformerPluginFingerprint`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TransformerPluginFingerprint {
	pub(crate) transform: String,
	pub(crate) config: String,
	pub(crate) version: String,
	pub(crate) entry_hash: String,
	pub(crate) unresolved: String,
}

impl TransformerPluginFingerprint {
	/// Go's `json.Marshal`.
	pub(crate) fn to_json(&self) -> String {
		let mut object = gojson::Object::new().string("transform", &self.transform);
		if !self.config.is_empty() {
			object = object.raw("config", &self.config);
		}
		object
			.string_omitempty("version", &self.version)
			.string_omitempty("entryHash", &self.entry_hash)
			.string_omitempty("unresolved", &self.unresolved)
			.finish()
	}
}

/// `transformerPluginFingerprints`
pub(crate) fn transformer_plugin_fingerprints(
	project_dir: &str,
	config_path: &str,
) -> Result<Vec<TransformerPluginFingerprint>, Error> {
	if config_path.is_empty() {
		return Ok(Vec::new());
	}
	let plugins = effective_transformer_plugins(config_path)?;
	Ok(plugins
		.iter()
		.map(|plugin| fingerprint_transformer_plugin(project_dir, plugin))
		.collect())
}

/// `fingerprintTransformerPlugin`
fn fingerprint_transformer_plugin(project_dir: &str, plugin: &TransformerPluginConfig) -> TransformerPluginFingerprint {
	let mut fingerprint = TransformerPluginFingerprint {
		transform: plugin.transform.clone(),
		config: plugin.marshal_json(),
		..TransformerPluginFingerprint::default()
	};
	match resolve_transformer_plugin_entry(project_dir, &plugin.transform) {
		Err(reason) => reason.clone_into(&mut fingerprint.unresolved),
		Ok((entry_path, version)) => {
			fingerprint.version = version;
			match std::fs::read(&entry_path) {
				Ok(contents) => fingerprint.entry_hash = hex_sha256(&contents),
				Err(_) => "entry unreadable".clone_into(&mut fingerprint.unresolved),
			}
		}
	}
	fingerprint
}

/// `resolveTransformerPluginEntry`: the plugin's entry file and package version, or why it does
/// not resolve.
pub(crate) fn resolve_transformer_plugin_entry(
	project_dir: &str,
	transform: &str,
) -> Result<(String, String), &'static str> {
	let project_dir = gopath::clean(&gopath::from_slash(project_dir));
	if transform.is_empty() {
		return Err("empty transform");
	}
	if transform.starts_with('.') || gopath::is_abs(&gopath::from_slash(transform)) {
		let mut candidate = gopath::from_slash(transform).into_owned();
		if !gopath::is_abs(&candidate) {
			candidate = gopath::join(&[&project_dir, &candidate]);
		}
		return resolve_module_file(&candidate)
			.map(|resolved| (resolved, String::new()))
			.ok_or("entry not found");
	}

	let (package_name, subpath) = split_package_specifier(transform);
	let package_dir = find_package_directory(&project_dir, &package_name).ok_or("package not installed")?;
	let manifest = read_package_manifest(&package_dir).map_err(|_| "package.json unreadable")?;
	let candidates = if subpath.is_empty() {
		let exports_runtime_path = resolve_package_exports(&package_dir, &[])
			.map(|(_, runtime_path)| runtime_path)
			.unwrap_or_default();
		vec![
			exports_runtime_path,
			package_exports_runtime_entry(manifest.exports.as_ref()),
			manifest.main.clone(),
			"index.js".to_owned(),
		]
	} else {
		vec![subpath]
	};
	for entry in candidates {
		if entry.is_empty() {
			continue;
		}
		let mut candidate = gopath::from_slash(&entry).into_owned();
		if !gopath::is_abs(&candidate) {
			candidate = gopath::join(&[&package_dir, &candidate]);
		}
		if let Some(resolved) = resolve_module_file(&candidate) {
			return Ok((resolved, manifest.version));
		}
	}
	Err("entry not found")
}

/// `packageExportsRuntimeEntry`: the `.` export's string, or its first runtime condition.
fn package_exports_runtime_entry(exports: Option<&Value>) -> String {
	match exports {
		Some(Value::String(text)) => text.clone(),
		Some(table @ Value::Object(_)) => match table.get(".") {
			Some(Value::String(text)) => text.clone(),
			target => package_exports_condition(target.unwrap_or(table)),
		},
		_ => String::new(),
	}
}

/// `packageExportsCondition`
fn package_exports_condition(target: &Value) -> String {
	if !matches!(target, Value::Object(_)) {
		return String::new();
	}
	for condition in ["require", "node", "default", "import"] {
		let Some(raw) = target.get(condition) else {
			continue;
		};
		if let Value::String(text) = raw {
			return text.clone();
		}
		let nested = package_exports_condition(raw);
		if !nested.is_empty() {
			return nested;
		}
	}
	String::new()
}

/// `splitPackageSpecifier`
pub(crate) fn split_package_specifier(specifier: &str) -> (String, String) {
	let parts: Vec<&str> = specifier.split('/').collect();
	let take = if specifier.starts_with('@') && parts.len() > 1 {
		2
	} else {
		1
	};
	if parts.len() <= take {
		return (specifier.to_owned(), String::new());
	}
	(parts[..take].join("/"), parts[take..].join("/"))
}

/// `findPackageDirectory`: `node_modules/<name>` with a `package.json`, walking up.
pub(crate) fn find_package_directory(start_dir: &str, package_name: &str) -> Option<String> {
	let mut directory = start_dir.to_owned();
	loop {
		let candidate = gopath::join(&[&directory, "node_modules", &gopath::from_slash(package_name)]);
		if std::fs::metadata(gopath::join(&[&candidate, "package.json"])).is_ok_and(|info| !info.is_dir()) {
			return Some(candidate);
		}
		let parent = gopath::dir(&directory);
		if parent == directory {
			return None;
		}
		directory = parent;
	}
}

/// `resolveModuleFile`: the file, then with a JavaScript extension, then a directory index.
pub(crate) fn resolve_module_file(candidate: &str) -> Option<String> {
	let is_file = |path: &str| std::fs::metadata(path).is_ok_and(|info| !info.is_dir());
	if is_file(candidate) {
		return Some(candidate.to_owned());
	}
	for extension in [".js", ".cjs", ".mjs", ".ts"] {
		let with_extension = format!("{candidate}{extension}");
		if is_file(&with_extension) {
			return Some(with_extension);
		}
	}
	for index in ["index.js", "index.cjs", "index.mjs"] {
		let in_directory = gopath::join(&[candidate, index]);
		if is_file(&in_directory) {
			return Some(in_directory);
		}
	}
	None
}
