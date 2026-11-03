//! `incremental_salt.go`.
//!
//! The salt that invalidates the whole incremental manifest. It covers the compiler options, the
//! Rojo and include topology, the macro inputs, the native Flamework inputs, and each transformer
//! plugin's identity, none of which show up as a source change.
//!
//! The salt is an opaque hash compared only with salts this compiler wrote; its payload follows
//! Go's fields, but is not byte-identical to Go's (the compiler options are encoded by
//! `rotor_tsrs`), so a manifest written by the Go compiler never matches.

use rotor_rojo::{PartitionInfo, ResolverFilePathState, ResolverState, RojoResolverCache};
use rotor_support::gopath;
use rotor_tsrs::compiler::Program;

use crate::compile::Error;
use crate::flamework_pipeline::FlameworkPipeline;
use crate::gojson;
use crate::incremental_flamework::{flamework_incremental_inputs, flamework_incremental_salt};
use crate::incremental_paths::{incremental_path_encoder, relative_incremental_path};
use crate::incremental_plugin_salt::transformer_plugin_fingerprints;
use crate::luau_types::effective_luau_types;
use crate::macro_inputs::macro_input_values;
use crate::native_types::effective_native_types;
use crate::package_layout::hex_sha256;
use crate::project::{
	ProjectOptions, create_project_reference_path_map, resolve_include_path, resolve_rojo_config_path,
};

/// `outputManifestPath`: where a project without a build info path keeps its manifest.
pub(crate) fn output_manifest_path(project_dir: &str, config_path: &str) -> String {
	let canonical = gopath::abs(config_path).unwrap_or_else(|_| gopath::clean(config_path));
	let sum = hex_sha256(gopath::to_slash(&canonical).as_bytes());
	gopath::join(&[
		project_dir,
		".rotor",
		"cache",
		"output-manifests",
		&format!("{sum}.json"),
	])
}

/// `incrementalSaltWithFlamework`
pub(crate) fn incremental_salt_with_flamework(
	dir: &str,
	program: &'static Program,
	opts: &ProjectOptions,
	path_translator_build_info_path: &str,
	flamework: Option<&FlameworkPipeline>,
) -> Result<String, Error> {
	let encode_path = incremental_path_encoder(dir, Some(program));
	let relative_path = |name: &str| relative_incremental_path(dir, name);
	let options = program.options();
	// State-file directories cannot affect emitted code; anchor their names the same way inside
	// the project and on another drive.
	let mut portable = portable_options(&options, program.get_current_directory(), &relative_path);
	portable.ts_build_info_file = incremental_build_info_identity(&options.ts_build_info_file);
	let options_json =
		rotor_tsrs::tscore::json::marshal(&rotor_tsrs::tsoptions::gojson::compiler_options_to_go_json(&portable))
			.map_err(Error::Message)?;
	let build_info_target = incremental_build_info_identity(path_translator_build_info_path);
	let project_reference_paths = opts.project_reference_salt_paths.as_ref().map_or_else(
		|| create_project_reference_path_map(program, opts, false),
		|paths| (**paths).clone(),
	);
	let portable_references: std::collections::BTreeMap<String, String> = project_reference_paths
		.iter()
		.map(|(source, output)| (encode_path(source), relative_path(output)))
		.collect();
	let (rojo_config_path, _) = resolve_rojo_config_path(dir, &opts.rojo_config_path)?;
	let include_path = resolve_include_path(dir, &opts.include_path)?;
	let flamework_salt = flamework_incremental_salt(flamework_incremental_inputs(flamework).as_ref());
	let rojo_resolver =
		rojo_resolver_fingerprint(opts.rojo_cache.as_deref(), rojo_config_path.as_deref(), &relative_path);
	let package_layout = opts
		.package_layout
		.as_ref()
		.map(|layout| layout.fingerprint(dir))
		.unwrap_or_default();
	let plugins = transformer_plugin_fingerprints(dir, &options.config_file_path)?;
	let macro_inputs = macro_input_values(dir, program);

	let mut payload = gojson::Object::new();
	if !macro_inputs.is_empty() {
		payload = payload.raw("macroInputs", &gojson::string_map(&macro_inputs));
	}
	payload = payload
		.string("version", "rotor-incremental-v5")
		.raw("compilerOptions", &options_json)
		.string("configFilePath", &portable.config_file_path)
		.string("outDir", &portable.out_dir)
		.string("tsBuildInfoFile", &portable.ts_build_info_file)
		.string("pathTranslatorBuildInfoPath", &build_info_target)
		.string(
			"type",
			opts.project_type.map_or("", |project_type| project_type.as_str()),
		)
		.string(
			"rojoConfigPath",
			&relative_path(rojo_config_path.as_deref().unwrap_or("")),
		)
		.string("includePath", &relative_path(&include_path))
		.bool("luaExtension", !opts.lua_extension)
		.bool("declaration", options.declaration.is_true())
		.bool("emitDeclarationOnly", opts.emit_declaration_only)
		.bool("optimizedArrayAppends", opts.optimized_array_appends);
	if effective_native_types(dir, &options.config_file_path, opts.native_types) {
		payload = payload.bool("nativeTypes", true);
	}
	if effective_luau_types(
		dir,
		&options.config_file_path,
		opts.solution_argv.as_ref(),
		opts.luau_types,
	) {
		payload = payload.bool("luauTypes", true);
	}
	payload = payload
		.bool("noOptimizedLoops", opts.no_optimized_loops)
		.string_omitempty("flameworkSalt", &flamework_salt);
	if !plugins.is_empty() {
		payload = payload.raw(
			"transformerPlugins",
			&gojson::array(
				plugins
					.iter()
					.map(super::incremental_plugin_salt::TransformerPluginFingerprint::to_json),
			),
		);
	}
	if !portable_references.is_empty() {
		payload = payload.raw("projectReferencePaths", &gojson::string_map(&portable_references));
	}
	let payload = payload
		.string_omitempty("rojoResolver", &rojo_resolver)
		.string_omitempty("packageLayout", &package_layout)
		.finish();
	Ok(hex_sha256(payload.as_bytes()))
}

