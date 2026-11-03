//! `jecs_native.go`: rbxts-transformer-jecs run natively ahead of the sidecar.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, OnceLock};

use rotor_support::{gopath, logservice};
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::checker::Checker;
use rotor_tsrs::compiler::Program;
use sha2::{Digest, Sha256};

use crate::compile::Error;
use crate::flamework_program_update::{
	NativeProgramUpdate, NativeSourceOverlay, print_native_source, update_native_flamework_program,
};
use crate::gojson;
use crate::incremental_manifest::normalize_source_file_path;
use crate::jecs_rebase::new_jecs_rebase;
use crate::project::lock;
use crate::sidecar::PreparedTransformerProgram;
use crate::source_trace::DiagnosticTraces;
use crate::timings::log_stage_named;
use crate::transformer_host::TransformerHost;
use crate::transformer_order::TransformerPluginConfig;

/// `nativeJecsTransform`
pub(crate) const NATIVE_JECS_TRANSFORM: &str = "rbxts-transformer-jecs";
/// `nativeJecsEnv`
const NATIVE_JECS_ENV: &str = "ROTOR_NATIVE_JECS";

/// `nativeJecsSourceHashes`: the installed transformers `rotor_jecs` is a port of, hashed by
/// [`hash_jecs_transformer_sources`]. Any other build of the package, including a re-patched one,
/// keeps running in the sidecar.
fn native_jecs_source_hashes() -> &'static Mutex<HashSet<String>> {
	static HASHES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
	HASHES.get_or_init(|| {
		Mutex::new(HashSet::from([
			// rbxts-transformer-jecs 0.0.11 with patches/rbxts-transformer-jecs@0.0.11.patch
			"79f5ae9346aaa20019f1147dcd103ec16fc6df3a1a622f774780efbf4293a3fd".to_owned(),
		]))
	})
}

/// Registers a hash as a ported build (a test's `nativeJecsSourceHashes[hash] = true`).
#[cfg(test)]
pub(crate) fn register_native_jecs_source_hash(hash: &str) {
	lock(native_jecs_source_hashes()).insert(hash.to_owned());
}

/// `jecsSourceHashCache`: package directory to hash, or empty.
pub(crate) fn jecs_source_hash_cache() -> &'static Mutex<HashMap<String, String>> {
	static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
	CACHE.get_or_init(Mutex::default)
}

/// `nativeJecsStage`: a transformer stage split into a leading native jecs pass and the plugins
/// left for the sidecar. It declines (`None`) whenever running jecs first could change what the
/// remaining plugins see: jecs must be the stage's first `before` transformer, configured only
/// with options the port understands, and installed as a build the port matches.
pub(crate) fn native_jecs_stage(
	dir: &str,
	plugins: &[TransformerPluginConfig],
) -> (Option<rotor_jecs::Config>, Vec<TransformerPluginConfig>) {
	if crate::sidecar::getenv(NATIVE_JECS_ENV).as_deref() == Some("0") {
		return (None, plugins.to_vec());
	}
	for (index, plugin) in plugins.iter().enumerate() {
		let Ok(parsed) = gojson::parse(plugin.marshal_json().as_bytes()) else {
			return (None, plugins.to_vec());
		};
		let Some(members) = parsed.as_object() else {
			return (None, plugins.to_vec());
		};
		// Go decodes into a map: a repeated key keeps its last value.
		let mut fields: Vec<(&str, &gojson::Value)> = Vec::new();
		for (key, value) in members {
			fields.retain(|(existing, _)| existing != key);
			fields.push((key, value));
		}
		let field = |name: &str| fields.iter().find(|(key, _)| *key == name).map(|(_, value)| *value);
		if is_truthy_json(field("after")) || is_truthy_json(field("afterDeclarations")) {
			continue;
		}
		if plugin.transform != NATIVE_JECS_TRANSFORM {
			return (None, plugins.to_vec());
		}
		let mut config = rotor_jecs::Config::default();
		for &(key, value) in &fields {
			match key {
				"transform" | "after" | "afterDeclarations" => {}
				"silent" => match value {
					gojson::Value::Bool(silent) => config.silent = *silent,
					gojson::Value::Null => {}
					_ => return (None, plugins.to_vec()),
				},
				"jecsPackage" => match value {
					gojson::Value::String(package) => package.clone_into(&mut config.jecs_package),
					gojson::Value::Null => {}
					_ => return (None, plugins.to_vec()),
				},
				_ => return (None, plugins.to_vec()),
			}
		}
		if !lock(native_jecs_source_hashes()).contains(&installed_jecs_source_hash(dir)) {
			return (None, plugins.to_vec());
		}
		let mut rest = plugins.to_vec();
		rest.remove(index);
		return (Some(config), rest);
	}
	(None, plugins.to_vec())
}

