//! `incremental_flamework.go`: the native Flamework state that feeds the incremental salt.

use rotor_config::config::FlameworkConfig;
use rotor_support::gopath;

use crate::compile::Error;
use crate::flamework_pipeline::FlameworkPipeline;
use crate::gojson;
use crate::package_layout::hex_sha256;
use crate::project::lock;

/// `FlameworkIncrementalFile`: one normalized file input the native transformer read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlameworkIncrementalFile {
	/// The file.
	pub path: String,
	/// Its contents.
	pub contents: Vec<u8>,
}

/// `FlameworkIncrementalGlob`: one transformer glob and its current matches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlameworkIncrementalGlob {
	/// The pattern.
	pub pattern: String,
	/// The matched paths; `None` is Go's nil slice.
	pub matches: Option<Vec<String>>,
}

/// `FlameworkIncrementalInputs`: the effective native transformer state that contributes to the
/// incremental salt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlameworkIncrementalInputs {
	/// The effective configuration.
	pub effective_config: FlameworkConfig,
	/// The Flamework version the port implements.
	pub transformer_version: String,
	/// The effective transformer plugins, as raw JSON.
	pub effective_plugins: Vec<String>,
	/// The runtime configuration as JSON; `None` without one.
	pub runtime_config: Option<Vec<u8>>,
	/// `package.json` inputs.
	pub package_inputs: Vec<FlameworkIncrementalFile>,
	/// Package `flamework.build` inputs.
	pub build_info_inputs: Vec<FlameworkIncrementalFile>,
	/// The globs Flamework expands.
	pub relevant_globs: Vec<FlameworkIncrementalGlob>,
}

/// `flameworkIncrementalInputs`: `None` without a pipeline.
///
/// # Errors
///
/// Refreshing the glob inputs, collecting the static `addPaths` matches, or reading a package
/// input failed.
pub(crate) fn flamework_incremental_inputs(
	state: Option<&FlameworkPipeline>,
) -> Result<Option<FlameworkIncrementalInputs>, Error> {
	let Some(state) = state else {
		return Ok(None);
	};
	let mut project = lock(&state.project);
	project
		.runtime_artifacts()
		.map_err(|error| Error::message(format!("refresh Flamework glob inputs: {error}")))?;
	let root_build = project.build_info_snapshot();
	let mut inputs = FlameworkIncrementalInputs {
		effective_config: state.config.clone(),
		transformer_version: rotor_flamework::FLAMEWORK_VERSION.to_owned(),
		effective_plugins: crate::flamework_pipeline::raw_transformer_plugins(&state.plugins),
		relevant_globs: incremental_globs(root_build.metadata.as_ref()),
		..FlameworkIncrementalInputs::default()
	};
	let static_matches = {
		let mut checker = state.program.get_type_checker(&rotor_tsrs::Context::default());
		let matches =
			rotor_flamework::transform_addpaths::static_add_paths_matches(state.program, &mut checker, &mut project);
		drop(checker);
		matches.map_err(|error| Error::message(format!("collect static addPaths matches: {error}")))?
	};
	for (pattern, matches) in static_matches {
		inputs.relevant_globs.push(FlameworkIncrementalGlob {
			pattern,
			matches: Some(matches),
		});
	}
	if let Some(config) = root_build
		.metadata
		.as_ref()
		.and_then(|metadata| metadata.config.as_ref())
	{
		inputs.runtime_config = Some(config.marshal_json().into_bytes());
	}
	let package_path = gopath::join(&[project.root_directory(), "package.json"]);
	let package_data = std::fs::read(&package_path).map_err(|error| {
		Error::message(format!(
			"read Flamework package input {}: {}",
			rotor_support::gostr::quote(&package_path),
			rotor_support::gostr::path_error("open", &package_path, &error)
		))
	})?;
	inputs.package_inputs.push(FlameworkIncrementalFile {
		path: package_path,
		contents: package_data,
	});
	for snapshot in project.package_build_info_snapshots() {
		let build_data = std::fs::read(&snapshot.path).map_err(|error| {
			Error::message(format!(
				"read Flamework package build input {}: {}",
				rotor_support::gostr::quote(&snapshot.path),
				rotor_support::gostr::path_error("open", &snapshot.path, &error)
			))
		})?;
		inputs.build_info_inputs.push(FlameworkIncrementalFile {
			path: snapshot.path.clone(),
			contents: build_data,
		});
		let dependency_package_path = gopath::join(&[&gopath::dir(&snapshot.path), "package.json"]);
		match std::fs::read(&dependency_package_path) {
			Ok(data) => inputs.package_inputs.push(FlameworkIncrementalFile {
				path: dependency_package_path,
				contents: data,
			}),
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
			Err(error) => {
				return Err(Error::message(format!(
					"read Flamework dependency package input {}: {}",
					rotor_support::gostr::quote(&dependency_package_path),
					rotor_support::gostr::path_error("open", &dependency_package_path, &error)
				)));
			}
		}
	}
	drop(project);
	Ok(Some(inputs))
}

