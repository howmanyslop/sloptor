//! The integration layer between rotor and the vendored tsrs checker.
//!
//! This is the smoke surface from the spike: load a tsconfig into a bound program, count its
//! diagnostics, and replay checker queries over its files (see [`replay`]).

pub mod replay;

use std::fmt;
use std::path::Path;
use std::sync::Arc;

use tsrs_compiler::{Program, ProgramOptions, new_cached_fs_compiler_host, new_program};
use tsrs_core::{CompilerOptions, Context, P, Tristate, tspath};
use tsrs_tsoptions::{ParseConfigHost, get_parsed_command_line_of_config_file};
use tsrs_vfs::{FS, bundled, osvfs, wrapvfs};

/// The checker recurses deeply, and tsrs runs it on 512 MiB stacks.
const CHECKER_STACK_BYTES: usize = 512 << 20;

/// Why a tsconfig could not be turned into a program.
#[derive(Debug)]
pub enum LoadError {
	/// The process working directory could not be read.
	CurrentDirectory(std::io::Error),
	/// The tsconfig path is not valid UTF-8.
	NonUtf8Path,
	/// tsrs could not parse the tsconfig at all.
	Config,
}

impl fmt::Display for LoadError {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::CurrentDirectory(error) => write!(formatter, "cannot read the current directory: {error}"),
			Self::NonUtf8Path => formatter.write_str("the tsconfig path is not valid UTF-8"),
			Self::Config => formatter.write_str("the tsconfig could not be parsed"),
		}
	}
}

impl std::error::Error for LoadError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::CurrentDirectory(error) => Some(error),
			Self::NonUtf8Path | Self::Config => None,
		}
	}
}

/// What loading a tsconfig produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramSummary {
	/// Source files in the program, including declaration files.
	pub source_files: usize,
	/// Config, program, syntactic, semantic and global diagnostics, of any category.
	pub diagnostics: usize,
}

/// A bound program and the diagnostics produced while parsing its config.
#[derive(Clone, Copy)]
pub struct OpenedProgram {
	/// The bound program.
	pub program: &'static Program,
	/// Diagnostics from reading and parsing the tsconfig.
	pub config_diagnostics: usize,
}

/// Runs `work` on a thread with the stack size the checker needs.
///
/// # Panics
///
/// Resumes the panic of `work`, if it panics.
pub fn with_large_stack<R: Send>(work: impl FnOnce() -> R + Send) -> R {
	std::thread::scope(|scope| {
		let spawned = std::thread::Builder::new()
			.name("rotor-tsrs".to_owned())
			.stack_size(CHECKER_STACK_BYTES)
			.spawn_scoped(scope, work);
		match spawned {
			Ok(handle) => handle
				.join()
				.unwrap_or_else(|payload| std::panic::resume_unwind(payload)),
			Err(error) => std::panic::resume_unwind(Box::new(error)),
		}
	})
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

/// Rewrites the `@rbxts/compiler-types` iteration interfaces to the arity TypeScript 7 expects
/// (the port of `internal/compile/sanitize.go`).
fn rewrite_iterable_arity(source: &str) -> String {
	let mut text = source.to_owned();
	for name in ["AsyncIterableIterator", "IterableIterator", "AsyncIterable", "Iterable"] {
		text = text.replace(
			&format!("interface {name}<T>"),
			&format!("interface {name}<T, TReturn = any, TNext = any>"),
		);
	}
	text
}

/// Opens `tsconfig` as a bound tsrs program.
///
/// The program, its file system and its checkers live for the rest of the process: tsrs hands out
/// `&'static` references, and rotor is a one-shot compiler. `checkers` overrides the checker pool
/// size when set.
///
/// # Errors
///
/// Returns [`LoadError`] if the working directory is unreadable or the tsconfig does not parse.
pub fn open_program(tsconfig: &Path, checkers: Option<i64>) -> Result<OpenedProgram, LoadError> {
	let tsconfig = tsconfig.to_str().ok_or(LoadError::NonUtf8Path)?;
	let cwd = std::env::current_dir().map_err(LoadError::CurrentDirectory)?;
	let cwd = tspath::normalize_slashes(&cwd.to_string_lossy());
	let config_path = tspath::get_normalized_absolute_path(tsconfig, &cwd);

	let bundled_fs: &'static dyn FS = Box::leak(Box::new(bundled::wrap_fs(osvfs::fs())));
	let sanitized = wrapvfs::wrap(
		bundled_fs,
		wrapvfs::Replacements {
			read_file: Some(Box::new(move |path: &str| {
				let text = bundled_fs.read_file(path)?;
				if path.contains("/node_modules/@rbxts/compiler-types/") && path.ends_with(".d.ts") {
					return Some(rewrite_iterable_arity(&text));
				}
				Some(text)
			})),
			..Default::default()
		},
	);
	let fs: Arc<dyn FS> = Arc::new(sanitized);
	let parse_host: &'static ConfigHost = Box::leak(Box::new(ConfigHost {
		fs: Arc::clone(&fs),
		cwd: cwd.clone(),
	}));
	let overrides = CompilerOptions {
		checkers,
		..CompilerOptions::default()
	};
	let (parsed, config_errors) =
		get_parsed_command_line_of_config_file(&config_path, Some(&overrides), None, parse_host, None);
	let parsed = parsed.ok_or(LoadError::Config)?;

	let host = new_cached_fs_compiler_host(&cwd, fs, &bundled::default_lib_path(), None, None, None);
	let mut options = ProgramOptions::new(P::new(parsed), host);
	options.single_threaded = Tristate::Unknown;
	let program = new_program(options);
	program.bind_source_files();
	Ok(OpenedProgram {
		program,
		config_diagnostics: config_errors.len(),
	})
}

/// Loads `tsconfig` and returns its source file and diagnostic counts.
///
/// Runs on a large-stack thread, so it is safe to call from a test or any other thread.
///
/// # Errors
///
/// Returns [`LoadError`] if the tsconfig cannot be opened.
pub fn load_program(tsconfig: &Path) -> Result<ProgramSummary, LoadError> {
	with_large_stack(|| {
		let OpenedProgram {
			program,
			config_diagnostics,
		} = open_program(tsconfig, None)?;
		let context = Context::default();
		let counts: [usize; 4] = [
			program.get_program_diagnostics().len(),
			program.get_syntactic_diagnostics(&context, None).len(),
			program.get_semantic_diagnostics(&context, None).len(),
			program.get_global_diagnostics(&context).len(),
		];
		Ok(ProgramSummary {
			source_files: program.source_files().len(),
			diagnostics: config_diagnostics + counts.iter().sum::<usize>(),
		})
	})
}
