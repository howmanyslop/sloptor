//! Program loading the way `internal/compile` builds rotor's program, plus derived programs and
//! per-file checker access.
//!
//! [`load_project`] is the port of `newProjectProgramWithOptions` for a build without overlays:
//! the sanitized file system ([`crate::tsconfig`]), the synthetic `$env` and macro declarations,
//! `ApplyAutomaticTypes`, the checker and single-threaded overrides, and rbxtsc's option
//! validation. [`update_with_overlays`] derives a program with replaced file texts through tsrs's
//! `update_program`, and [`for_each_checker_group`] hands each file the checker that checked it.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path as StdPath;
use std::sync::Arc;

use rotor_support::gopath;
use tsrs_ast::SourceFile;
use tsrs_checker::Checker;
use tsrs_compiler::{CompilerHost, Program, ProgramOptions, new_cached_fs_compiler_host, new_program};
use tsrs_core::{CompilerOptions, P, Tristate, tspath};
use tsrs_tsoptions::{ParseConfigHost, ParsedCommandLine, get_parsed_command_line_of_config_file};
use tsrs_vfs::{FS, bundled, osvfs, wrapvfs};

use crate::checker::TypeChecker;
use crate::tsconfig;

/// File name of the synthetic `$env` declaration (`envdecl.go`).
pub const ENV_DECL_FILE_NAME: &str = "__rotor_env.d.ts";
/// File name of the synthetic `$nameof` / `$keys` / `$file` declaration (`macrodecl.go`).
pub const MACRO_DECL_FILE_NAME: &str = "__rotor_macros.d.ts";
/// The legacy on-disk `$env` companion that suppresses the synthetic declaration.
pub const ENV_DECL_COMPANION: &str = "rotor-env.d.ts";
/// The legacy on-disk macro companion that suppresses the synthetic declaration.
pub const MACRO_DECL_COMPANION: &str = "rotor-macros.d.ts";
/// The consolidated on-disk companion that suppresses both synthetic declarations.
pub const ROTOR_TYPES_FILE_NAME: &str = "rotor.d.ts";

/// The one `$env` ambient declaration (`envDeclBody`).
pub const ENV_DECL_BODY: &str = "declare const $env: {
	readonly [envName: string]: string | undefined;
} & ((name: string, fallback: string) => string) &
	((name: string) => string | undefined);
";

/// The one `$nameof` / `$keys` / `$file` ambient declaration (`macroDeclBody`).
pub const MACRO_DECL_BODY: &str = "declare function $nameof(item: unknown): string;
declare function $keys<T>(): string[];
declare function $file(path: string): any;
";

const ENV_DECL_HEADER: &str = "// rotor compiler extension (synthetic, in-memory): the $env compile-time
// environment macro. This file is injected by the rotor compiler and does
// not exist on disk.
";

const MACRO_DECL_HEADER: &str = "// rotor compiler extension (synthetic, in-memory): $nameof, $keys, and $file.
// This file is injected by the rotor
// compiler and does not exist on disk.
";

/// Why a project could not be loaded.
#[derive(Debug)]
pub enum LoadError {
	/// The process working directory could not be read.
	CurrentDirectory(std::io::Error),
	/// The tsconfig path is not valid UTF-8.
	NonUtf8Path,
	/// tsrs reported diagnostics while parsing the tsconfig, or could not parse it at all.
	Config(Vec<String>),
	/// The tsconfig parsed but breaks one of rbxtsc's required settings; the full message.
	Invalid(String),
}

impl fmt::Display for LoadError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::CurrentDirectory(error) => write!(formatter, "cannot read the current directory: {error}"),
			Self::NonUtf8Path => formatter.write_str("the tsconfig path is not valid UTF-8"),
			Self::Config(diagnostics) if diagnostics.is_empty() => {
				formatter.write_str("compile: tsconfig.json has errors")
			}
			Self::Config(diagnostics) => {
				write!(
					formatter,
					"compile: tsconfig.json has errors: {}",
					diagnostics.join("; ")
				)
			}
			Self::Invalid(message) => formatter.write_str(message),
		}
	}
}

