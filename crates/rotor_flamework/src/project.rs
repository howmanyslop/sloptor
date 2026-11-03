//! `project.go`: an open Flamework project.

use rotor_config::config::FlameworkConfig;
use rotor_rojo::PathTranslator;
use rotor_rojo::RojoResolver;
use rotor_support::gopath;
use rotor_tsrs::diagnostics::Category;

use crate::build_info::BuildInfo;
use crate::build_info_clone::clone_runtime_config;
use crate::plan::{AnalysisState, FileAnalysis, FilePlan};
use crate::project_discovery::{load_project_build_info, open_project_root, open_rojo_resolver, resolve_project_path};
use crate::runtime_config::{RuntimeConfig, load_runtime_config};
use crate::state::FLAMEWORK_VERSION;
use crate::transform_diagnostics::Diagnostic;
use crate::{Error, ErrorDetail, ErrorKind};

/// `flameworkDiagnosticCode`
pub const FLAMEWORK_DIAGNOSTIC_CODE: &str = " @flamework/core";

/// `upstreamBoundaryError`: a message upstream Flamework raises as an exception.
pub(crate) fn upstream_boundary_error(message: impl Into<String>) -> Error {
	Error::with_detail(message, ErrorDetail::UpstreamBoundary)
}

/// `IDMode`
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdMode {
	/// `full`
	Full,
	/// `obfuscated`
	Obfuscated,
	/// `short`
	Short,
	/// `tiny`
	Tiny,
	/// Any other text, which is invalid.
	Other(String),
}

impl Default for IdMode {
	fn default() -> Self {
		Self::Other(String::new())
	}
}

impl IdMode {
	/// The mode for its text.
	#[must_use]
	pub fn parse(text: &str) -> Self {
		match text {
			"full" => Self::Full,
			"obfuscated" => Self::Obfuscated,
			"short" => Self::Short,
			"tiny" => Self::Tiny,
			other => Self::Other(other.to_owned()),
		}
	}

	/// The mode's text.
	#[must_use]
	pub fn as_str(&self) -> &str {
		match self {
			Self::Full => "full",
			Self::Obfuscated => "obfuscated",
			Self::Short => "short",
			Self::Tiny => "tiny",
			Self::Other(text) => text,
		}
	}

	const fn valid(&self) -> bool {
		!matches!(self, Self::Other(_))
	}
}

/// `ProjectOptions`
#[derive(Clone, Debug, Default)]
pub struct ProjectOptions {
	/// The project directory.
	pub project_dir: String,
	/// The TypeScript root directory.
	pub root_dir: String,
	/// The TypeScript output directory.
	pub out_dir: String,
	/// The include directory.
	pub include_directory: String,
	/// Skips `include/flamework` artifacts, like rbxts `noInclude` skips `include/`.
	pub no_include: bool,
	/// The Rojo configuration path.
	pub rojo_config_path: String,
	/// Whether declarations are emitted.
	pub declaration: bool,
	/// The Flamework configuration.
	pub config: FlameworkConfig,
}

/// `Project`
#[derive(Clone, Debug)]
pub struct Project {
	pub(crate) directory: String,
	pub(crate) root_directory: String,
	#[expect(dead_code, reason = "Go's Project keeps it; nothing reads it")]
	pub(crate) out_directory: String,
	pub(crate) include_directory: String,
	pub(crate) no_include: bool,
	pub(crate) package_name: String,
	pub(crate) package_version: String,
	pub(crate) hash_prefix: String,
	pub(crate) id_mode: IdMode,
	pub(crate) is_game: bool,
	#[expect(dead_code, reason = "Go's Project keeps it; nothing reads it")]
	pub(crate) pack_mode: bool,
	pub(crate) config: FlameworkConfig,
	pub(crate) translator: PathTranslator,
	pub(crate) resolver: RojoResolver,
	pub(crate) analysis: AnalysisState,
	pub(crate) build_info: BuildInfo,
	#[expect(dead_code, reason = "Go's Project keeps it; nothing reads it")]
	pub(crate) runtime_config: Option<RuntimeConfig>,
}