/// `incrementalGlobs`
fn incremental_globs(metadata: Option<&rotor_flamework::BuildMetadata>) -> Vec<FlameworkIncrementalGlob> {
	let Some(paths) = metadata
		.and_then(|metadata| metadata.globs.as_ref())
		.and_then(|globs| globs.paths.as_ref())
	else {
		return Vec::new();
	};
	paths
		.iter()
		.map(|(pattern, matches)| FlameworkIncrementalGlob {
			pattern: pattern.clone(),
			matches: matches.clone(),
		})
		.collect()
}

/// `flameworkIncrementalSalt`: empty without inputs.
///
/// # Errors
///
/// A plugin entry is not JSON.
pub(crate) fn flamework_incremental_salt(inputs: Option<&FlameworkIncrementalInputs>) -> Result<String, Error> {
	let Some(inputs) = inputs else {
		return Ok(String::new());
	};
	let effective_plugins = normalize_effective_plugins(&inputs.effective_plugins)?;
	let payload = gojson::Object::new()
		.string("version", "rotor-flamework-incremental-v1")
		.raw("effectiveConfig", &flamework_config_json(&inputs.effective_config))
		.string("transformerVersion", &inputs.transformer_version)
		.raw("effectivePlugins", &gojson::array(effective_plugins))
		.string(
			"runtimeConfigHash",
			&incremental_content_hash(inputs.runtime_config.as_deref().unwrap_or_default()),
		)
		.raw(
			"packageInputs",
			&file_states_json(&normalize_flamework_incremental_files(&inputs.package_inputs)),
		)
		.raw(
			"buildInfoInputs",
			&file_states_json(&normalize_flamework_incremental_files(&inputs.build_info_inputs)),
		)
		.raw(
			"relevantGlobs",
			&globs_json(&normalize_flamework_incremental_globs(&inputs.relevant_globs)),
		)
		.finish();
	Ok(incremental_content_hash(payload.as_bytes()))
}

/// `json.Marshal(config.FlameworkConfig)`
fn flamework_config_json(config: &FlameworkConfig) -> String {
	let mut object = gojson::Object::new()
		.string_omitempty("after", &config.after)
		.bool_omitempty("noSemanticDiagnostics", config.no_semantic_diagnostics)
		.bool_omitempty("obfuscation", config.obfuscation)
		.string_omitempty("idGenerationMode", &config.id_generation_mode)
		.string_omitempty("hashPrefix", &config.hash_prefix)
		.string_omitempty("salt", &config.salt)
		.bool_omitempty("preloadIds", config.preload_ids)
		.bool_omitempty("skipUnchangedFiles", config.skip_unchanged_files)
		.raw("optimizations", &optimizations_json(&config.optimizations));
	if !config.profiles.is_empty() {
		let profiles = config
			.profiles
			.iter()
			.map(|(name, profile)| {
				let value = gojson::Object::new()
					.string_omitempty("after", &profile.after)
					.bool_omitempty("noSemanticDiagnostics", profile.no_semantic_diagnostics)
					.bool_omitempty("obfuscation", profile.obfuscation)
					.string_omitempty("idGenerationMode", &profile.id_generation_mode)
					.string_omitempty("hashPrefix", &profile.hash_prefix)
					.string_omitempty("salt", &profile.salt)
					.bool_omitempty("preloadIds", profile.preload_ids)
					.bool_omitempty("skipUnchangedFiles", profile.skip_unchanged_files)
					.raw("optimizations", &optimizations_json(&profile.optimizations))
					.finish();
				format!("{}:{value}", gojson::string(name))
			})
			.collect::<Vec<_>>()
			.join(",");
		object = object.raw("profiles", &format!("{{{profiles}}}"));
	}
	object.finish()
}

/// `json.Marshal(config.FlameworkOptimizations)`: a struct, which `omitempty` never omits.
fn optimizations_json(optimizations: &rotor_config::config::FlameworkOptimizations) -> String {
	let mut object = gojson::Object::new();
	if let Some(limit) = optimizations.guard_generation_dedup_limit {
		object = object.int("guardGenerationDedupLimit", limit);
	}
	object.finish()
}