impl std::error::Error for LoadError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::CurrentDirectory(error) => Some(error),
			Self::NonUtf8Path | Self::Config(_) | Self::Invalid(_) => None,
		}
	}
}

/// The CLI knobs that change how the program is built (the program half of `ProjectOptions`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectOptions {
	/// Overrides the checker pool size (`--checkers`, `ApplyCheckerOverride`).
	pub checkers: Option<i64>,
	/// Overrides `singleThreaded` (`ApplySingleThreadedOverride`).
	pub single_threaded: Option<bool>,
	/// Skips rbxtsc's option validation, for tools that load configs rotor would refuse.
	pub skip_validation: bool,
}

/// A loaded project: its bound program and where it came from.
#[derive(Clone, Copy)]
pub struct Project {
	/// The program. tsrs hands out `&'static` programs; rotor is a one-shot compiler.
	pub program: &'static Program,
}

impl Project {
	/// The project directory: the tsconfig's directory, absolute and slash-separated.
	#[must_use]
	pub fn directory(&self) -> &str {
		self.program.get_current_directory()
	}

	/// The source files rotor compiles ([`project_source_files`]).
	#[must_use]
	pub fn source_files(&self) -> Vec<P<SourceFile>> {
		project_source_files(self.program)
	}
}

struct ConfigHost {
	fs: Arc<dyn FS>,
	cwd: String,
}

impl ParseConfigHost for ConfigHost {
	fn fs(&self) -> &dyn FS {
		&*self.fs
	}

	fn get_current_directory(&self) -> &str {
		&self.cwd
	}
}

fn path_matches(case_sensitive: bool, left: &str, right: &str) -> bool {
	if case_sensitive {
		left == right
	} else {
		left.eq_ignore_ascii_case(right)
	}
}

/// The base file system of a project: bundled libs over the OS, every config of the extends chain
/// sanitized, compiler-types' iteration interfaces rewritten, and the two synthetic declarations
/// served from memory (`solutionCacheFS`, `injectEnvDeclFS`, `injectMacroDeclFS`).
fn project_fs(config_path: &str) -> Arc<dyn FS> {
	let bundled_fs: &'static dyn FS = Box::leak(Box::new(bundled::wrap_fs(osvfs::fs())));
	let case_sensitive = bundled_fs.use_case_sensitive_file_names();
	let directory = tspath::get_directory_path(config_path);
	let env_decl = format!("{directory}/{ENV_DECL_FILE_NAME}");
	let macro_decl = format!("{directory}/{MACRO_DECL_FILE_NAME}");
	let chain: Vec<String> = tsconfig::extends_chain(&gopath::from_slash(config_path))
		.iter()
		.map(|path| gopath::to_slash(path).into_owned())
		.collect();
	let config_path = config_path.to_owned();

	let synthetic = move |path: &str| -> Option<String> {
		if path_matches(case_sensitive, path, &env_decl) {
			return Some(format!("{ENV_DECL_HEADER}{ENV_DECL_BODY}"));
		}
		if path_matches(case_sensitive, path, &macro_decl) {
			return Some(format!("{MACRO_DECL_HEADER}{MACRO_DECL_BODY}"));
		}
		None
	};
	let synthetic = Arc::new(synthetic);
	let exists = Arc::clone(&synthetic);
	Arc::new(wrapvfs::wrap(
		bundled_fs,
		wrapvfs::Replacements {
			file_exists: Some(Box::new(move |path: &str| {
				exists(path).is_some() || bundled_fs.file_exists(path)
			})),
			read_file: Some(Box::new(move |raw_path: &str| {
				if let Some(text) = synthetic(raw_path) {
					return Some(text);
				}
				let text = bundled_fs.read_file(raw_path)?;
				let path = gopath::to_slash(raw_path);
				if tsconfig::is_tsconfig_path(&path) || path == config_path || chain.iter().any(|link| *link == path) {
					return Some(tsconfig::sanitize_tsconfig(&text));
				}
				if tsconfig::is_compiler_types_dts_path(&path) {
					return Some(tsconfig::rewrite_iterable_arity(&text));
				}
				Some(text)
			})),
			..Default::default()
		},
	))
}