/// `OpenProject`
///
/// # Errors
///
/// The project cannot be opened; Flamework's own failures carry
/// [`ErrorDetail::UpstreamBoundary`] or [`ErrorDetail::ProjectDiagnostics`].
pub fn open_project(options: &ProjectOptions) -> Result<Project, Error> {
	let project_root = open_project_root(&options.project_dir)?;
	let directory = project_root.directory.clone();

	if options.root_dir.is_empty() {
		return Err(upstream_boundary_error(
			"Assertion Failed! rootDir or rootDirs must be specified\nPlease submit a bug report here:\nhttps://github.com/rbxts-flamework/core/issues",
		));
	}
	let root_directory = resolve_project_path(&directory, &options.root_dir, ".")?;
	let out_directory = resolve_project_path(&directory, &options.out_dir, ".")?;
	let include_directory = resolve_project_path(&directory, &options.include_directory, "include")?;

	let mode = if options.config.id_generation_mode.is_empty() {
		if options.config.obfuscation {
			IdMode::Obfuscated
		} else {
			IdMode::Full
		}
	} else {
		IdMode::parse(&options.config.id_generation_mode)
	};
	if !mode.valid() {
		return Err(Error::kinded(
			ErrorKind::InvalidProject,
			&format!(": ID generation mode {}", rotor_support::gostr::quote(mode.as_str())),
		));
	}

	let is_game = !project_root.package_name.starts_with('@');
	let pack_mode = project_root.package_name == "@flamework/core";
	let mut hash_prefix = options.config.hash_prefix.clone();
	if !is_game && hash_prefix.is_empty() {
		hash_prefix.clone_from(&project_root.package_name);
	}
	if hash_prefix.starts_with('$') && !pack_mode {
		return Err(upstream_boundary_error(
			"The hashPrefix $ is used internally by Flamework",
		));
	}

	let translator = PathTranslator::new(&root_directory, &out_directory, "", options.declaration, false);
	let resolver = open_rojo_resolver(&directory, &out_directory, &options.rojo_config_path)?;
	let mut build_info = load_project_build_info(&directory, &project_root.package_directory)?;
	if !options.config.salt.is_empty() {
		build_info.set_salt(None);
	}
	let previous = build_info.flamework_version().to_owned();
	if previous != FLAMEWORK_VERSION {
		let relative_out = gopath::rel(&directory, &out_directory).unwrap_or_else(|_| out_directory.clone());
		return Err(new_project_diagnostic_error(&[
			"Project was compiled on different version of Flamework.",
			&format!(
				"Please recompile by deleting the {} directory",
				gopath::to_slash(&relative_out)
			),
			&format!("Current Flamework Version: {FLAMEWORK_VERSION}"),
			&format!("Previous Flamework Version: {previous}"),
		]));
	}
	let (runtime_config, present) = match load_runtime_config(&project_root.package_directory) {
		Ok(loaded) => loaded,
		Err(error) => {
			let messages = runtime_config_schema_diagnostics(&error);
			if !messages.is_empty() {
				let messages: Vec<&str> = messages.iter().map(String::as_str).collect();
				return Err(new_project_diagnostic_error(&messages));
			}
			return Err(error);
		}
	};
	build_info.set_runtime_config(None);
	let loaded_runtime_config = present.then_some(runtime_config);
	if let Some(config) = &loaded_runtime_config {
		build_info.set_runtime_config(Some(config));
	}
	if hash_prefix.is_empty() {
		build_info.set_identifier_prefix(None);
	} else {
		build_info.set_identifier_prefix(Some(&hash_prefix));
	}
	Ok(Project {
		directory,
		root_directory: project_root.package_directory,
		out_directory,
		include_directory,
		no_include: options.no_include,
		package_name: project_root.package_name,
		package_version: project_root.package_version,
		hash_prefix,
		id_mode: mode,
		is_game,
		pack_mode,
		config: options.config.clone(),
		translator,
		resolver,
		analysis: AnalysisState::new(),
		build_info,
		runtime_config: clone_runtime_config(loaded_runtime_config.as_ref()),
	})
}

fn runtime_config_schema_diagnostics(error: &Error) -> Vec<String> {
	let message = error.message();
	for field in ["profiling", "disableDependencyWarnings"] {
		if message.contains(&format!("{field} must be boolean")) {
			return vec![
				"Malformed flamework.json".to_owned(),
				format!("type /{field}: must be boolean {{\"type\":\"boolean\"}}"),
			];
		}
	}
	if message.contains("logLevel must be none or verbose") {
		return vec![
			"Malformed flamework.json".to_owned(),
			r#"enum /logLevel: must be equal to one of the allowed values {"allowedValues":["none","verbose"]}"#
				.to_owned(),
		];
	}
	Vec::new()
}

/// `newProjectDiagnosticError`: one undefined-range error diagnostic per message.
pub(crate) fn new_project_diagnostic_error(messages: &[&str]) -> Error {
	let items: Vec<Diagnostic> = messages
		.iter()
		.map(|message| {
			Diagnostic::with_string_code(
				None,
				-1,
				-1,
				FLAMEWORK_DIAGNOSTIC_CODE.trim(),
				Category::Error,
				*message,
			)
		})
		.collect();
	let text = items.iter().map(Diagnostic::string).collect::<Vec<_>>().join("\n");
	Error::with_detail(text, ErrorDetail::ProjectDiagnostics(items))
}

impl Project {
	/// `PackageName`
	#[must_use]
	pub fn package_name(&self) -> &str {
		&self.package_name
	}

	/// `PackageVersion`
	#[must_use]
	pub fn package_version(&self) -> &str {
		&self.package_version
	}

	/// `RootDirectory`
	#[must_use]
	pub fn root_directory(&self) -> &str {
		&self.root_directory
	}

	/// `IncludeDirectory`
	#[must_use]
	pub fn include_directory(&self) -> &str {
		&self.include_directory
	}

	/// `IsGame`
	#[must_use]
	pub const fn is_game(&self) -> bool {
		self.is_game
	}

	/// `HashPrefix`
	#[must_use]
	pub fn hash_prefix(&self) -> &str {
		&self.hash_prefix
	}

	/// `IDMode`
	#[must_use]
	pub const fn id_mode(&self) -> &IdMode {
		&self.id_mode
	}

	/// `PathTranslator`
	#[must_use]
	pub const fn path_translator(&self) -> &PathTranslator {
		&self.translator
	}

	/// `SkipUnchangedFiles`
	#[must_use]
	pub const fn skip_unchanged_files(&self) -> bool {
		self.config.skip_unchanged_files
	}

	/// `RojoResolver`
	#[must_use]
	pub const fn rojo_resolver(&self) -> &RojoResolver {
		&self.resolver
	}

	/// The Flamework configuration.
	#[must_use]
	pub const fn config(&self) -> &FlameworkConfig {
		&self.config
	}

	/// `Analyze`
	///
	/// # Errors
	///
	/// See [`AnalysisState::analyze`].
	pub fn analyze(&mut self, files: &[FileAnalysis]) -> Result<Vec<FilePlan>, Error> {
		self.analysis.analyze(files)
	}

	/// `Plans`
	#[must_use]
	pub fn plans(&self) -> Vec<FilePlan> {
		self.analysis.plans()
	}

	/// The project's analysis state.
	#[must_use]
	pub const fn analysis(&self) -> &AnalysisState {
		&self.analysis
	}
}