/// `portableFlameworkIncrementalInputs`: the inputs with every path encoded by `encode_path`.
pub(crate) fn portable_flamework_incremental_inputs(
	inputs: Option<&FlameworkIncrementalInputs>,
	encode_path: &dyn Fn(&str) -> String,
) -> Option<FlameworkIncrementalInputs> {
	let mut result = inputs?.clone();
	for input in &mut result.package_inputs {
		input.path = encode_path(&input.path);
	}
	for input in &mut result.build_info_inputs {
		input.path = encode_path(&input.path);
	}
	for glob in &mut result.relevant_globs {
		glob.pattern = encode_path(&glob.pattern);
		if let Some(matches) = &mut glob.matches {
			for path in matches {
				*path = encode_path(path);
			}
		}
	}
	Some(result)
}

/// `normalizeEffectivePlugins`: each plugin entry re-encoded the way `json.Marshal` writes a
/// decoded `any`: object keys sorted.
fn normalize_effective_plugins(plugins: &[String]) -> Result<Vec<String>, Error> {
	plugins
		.iter()
		.map(|raw| {
			let value: serde_json::Value = serde_json::from_str(raw)
				.map_err(|error| Error::message(format!("normalize effective transformer plugin: {error}")))?;
			Ok(canonical_json(&value))
		})
		.collect()
}

/// `json.Marshal` of a decoded `any`: keys sorted, HTML escaped.
fn canonical_json(value: &serde_json::Value) -> String {
	match value {
		serde_json::Value::Object(map) => {
			let mut keys: Vec<&String> = map.keys().collect();
			keys.sort();
			let members: Vec<String> = keys
				.into_iter()
				.map(|key| format!("{}:{}", gojson::string(key), canonical_json(&map[key])))
				.collect();
			format!("{{{}}}", members.join(","))
		}
		serde_json::Value::Array(values) => {
			format!("[{}]", values.iter().map(canonical_json).collect::<Vec<_>>().join(","))
		}
		serde_json::Value::String(text) => gojson::string(text),
		other => other.to_string(),
	}
}

/// `flameworkIncrementalFileState`
struct FileState {
	path: String,
	hash: String,
}

fn normalize_flamework_incremental_files(inputs: &[FlameworkIncrementalFile]) -> Vec<FileState> {
	let mut states: Vec<FileState> = inputs
		.iter()
		.map(|input| FileState {
			path: normalize_flamework_incremental_path(&input.path),
			hash: incremental_content_hash(&input.contents),
		})
		.collect();
	states.sort_by(|left, right| left.path.cmp(&right.path).then_with(|| left.hash.cmp(&right.hash)));
	states
}

fn file_states_json(states: &[FileState]) -> String {
	gojson::array(states.iter().map(|state| {
		gojson::Object::new()
			.string("path", &state.path)
			.string("hash", &state.hash)
			.finish()
	}))
}

fn normalize_flamework_incremental_globs(globs: &[FlameworkIncrementalGlob]) -> Vec<FlameworkIncrementalGlob> {
	let mut normalized: Vec<FlameworkIncrementalGlob> = globs
		.iter()
		.map(|glob| {
			let mut matches: Vec<String> = glob
				.matches
				.iter()
				.flatten()
				.map(|path| normalize_flamework_incremental_path(path))
				.collect();
			matches.sort();
			matches.dedup();
			FlameworkIncrementalGlob {
				pattern: normalize_flamework_incremental_path(&glob.pattern),
				matches: Some(matches),
			}
		})
		.collect();
	normalized.sort_by(|left, right| left.pattern.cmp(&right.pattern));
	normalized
}

/// `json.Marshal([]FlameworkIncrementalGlob)`: the fields carry no tags, so Go writes their names.
fn globs_json(globs: &[FlameworkIncrementalGlob]) -> String {
	gojson::array(globs.iter().map(|glob| {
		gojson::Object::new()
			.string("Pattern", &glob.pattern)
			.raw(
				"Matches",
				&glob
					.matches
					.as_ref()
					.map_or_else(|| "null".to_owned(), |matches| gojson::string_array(matches)),
			)
			.finish()
	}))
}

/// `normalizeFlameworkIncrementalPath`
fn normalize_flamework_incremental_path(value: &str) -> String {
	rotor_support::gopath::Flavor::Unix.clean(&value.replace('\\', "/"))
}

/// `incrementalContentHash`
pub(crate) fn incremental_content_hash(contents: &[u8]) -> String {
	hex_sha256(contents)
}