/// Whether a root file named `companion` (or `rotor.d.ts`) already declares `marker`, so the
/// synthetic declaration must stay out (`projectDeclaresEnvOnDisk`, `projectDeclaresMacrosOnDisk`).
fn declares_on_disk(fs: &dyn FS, file_names: &[String], companion: &str, marker: &str) -> bool {
	file_names.iter().any(|name| {
		let base = name.rsplit('/').next().unwrap_or(name);
		(base.eq_ignore_ascii_case(companion) || base.eq_ignore_ascii_case(ROTOR_TYPES_FILE_NAME))
			&& fs.read_file(name).is_some_and(|text| text.contains(marker))
	})
}

/// Applies `ApplyAutomaticTypes`, `ApplyCheckerOverride` and `ApplySingleThreadedOverride`.
fn apply_overrides(options: &CompilerOptions, overrides: &ProjectOptions) -> CompilerOptions {
	let mut options = options.clone();
	if options.types.is_none() {
		options.types = Some(vec!["*".to_owned()]);
	}
	if let Some(checkers) = overrides.checkers {
		options.checkers = Some(checkers);
	}
	if let Some(single_threaded) = overrides.single_threaded {
		options.single_threaded = if single_threaded {
			Tristate::True
		} else {
			Tristate::False
		};
	}
	options
}

/// Parses `config_path` into the command line a program is built from (`parseProjectCommandLine`).
///
/// `project_dir` is the host's current directory and the directory rbxtsc's validation resolves
/// `typeRoots` against; the synthetic declarations sit beside the config.
fn parse_command_line(
	project_dir: &str,
	config_path: &str,
	fs: &Arc<dyn FS>,
	overrides: &ProjectOptions,
) -> Result<ParsedCommandLine, LoadError> {
	let directory = tspath::get_directory_path(config_path);
	let parse_host = ConfigHost {
		fs: Arc::clone(fs),
		cwd: project_dir.to_owned(),
	};
	let (parsed, diagnostics) = get_parsed_command_line_of_config_file(config_path, None, None, &parse_host, None);
	if !diagnostics.is_empty() {
		return Err(LoadError::Config(
			diagnostics.iter().map(|diagnostic| diagnostic.localize()).collect(),
		));
	}
	let mut parsed = parsed.ok_or(LoadError::Config(Vec::new()))?;
	let options = parsed
		.compiler_options()
		.map(|options| apply_overrides(&options, overrides))
		.ok_or(LoadError::Config(Vec::new()))?;
	if !overrides.skip_validation {
		let raw = tsconfig::read_raw_enforced_options(&gopath::from_slash(config_path));
		if let Some(message) = tsconfig::validate_compiler_options(&options, project_dir, &raw) {
			return Err(LoadError::Invalid(message));
		}
	}
	parsed.parsed_config.compiler_options = Some(P::new(options));

	let file_names = &mut parsed.parsed_config.file_names;
	if !declares_on_disk(&**fs, file_names, ENV_DECL_COMPANION, "declare const $env") {
		file_names.push(format!("{directory}/{ENV_DECL_FILE_NAME}"));
	}
	if !declares_on_disk(&**fs, file_names, MACRO_DECL_COMPANION, "declare function $nameof") {
		file_names.push(format!("{directory}/{MACRO_DECL_FILE_NAME}"));
	}
	Ok(parsed)
}

/// Parses `tsconfig` without building the program (`parseProjectConfig`).
///
/// The command line is the one a program would be built from: the same sanitized file system,
/// synthetic declarations, overrides and validation as [`load_project`].
///
/// # Errors
///
/// Returns [`LoadError`] if the path is unusable, the tsconfig has diagnostics, or it fails
/// rbxtsc's validation (unless [`ProjectOptions::skip_validation`]).
pub fn parse_project_config(tsconfig: &StdPath, options: &ProjectOptions) -> Result<ParsedCommandLine, LoadError> {
	let tsconfig = tsconfig.to_str().ok_or(LoadError::NonUtf8Path)?;
	let cwd = std::env::current_dir().map_err(LoadError::CurrentDirectory)?;
	let cwd = tspath::normalize_slashes(&cwd.to_string_lossy());
	let config_path = tspath::get_normalized_absolute_path(tsconfig, &cwd);
	let directory = tspath::get_directory_path(&config_path);
	let fs = project_fs(&config_path);
	parse_command_line(&directory, &config_path, &fs, options)
}