/// `portableIncrementalOptions`: path-typed options relative to the project, so a moved
/// checkout keeps its salt. `sourceRoot` and `mapRoot` are URL metadata and stay as authored.
pub(crate) fn portable_options(
	options: &rotor_tsrs::CompilerOptions,
	current_directory: &str,
	encode: &dyn Fn(&str) -> String,
) -> rotor_tsrs::CompilerOptions {
	let mut result = options.clone();
	for value in [
		&mut result.config_file_path,
		&mut result.out_dir,
		&mut result.ts_build_info_file,
		&mut result.root_dir,
		&mut result.base_url,
		&mut result.paths_base_path,
		&mut result.declaration_dir,
		&mut result.out_file,
		&mut result.project,
		&mut result.generate_cpu_profile,
		&mut result.generate_trace,
		&mut result.pprof_dir,
	] {
		*value = encode(value);
	}
	if let Some(root_dirs) = &mut result.root_dirs {
		for name in root_dirs {
			*name = encode(name);
		}
	}
	if let Some(type_roots) = &mut result.type_roots {
		for name in type_roots {
			*name = encode(name);
		}
	}
	if let Some(paths) = &options.paths {
		let base = options.get_paths_base_path(current_directory);
		let encoded: rotor_tsrs::tscore::collections::ordered_map::OrderedMap<String, Option<Vec<String>>> = paths
			.iter()
			.map(|(key, values)| {
				// Combined without cleaning: substitution happens before resolving `..` in a
				// wildcard target.
				let values = values.as_ref().map(|values| {
					values
						.iter()
						.map(|name| encode(&rotor_tsrs::tspath::combine_paths(&base, &[name])))
						.collect()
				});
				(key.clone(), values)
			})
			.collect();
		result.paths = Some(std::sync::Arc::new(encoded));
	}
	result
}

/// `incrementalBuildInfoIdentity`
pub(crate) fn incremental_build_info_identity(name: &str) -> String {
	if name.is_empty() {
		return String::new();
	}
	format!("buildinfo://{}", gopath::base(&gopath::from_slash(name)))
}

/// `rojoResolverFingerprint`: the resolver state that maps files to Roblox paths, without the
/// walked configs and directories. Empty without a Rojo project.
fn rojo_resolver_fingerprint(
	cache: Option<&RojoResolverCache>,
	rojo_config_path: Option<&str>,
	encode_path: &dyn Fn(&str) -> String,
) -> String {
	let (Some(cache), Some(rojo_config_path)) = (cache, rojo_config_path) else {
		return String::new();
	};
	let state = portable_incremental_rojo_state(cache.load(rojo_config_path).state(), encode_path);
	let payload = gojson::Object::new()
		.raw("rbxPath", &rbx_path_json(Some(&state.rbx_path)))
		.raw("partitions", &partitions_json(&state.partitions))
		.raw("filePaths", &file_paths_json(&state.file_paths))
		.raw(
			"isolatedContainers",
			&gojson::array(state.isolated_containers.iter().map(|path| rbx_path_json(Some(path)))),
		)
		.bool("isGame", state.is_game)
		.finish();
	hex_sha256(payload.as_bytes())
}

/// `portableIncrementalRojoState`: paths encoded, exact mappings sorted again.
pub(crate) fn portable_incremental_rojo_state(
	mut state: ResolverState,
	encode_path: &dyn Fn(&str) -> String,
) -> ResolverState {
	for partition in &mut state.partitions {
		partition.fs_path = encode_path(&partition.fs_path);
	}
	for file_path in &mut state.file_paths {
		file_path.path = encode_path(&file_path.path);
	}
	state.file_paths.sort_by(|left, right| left.path.cmp(&right.path));
	state
}

fn rbx_path_json(path: Option<&Vec<String>>) -> String {
	path.map_or_else(|| "null".to_owned(), |path| gojson::string_array(path))
}

/// The partitions as Go marshals `[]rojo.PartitionInfo`.
pub(crate) fn partitions_json(partitions: &[PartitionInfo]) -> String {
	gojson::array(partitions.iter().map(|partition| {
		gojson::Object::new()
			.raw("RbxPath", &rbx_path_json(Some(&partition.rbx_path)))
			.string("FsPath", &partition.fs_path)
			.finish()
	}))
}

/// The exact mappings as Go marshals `[]rojo.ResolverFilePathState`.
pub(crate) fn file_paths_json(file_paths: &[ResolverFilePathState]) -> String {
	gojson::array(file_paths.iter().map(|file_path| {
		gojson::Object::new()
			.string("path", &file_path.path)
			.raw("rbxPath", &rbx_path_json(file_path.rbx_path.as_ref()))
			.finish()
	}))
}
