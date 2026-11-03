//! `project_config.go`: a parsed project config, the options and root files a program would be
//! built from, without building the program. Output paths depend only on these.

use std::path::Path;

use rotor_support::gopath;
use rotor_tsrs::tsoptions::{ParsedCommandLine, outputpaths};
use rotor_tsrs::{CompilerOptions, P, tspath};

use crate::compile::{Error, Failure};
use crate::project::ProjectOutputHost;

/// `projectConfig`
pub struct ProjectConfig {
	parsed: P<ParsedCommandLine>,
	current_directory: String,
	case_sensitive: bool,
}

/// `parseProjectConfig`: parses `config_path` the way the program loader does.
///
/// # Errors
///
/// The config's diagnostics or rbxtsc's validation message.
pub fn parse_project_config(config_path: &str) -> Result<ProjectConfig, Failure> {
	let absolute = gopath::abs(config_path).map_err(|error| Failure::bare(Error::message(error.to_string())))?;
	let config_path = gopath::to_slash(&absolute).into_owned();
	let parsed =
		rotor_tsrs::project::parse_project_config(Path::new(&config_path), &rotor_tsrs::ProjectOptions::default())
			.map_err(|error| match error {
				rotor_tsrs::LoadError::Config(diagnostics) => {
					Failure::strings(diagnostics, Error::message("compile: tsconfig.json has errors"))
				}
				rotor_tsrs::LoadError::Invalid(message) => Failure::strings(
					vec![message],
					Error::message("compile: invalid tsconfig.json configuration"),
				),
				error => Failure::bare(Error::message(error.to_string())),
			})?;
	Ok(ProjectConfig {
		parsed: P::new(parsed),
		current_directory: tspath::get_directory_path(&config_path),
		case_sensitive: rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names(),
	})
}

impl ProjectConfig {
	/// `FileNames`
	#[must_use]
	pub fn file_names(&self) -> &[String] {
		self.parsed.file_names()
	}

	/// `GetCurrentDirectory`
	#[must_use]
	pub fn get_current_directory(&self) -> &str {
		&self.current_directory
	}

	/// `UseCaseSensitiveFileNames`
	#[must_use]
	pub const fn use_case_sensitive_file_names(&self) -> bool {
		self.case_sensitive
	}
}

impl ProjectOutputHost for ProjectConfig {
	fn options(&self) -> P<CompilerOptions> {
		self.parsed
			.compiler_options()
			.unwrap_or_else(|| P::new(CompilerOptions::default()))
	}

	/// `CommonSourceDirectory`: a config file always fixes it (rootDir, else the config's
	/// directory).
	fn common_source_directory(&self) -> String {
		let options = self.options();
		outputpaths::get_common_source_directory(
			&options,
			|| {
				self.file_names()
					.iter()
					.filter(|name| !tspath::is_declaration_file_name(name))
					.cloned()
					.collect()
			},
			&self.current_directory,
			self.case_sensitive,
			None,
		)
	}
}

impl outputpaths::OutputPathsHost for ProjectConfig {
	fn common_source_directory(&self) -> String {
		ProjectOutputHost::common_source_directory(self)
	}

	fn content_mapper_extensions(&self) -> Vec<String> {
		Vec::new()
	}

	fn get_current_directory(&self) -> &str {
		&self.current_directory
	}

	fn use_case_sensitive_file_names(&self) -> bool {
		self.case_sensitive
	}
}