/// Loads `tsconfig` into a bound program the way rotor's build does.
///
/// The project directory is the tsconfig's directory. Must run on a large stack
/// ([`crate::with_large_stack`]) once the program is checked.
///
/// # Errors
///
/// Returns [`LoadError`] if the path is unusable, the tsconfig has diagnostics, or it fails
/// rbxtsc's validation (unless [`ProjectOptions::skip_validation`]).
pub fn load_project(tsconfig: &StdPath, options: &ProjectOptions) -> Result<Project, LoadError> {
	load_project_in(tsconfig, None, options)
}

/// [`load_project`] with an explicit project directory.
///
/// The directory is `newProjectProgramFromFSWithOptions`'s `dir`, which differs from the
/// tsconfig's directory under `--project`: the compiler host's current directory and the base
/// `typeRoots` validation resolves against. `None` is the tsconfig's directory.
///
/// # Errors
///
/// As [`load_project`].
pub fn load_project_in(
	tsconfig: &StdPath,
	project_dir: Option<&str>,
	options: &ProjectOptions,
) -> Result<Project, LoadError> {
	let tsconfig = tsconfig.to_str().ok_or(LoadError::NonUtf8Path)?;
	let cwd = std::env::current_dir().map_err(LoadError::CurrentDirectory)?;
	let cwd = tspath::normalize_slashes(&cwd.to_string_lossy());
	let config_path = tspath::get_normalized_absolute_path(tsconfig, &cwd);
	let directory = project_dir.map_or_else(
		|| tspath::get_directory_path(&config_path),
		|dir| tspath::get_normalized_absolute_path(&tspath::normalize_slashes(dir), &cwd),
	);

	let fs = project_fs(&config_path);
	let parsed = parse_command_line(&directory, &config_path, &fs, options)?;
	let host = new_cached_fs_compiler_host(&directory, fs, &bundled::default_lib_path(), None, None, None);
	let program = new_program(ProgramOptions::new(P::new(parsed), host));
	program.bind_source_files();
	Ok(Project { program })
}

/// The source files rotor compiles: not declarations, not from external libraries or project
/// references, and `.ts`/`.tsx` (`projectSourceFiles`).
#[must_use]
#[expect(
	clippy::case_sensitive_file_extension_comparisons,
	reason = "Go's projectSourceFiles matches the extensions case-sensitively"
)]
pub fn project_source_files(program: &Program) -> Vec<P<SourceFile>> {
	program
		.source_files()
		.iter()
		.copied()
		.filter(|&file| {
			let name = file.file_name();
			!file.is_declaration_file()
				&& !program.is_source_file_from_external_library(file)
				&& !program.is_source_from_project_reference(file.path())
				&& (name.ends_with(".ts") || name.ends_with(".tsx"))
		})
		.collect()
}

/// Why overlays could not be applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverlayError {
	/// An overlay names a file the program does not hold.
	Missing(String),
}

impl fmt::Display for OverlayError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Missing(path) => write!(formatter, "compile: overlay source missing from Program: {path}"),
		}
	}
}

impl std::error::Error for OverlayError {}

/// The checker count of a program derived from a project's (`derivedProgramDefaultCheckers`).
pub const DERIVED_PROGRAM_CHECKERS: i64 = 8;

/// The checker count a derived program's pool gets (`derivedProgramCheckers`):
/// [`DERIVED_PROGRAM_CHECKERS`], unless the project set a count or runs single-threaded, in which
/// case `None` keeps its setting.
#[must_use]
pub fn derived_program_checkers(program: &Program) -> Option<i64> {
	if program.options().checkers.is_some() || program.single_threaded() {
		return None;
	}
	Some(DERIVED_PROGRAM_CHECKERS)
}