/// `usesSubprocessSidecar`: whether `host` ends in the CLI's Node worker. API callback hosts run
/// plugins in the caller's own process, which keeps every plugin there.
pub(crate) fn uses_subprocess_sidecar(host: &dyn TransformerHost) -> bool {
	host.uses_subprocess_sidecar()
}

/// `isTruthyJSON`: Go compares the raw text; the parse keeps a number's text.
fn is_truthy_json(raw: Option<&gojson::Value>) -> bool {
	match raw {
		None | Some(gojson::Value::Null | gojson::Value::Bool(false)) => false,
		Some(gojson::Value::Number(text)) => text != "0",
		Some(gojson::Value::String(text)) => !text.is_empty(),
		Some(_) => true,
	}
}

/// `installedJecsSourceHash`: the hash of the transformer the sidecar would load for a project in
/// `dir`, found the way Node resolves it: the nearest `node_modules`.
fn installed_jecs_source_hash(dir: &str) -> String {
	let mut current = gopath::clean(&gopath::from_slash(dir));
	loop {
		let package_dir = gopath::join(&[&current, "node_modules", NATIVE_JECS_TRANSFORM]);
		if std::fs::metadata(&package_dir).is_ok_and(|metadata| metadata.is_dir()) {
			if let Some(cached) = lock(jecs_source_hash_cache()).get(&package_dir) {
				return cached.clone();
			}
			let hash = hash_jecs_transformer_sources(&gopath::join(&[&package_dir, "out", "src"])).unwrap_or_default();
			lock(jecs_source_hash_cache()).insert(package_dir, hash.clone());
			return hash;
		}
		let parent = gopath::dir(&current);
		if parent == current {
			return String::new();
		}
		current = parent;
	}
}

/// `hashJecsTransformerSources`: sha256 over every `.js` file under `root` in path order, each
/// framed as `<relative path>\0<contents>\0`.
pub(crate) fn hash_jecs_transformer_sources(root: &str) -> std::io::Result<String> {
	fn walk(root: &str, directory: &str, files: &mut Vec<String>) -> std::io::Result<()> {
		let mut entries: Vec<_> = std::fs::read_dir(directory)?.collect::<Result<_, _>>()?;
		entries.sort_by_key(std::fs::DirEntry::file_name);
		for entry in entries {
			let path = gopath::join(&[directory, &entry.file_name().to_string_lossy()]);
			if entry.file_type()?.is_dir() {
				walk(root, &path, files)?;
			} else if has_js_extension(&path) {
				let relative = gopath::rel(root, &path).map_err(|error| std::io::Error::other(error.to_string()))?;
				files.push(gopath::to_slash(&relative).into_owned());
			}
		}
		Ok(())
	}
	std::fs::metadata(root)?;
	let mut files = Vec::new();
	walk(root, root, &mut files)?;
	files.sort();
	let mut hash = Sha256::new();
	for file in &files {
		let contents = std::fs::read(gopath::join(&[root, &gopath::from_slash(file)]))?;
		hash.update(file.as_bytes());
		hash.update([0]);
		hash.update(&contents);
		hash.update([0]);
	}
	let mut hex = String::with_capacity(64);
	for byte in hash.finalize() {
		let _ = write!(hex, "{byte:02x}");
	}
	Ok(hex)
}

/// Go's `strings.HasSuffix(path, ".js")`.
#[expect(
	clippy::case_sensitive_file_extension_comparisons,
	reason = "Go's HasSuffix is case-sensitive"
)]
fn has_js_extension(path: &str) -> bool {
	path.ends_with(".js")
}

