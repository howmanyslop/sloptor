//! `sidecar.go`.
//!
//! The Node transformer sidecar is Stage 5b. Stage 5a ports the helpers the single-project pipeline
//! calls: the "does this project run transformer plugins" gate, source file remapping, and path
//! normalization. The program construction half of `sidecar.go` (`parseProjectCommandLine`,
//! `newProjectProgramFromFSWithOptions`) is `rotor_tsrs::project::load_project`.

use std::collections::{HashMap, HashSet};

use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::Program;
use rotor_tsrs::tsoptions::ParsedCommandLine;

use crate::compile::{Error, Failure};
use crate::incremental_manifest::normalize_source_file_path;
use crate::project::project_source_files;
use crate::source_trace::DiagnosticTraces;

/// `preparedTransformerProgram`: the program the transform runs on, its source files, and the
/// traces back to the original text.
#[derive(Clone)]
pub struct PreparedTransformerProgram {
	/// The program to transform: the original one without transformer plugins.
	pub program: &'static Program,
	/// The selected files, as files of `program`.
	pub source_files: Vec<P<SourceFile>>,
	/// The active Flamework configuration, when native Flamework ran.
	pub flamework: Option<rotor_config::config::FlameworkConfig>,
	/// Traces from reprinted files back to the text on disk.
	pub source_traces: DiagnosticTraces,
}

/// The error a Stage 5b or Stage 6 stub returns.
pub(crate) fn not_yet_ported(what: &str) -> Failure {
	let message = format!("compile: {what} is not yet ported to rotor-rs");
	Failure::strings(vec![message.clone()], Error::message(message))
}

/// `prepareTransformerProgramWithHost` without the sidecar: projects with transformer plugins
/// need the Node sidecar (Stage 5b) and fail with a not-yet-ported diagnostic.
pub(crate) fn prepare_transformer_program(
	dir: &str,
	program: &'static Program,
	source_files: &[P<SourceFile>],
) -> Result<PreparedTransformerProgram, Failure> {
	let flamework = crate::flamework_config::prepare_flamework_config(dir, &program.command_line())?;
	if source_files.is_empty() || !project_uses_transformer_plugins(Some(&program.command_line())) {
		return Ok(PreparedTransformerProgram {
			program,
			source_files: source_files.to_vec(),
			flamework,
			source_traces: DiagnosticTraces::default(),
		});
	}
	Err(not_yet_ported("the transformer plugin sidecar"))
}

/// `remapProgramSourceFiles`: the same files, by path, in a derived program.
///
/// # Errors
///
/// A file is missing from the derived program.
pub fn remap_program_source_files(
	program: &Program,
	source_files: &[P<SourceFile>],
) -> Result<Vec<P<SourceFile>>, Error> {
	let by_path: HashMap<String, P<SourceFile>> = project_source_files(program)
		.into_iter()
		.map(|file| (normalize_source_file_path(file.file_name()), file))
		.collect();
	source_files
		.iter()
		.map(|file| {
			let path = normalize_source_file_path(file.file_name());
			by_path.get(&path).copied().ok_or_else(|| {
				Error::message(format!(
					"compile: transformed source file missing from overlay program: {path}"
				))
			})
		})
		.collect()
}

/// `projectUsesTransformerPlugins`: whether this project has a transformer to run. The project's
/// own config answers when it declares `plugins`; otherwise any config of its extends chain that
/// declares a transform does.
pub(crate) fn project_uses_transformer_plugins(parsed: Option<&ParsedCommandLine>) -> bool {
	let Some(parsed) = parsed else {
		return false;
	};
	let config_name = parsed.config_name();
	if !config_name.is_empty()
		&& let (declares_transform, true) = config_file_plugins_declaration(&normalize_source_file_path(config_name))
	{
		return declares_transform;
	}
	let mut seen = HashSet::new();
	for config_path in parsed.extended_source_files() {
		if config_path.is_empty() {
			continue;
		}
		let path = normalize_source_file_path(&config_path);
		if !seen.insert(path.clone()) {
			continue;
		}
		if config_file_plugins_declaration(&path).0 {
			return true;
		}
	}
	false
}

/// `configFilePluginsDeclaration`: whether the config's own `plugins` names a transform, and
/// whether it declares the key at all.
pub(crate) fn config_file_plugins_declaration(config_path: &str) -> (bool, bool) {
	config_file_transformer_plugins(config_path).map_or((false, false), |plugins| (!plugins.is_empty(), true))
}

/// `configFileTransformerPlugins`: the `transform` names of the config's own
/// `compilerOptions.plugins`; `None` when it does not declare an array there.
pub(crate) fn config_file_transformer_plugins(config_path: &str) -> Option<Vec<String>> {
	let data = std::fs::read(config_path).ok()?;
	let text = rotor_tsrs::tsconfig::strip_jsonc(&String::from_utf8_lossy(&data));
	let root: serde_json::Value = serde_json::from_str(&text).ok()?;
	let plugins = root.get("compilerOptions")?.as_object()?.get("plugins")?.as_array()?;
	Some(
		plugins
			.iter()
			.filter_map(|plugin| plugin.get("transform")?.as_str())
			.filter(|transform| !transform.is_empty())
			.map(str::to_owned)
			.collect(),
	)
}

/// `normalizeOverlayPath`
pub(crate) fn normalize_overlay_path(path: &str, case_sensitive: bool) -> String {
	let path = normalize_source_file_path(path);
	if case_sensitive { path } else { path.to_lowercase() }
}

/// `osvfs.FS().Realpath`: the physical spelling of an absolute path, the path itself for a
/// relative one (the file system asserts a rooted argument).
pub(crate) fn realpath_or(path: &str) -> String {
	let slashed = gopath::to_slash(path);
	if rotor_tsrs::tspath::get_root_length(&slashed) == 0 {
		return path.to_owned();
	}
	gopath::from_slash(&rotor_tsrs::vfs::osvfs::fs().realpath(&slashed)).into_owned()
}