/// Derives a program from `program` with the texts of `overlays` (keyed by file name) replacing
/// the files they name (`updateProgramWithTextOverlays`).
///
/// Overlays whose text equals the file's are skipped, and with nothing left to change `program`
/// itself comes back. The changed files go in program order, one `update_program` call each, over
/// a host whose file system serves every overlay text; the derived pool gets
/// [`derived_program_checkers`] checkers. The flag is whether every call reused the program it was
/// given, Go's `reused`.
///
/// # Errors
///
/// Returns [`OverlayError::Missing`] for an overlay naming a file the program does not hold: the
/// first one in Go's order, where a missing file sorts with the program's first file.
pub fn update_with_overlays(
	program: &'static Program,
	overlays: &BTreeMap<String, String>,
) -> Result<(&'static Program, bool), OverlayError> {
	update_with_overlays_checkers(program, overlays, derived_program_checkers(program))
}

/// [`update_with_overlays`] with the derived pool's checker count chosen by the caller.
///
/// `None` keeps `program`'s own count (its `checkers` option, else the default), the count a fresh
/// program over the same config gets. rotor's single-project compile builds its overlaid program
/// that way (Go's second `newProjectProgramFromFSWithOptions`), so checker groups and declaration
/// emit order match a build without overlays.
///
/// # Errors
///
/// As [`update_with_overlays`].
pub fn update_with_overlays_checkers(
	program: &'static Program,
	overlays: &BTreeMap<String, String>,
	checkers: Option<i64>,
) -> Result<(&'static Program, bool), OverlayError> {
	if overlays.is_empty() {
		return Ok((program, true));
	}
	let case_sensitive = program.use_case_sensitive_file_names();
	let normalize = move |path: &str| {
		let path = tspath::normalize_slashes(path);
		if case_sensitive { path } else { path.to_lowercase() }
	};
	let order: std::collections::HashMap<String, usize> = program
		.source_files()
		.iter()
		.enumerate()
		.map(|(index, file)| (normalize(file.file_name()), index))
		.collect();
	let mut names: Vec<&String> = overlays.keys().collect();
	names.sort_by_key(|name| order.get(&normalize(name)).copied().unwrap_or(0));

	let mut changed = Vec::with_capacity(names.len());
	let mut texts = BTreeMap::new();
	for name in names {
		let file = program
			.get_source_file(name)
			.ok_or_else(|| OverlayError::Missing(name.clone()))?;
		let text = &overlays[name];
		texts.insert(normalize(file.file_name()), text.clone());
		if file.text() != text {
			changed.push(file.path().clone());
		}
	}
	if changed.is_empty() {
		return Ok((program, true));
	}

	let base_fs: &'static dyn FS = program.host().fs();
	let texts = Arc::new(texts);
	let exists_texts = Arc::clone(&texts);
	let exists_normalize = normalize;
	let overlay_fs = wrapvfs::wrap(
		base_fs,
		wrapvfs::Replacements {
			file_exists: Some(Box::new(move |path: &str| {
				exists_texts.contains_key(&exists_normalize(path)) || base_fs.file_exists(path)
			})),
			read_file: Some(Box::new(move |path: &str| {
				texts.get(&normalize(path)).cloned().or_else(|| base_fs.read_file(path))
			})),
			..Default::default()
		},
	);
	let host: Arc<dyn CompilerHost> = new_cached_fs_compiler_host(
		program.get_current_directory(),
		Arc::new(overlay_fs),
		program.host().default_library_path(),
		None,
		None,
		None,
	);
	let mut derived = program;
	let mut reused = true;
	for path in changed {
		let (next, _, kept) = derived.update_program_with_checkers(&path, Arc::clone(&host), None, None, checkers);
		derived = next;
		reused &= kept;
	}
	Ok((derived, reused))
}

/// Runs `visit` for every file of `files` with the checker that checks it.
///
/// There is one task per checker, each visiting its files in the order of `files`
/// (`for_each_checker_group`); `visit` gets the file's index in `files`. False, without visiting
/// anything, when the program has no checker pool of its own.
pub fn for_each_checker_group(
	program: &Program,
	files: &[P<SourceFile>],
	visit: impl Fn(&mut TypeChecker<'_>, usize, P<SourceFile>) + Sync,
) -> bool {
	program.for_each_checker_group(files, |checker: &mut Checker, index, file| {
		visit(&mut TypeChecker::new(checker), index, file);
	})
}