/// One checker's share: the candidates' indices in `source_files`.
fn transform_group(
	program: &'static Program,
	checker: &mut Checker,
	source_files: &[P<SourceFile>],
	group: &[usize],
	config: &rotor_jecs::Config,
	overlays: &Mutex<Vec<Option<NativeSourceOverlay>>>,
) -> Result<(), Error> {
	let files: Vec<P<SourceFile>> = group.iter().map(|&index| source_files[index]).collect();
	let warn = |message: &str| logservice::warn(message);
	let result = rotor_jecs::transform(rotor_jecs::Input {
		program,
		checker,
		files: files.clone(),
		config: config.clone(),
		warn: Some(&warn),
	})
	.map_err(|error| Error::message(format!("compile: native jecs transform: {error}")))?;
	for (position, &changed) in result.changed.iter().enumerate() {
		if !changed {
			continue;
		}
		let original = files[position];
		let (text, trace) = print_native_source(
			result.files[position],
			original,
			result.emit_context,
			original.file_name(),
			original.text(),
		)?;
		let rebase = new_jecs_rebase(original, result.files[position], &text).map(Arc::new);
		lock(overlays)[group[position]] = Some(NativeSourceOverlay {
			file_name: original.file_name().to_owned(),
			text,
			trace: Some(Arc::new(trace)),
			rebase,
		});
	}
	Ok(())
}

/// `applyNativeJecsTransform`: jecs over `source_files`, the files it changed overlaid back into
/// the program, the way the native Flamework stage does. The files that pass the transformer's
/// text prefilter are split across the program's checkers: each file's transform reads only its
/// own syntax and the checker's answers, so every checker can take a share with its own state.
pub(crate) fn apply_native_jecs_transform(
	program: &'static Program,
	source_files: &[P<SourceFile>],
	traces: &DiagnosticTraces,
	config: &rotor_jecs::Config,
) -> Result<PreparedTransformerProgram, Error> {
	let config_path = program.options().config_file_path.clone();
	let stop = log_stage_named(&config_path, "native jecs transform");
	let result = apply_native_jecs_transform_inner(program, source_files, traces, config);
	stop();
	result
}

fn apply_native_jecs_transform_inner(
	program: &'static Program,
	source_files: &[P<SourceFile>],
	traces: &DiagnosticTraces,
	config: &rotor_jecs::Config,
) -> Result<PreparedTransformerProgram, Error> {
	let candidates: Vec<usize> = source_files
		.iter()
		.enumerate()
		.filter(|(_, file)| !file.is_declaration_file() && rotor_jecs::needs_transform(file.text()))
		.map(|(index, _)| index)
		.collect();
	let overlays: Mutex<Vec<Option<NativeSourceOverlay>>> = Mutex::new(vec![None; source_files.len()]);
	let group_count = if candidates.len() > 1 {
		program.checker_pool_size()
	} else {
		0
	};
	let mut errors: Vec<Option<Error>> = Vec::new();
	if group_count > 1 {
		let mut groups: Vec<Vec<usize>> = vec![Vec::new(); group_count];
		for (position, &index) in candidates.iter().enumerate() {
			groups[position % group_count].push(index);
		}
		let group_errors: Mutex<Vec<Option<Error>>> = Mutex::new((0..group_count).map(|_| None).collect());
		program.for_each_checker_parallel(|index, checker| {
			let Some(group) = groups.get(index) else {
				return;
			};
			if group.is_empty() {
				return;
			}
			let result = transform_group(program, checker, source_files, group, config, &overlays);
			if let Err(error) = result {
				lock(&group_errors)[index] = Some(error);
			}
		});
		errors = group_errors
			.into_inner()
			.unwrap_or_else(std::sync::PoisonError::into_inner);
	} else if !candidates.is_empty() {
		let mut checker = program.get_type_checker(&rotor_tsrs::Context::default());
		errors.push(transform_group(program, &mut checker, source_files, &candidates, config, &overlays).err());
		drop(checker);
	}
	if let Some(error) = errors.into_iter().flatten().next() {
		return Err(error);
	}

	let mut changed = Vec::new();
	let mut rebases = HashMap::new();
	for overlay in overlays
		.into_inner()
		.unwrap_or_else(std::sync::PoisonError::into_inner)
		.into_iter()
		.flatten()
	{
		if let Some(rebase) = &overlay.rebase {
			rebases.insert(normalize_source_file_path(&overlay.file_name), Arc::clone(rebase));
		}
		changed.push(overlay);
	}
	let (transformed_program, remapped, composed) = update_native_flamework_program(NativeProgramUpdate {
		program,
		source_files,
		traces,
		overlays: changed,
	})?;
	let mut prepared = PreparedTransformerProgram::plain(transformed_program, remapped, None);
	prepared.source_traces = composed;
	prepared.rebases = rebases;
	Ok(prepared)
}
