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
use tsrs_ast::{SourceFile, SourceFileParseOptions};
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
	sanitized_fs(config_path, true)
}

/// [`project_fs`], with the synthetic declarations only when `synthetic` (`rotor check` reads the
/// project without them: `SanitizeFSWithConfigPath` alone).
fn sanitized_fs(config_path: &str, synthetic: bool) -> Arc<dyn FS> {
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
		if !synthetic {
			return None;
		}
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

/// What [`load_check_program`] built.
pub struct CheckProgram {
	/// The program; `None` when the tsconfig could not be read at all.
	pub program: Option<&'static Program>,
	/// Why the tsconfig could not be read, when [`CheckProgram::program`] is `None`.
	pub config_diagnostics: Vec<P<tsrs_ast::Diagnostic>>,
	/// The tsconfig's root file names.
	pub file_names: Vec<String>,
}

/// The program `rotor check` type-checks (`newCheckProgram` in `cmd/rotor/check.go`).
///
/// Unlike [`load_project`]: the tsconfig is parsed with the compiler host as its host, the synthetic
/// `$env` and macro declarations stay out, rbxtsc's validation is not applied, and the tsconfig's
/// own diagnostics are left in the program for the check to report. `ApplyAutomaticTypes` and the
/// `--checkers` override still apply. `project_dir` is absolute and slash-separated, and
/// `config_path` is the tsconfig inside it. Must run on a large stack ([`crate::with_large_stack`]).
#[must_use]
pub fn load_check_program(project_dir: &str, config_path: &str, checkers: Option<i64>) -> CheckProgram {
	let fs = sanitized_fs(config_path, false);
	let parse_host = ConfigHost {
		fs: Arc::clone(&fs),
		cwd: project_dir.to_owned(),
	};
	let (parsed, config_diagnostics) =
		get_parsed_command_line_of_config_file(config_path, None, None, &parse_host, None);
	let Some(mut parsed) = parsed else {
		return CheckProgram {
			program: None,
			config_diagnostics,
			file_names: Vec::new(),
		};
	};
	if let Some(options) = parsed.compiler_options() {
		let overrides = ProjectOptions {
			checkers,
			..ProjectOptions::default()
		};
		parsed.parsed_config.compiler_options = Some(P::new(apply_overrides(&options, &overrides)));
	}
	let file_names = parsed.file_names().to_vec();
	let host = new_cached_fs_compiler_host(project_dir, fs, &bundled::default_lib_path(), None, None, None);
	tsrs_core::compat::set_rotor_fork(true);
	let mut program_options = ProgramOptions::new(P::new(parsed), host);
	// Inside a `ProgramScope` (`check --watch`), the scope frees the program and its checkers.
	let tracked = tracking_programs();
	program_options.checker_regions = tracked;
	track_loader_regions(&mut program_options, None);
	let program = new_program(program_options);
	if tracked {
		track_program(program, true);
		// Bound before any checker runs, so no file's symbols live in a checker's region.
		program.bind_source_files();
	}
	CheckProgram {
		program: Some(program),
		config_diagnostics,
		file_names,
	}
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

thread_local! {
	/// The open [`ProgramScope`]s of this thread, innermost last.
	static PROGRAM_SCOPES: std::cell::RefCell<Vec<ScopeEntries>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// What one [`ProgramScope`] frees.
#[derive(Default)]
struct ScopeEntries {
	/// The programs created on this thread while the scope was the innermost, oldest first, each
	/// with whether it owns its processed-file data (a program built in full) rather than sharing
	/// its base's (a reused update).
	programs: Vec<(&'static Program, bool)>,
	/// The file-loader regions of a long-lived session's programs, dropped after the programs.
	regions: Vec<Arc<dyn std::any::Any + Send + Sync>>,
}

/// Whether the process is a long-lived session ([`set_long_lived`]).
static LONG_LIVED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Marks the process as a long-lived session (watch mode, the language server, the build API).
///
/// There a [`ProgramScope`] must give back everything its programs allocated, not only their
/// checkers: the file loader's parses and resolutions go to regions of their own, and the
/// processed-file data is freed with the last program that shares it. A one-shot build, which
/// frees nothing it does not have to, leaves this off.
pub fn set_long_lived(on: bool) {
	LONG_LIVED.store(on, std::sync::atomic::Ordering::Relaxed);
	if on {
		// The session frees its programs' regions, so lazily filled data of an object outside
		// every region (a command line, a shared declaration) must go to its owner's region or
		// the thread arena, never to whichever region the filling thread had entered (tsrs's
		// `bind_in_file_regions` does the same for its watch sessions).
		tsrs_core::arena::route_by_owner();
	}
}

/// Whether the process is a long-lived session ([`set_long_lived`]).
#[must_use]
pub fn long_lived() -> bool {
	LONG_LIVED.load(std::sync::atomic::Ordering::Relaxed)
}

/// File-loader regions several programs share.
///
/// These serve the projects of one solution drain, whose shared parse cache hands one project's
/// parses to the next. They are freed when the last handle goes,
/// which a [`ProgramScope`] holds until its programs are freed.
#[derive(Clone)]
pub struct LoaderRegions {
	apply: Arc<dyn Fn(&mut ProgramOptions) + Send + Sync>,
	keep: Arc<dyn std::any::Any + Send + Sync>,
}

impl LoaderRegions {
	/// Fresh regions.
	#[must_use]
	pub fn new() -> Self {
		let regions = Arc::default();
		let keep: Arc<dyn std::any::Any + Send + Sync> = Arc::clone(&regions) as _;
		Self {
			apply: Arc::new(move |options: &mut ProgramOptions| options.regions = Some(Arc::clone(&regions))),
			keep,
		}
	}
}

impl Default for LoaderRegions {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Debug for LoaderRegions {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str("LoaderRegions")
	}
}

/// A fresh region that this thread's innermost [`ProgramScope`] frees after its programs.
///
/// It is for a thread the scope's work spawns in a long-lived session: that thread enters it, so what it
/// allocates goes back with the scope instead of staying in its arena. `None` outside a
/// long-lived session or a scope.
#[must_use]
pub fn scope_thread_region() -> Option<tsrs_core::arena::Region> {
	if !long_lived() {
		return None;
	}
	PROGRAM_SCOPES.with(|scopes| {
		let mut scopes = scopes.borrow_mut();
		let scope = scopes.last_mut()?;
		let region = tsrs_core::arena::Region::new_scratch(1 << 20);
		scope.regions.push(Arc::new(region.clone()));
		Some(region)
	})
}

/// Whether a [`ProgramScope`] is open on this thread.
fn tracking_programs() -> bool {
	PROGRAM_SCOPES.with(|scopes| !scopes.borrow().is_empty())
}

/// Records `program` in this thread's innermost [`ProgramScope`], if any. `owns_shared` is false
/// for a reused update, which shares its base's processed-file data.
fn track_program(program: &'static Program, owns_shared: bool) {
	PROGRAM_SCOPES.with(|scopes| {
		if let Some(scope) = scopes.borrow_mut().last_mut() {
			scope.programs.push((program, owns_shared));
		}
	});
}

/// In a long-lived session, gives `options` file-loader regions that this thread's innermost
/// [`ProgramScope`] keeps until after its programs are freed: `shared` when given, or regions of
/// the program's own.
fn track_loader_regions(options: &mut ProgramOptions, shared: Option<LoaderRegions>) {
	if !long_lived() || !tracking_programs() {
		return;
	}
	let regions = shared.unwrap_or_default();
	(regions.apply)(options);
	PROGRAM_SCOPES.with(|scopes| {
		if let Some(scope) = scopes.borrow_mut().last_mut() {
			scope.regions.push(regions.keep);
		}
	});
}

/// Frees, when dropped, the programs this thread creates while the scope is open.
///
/// [`load_project_in_cached`] and [`update_with_overlays`] record them: Go drops a project's
/// program after its build. A tracked program's checkers allocate in regions of their own (tsrs's
/// `checker_regions`), so their types, symbols and caches go back too. Parsed files stay in the
/// thread arena, so a [`SourceFileCache`] may share them with later programs.
///
/// Programs are freed newest first, so a derived program goes before the program it was
/// derived from. A program created on another thread is not tracked and stays allocated.
#[must_use = "dropping the scope frees its programs"]
pub struct ProgramScope {
	/// In a long-lived session, the scratch region the scope's thread allocates in while the scope
	/// is open; exited first when the scope drops, and freed after its programs.
	region: Option<tsrs_core::arena::RegionScope>,
	_not_send: std::marker::PhantomData<*const ()>,
}

impl ProgramScope {
	/// Opens a scope on this thread.
	///
	/// # Safety
	///
	/// Nothing from a program created on this thread while the scope is open may be used after
	/// the scope is dropped: not the program, a checker of its pool, nor a type, symbol,
	/// diagnostic or other value a checker made. No thread may still be using one when it drops.
	/// In a long-lived session ([`set_long_lived`]) the same holds for every `P` this thread
	/// allocates while the scope is open and no inner region is entered.
	#[must_use = "dropping the scope frees its programs"]
	#[expect(
		unsafe_code,
		reason = "the caller promises the scope outlives every use of its programs"
	)]
	pub unsafe fn enter() -> Self {
		let mut entries = ScopeEntries::default();
		let region = long_lived().then(|| {
			let region = tsrs_core::arena::Region::new_scratch(1 << 20);
			let entered = region.enter();
			entries.regions.push(Arc::new(region));
			entered
		});
		PROGRAM_SCOPES.with(|scopes| scopes.borrow_mut().push(entries));
		Self {
			region,
			_not_send: std::marker::PhantomData,
		}
	}
}

impl Drop for ProgramScope {
	#[expect(
		unsafe_code,
		reason = "frees the programs whose use `ProgramScope::enter`'s contract ends here"
	)]
	fn drop(&mut self) {
		// Back to the allocation target the scope was opened over before anything is freed.
		drop(self.region.take());
		let entries = PROGRAM_SCOPES
			.with(|scopes| scopes.borrow_mut().pop())
			.unwrap_or_default();
		// The processed-file data each owner allocated, taken while its program is alive.
		let shared: Vec<tsrs_compiler::SharedProgramData> = if long_lived() {
			entries
				.programs
				.iter()
				.filter(|(_, owns)| *owns)
				.map(|(program, _)| tsrs_compiler::shared_program_data(program))
				.collect()
		} else {
			Vec::new()
		};
		for (program, _) in entries.programs.into_iter().rev() {
			// SAFETY: `program` came from `new_program` or `update_program` on this thread inside
			// this scope, and `enter`'s contract says nothing uses it any more. Newest first, so no
			// program derived from it is still alive.
			unsafe { tsrs_compiler::free_program(program) };
		}
		for data in shared {
			// SAFETY: every program of the scope is freed, so none shares this data any more: a
			// reused update shares its base's data, and both were in this scope (an update is
			// recorded in the scope its base is in). Nothing outside the scope refers to it.
			unsafe { data.free() };
		}
		// The regions go last: the programs and their data pointed into them.
		drop(entries.regions);
	}
}

/// A parse cache shared between the programs of a solution (Go's `solutionCompileCache`).
///
/// The program host hands every file it parses to [`SourceFileCache::get_source_file`] with the
/// host's own parse; the cache decides which files it shares and returns its parse of them.
pub trait SourceFileCache: Send + Sync {
	/// The source file for `opts`, from the cache or from `parse`.
	fn get_source_file(
		&self,
		opts: SourceFileParseOptions,
		parse: &dyn Fn(SourceFileParseOptions) -> Option<P<SourceFile>>,
	) -> Option<P<SourceFile>>;

	/// The loader regions every program sharing the cache parses into, which must outlive the
	/// cache's entries. `None` keeps the parses in the thread arenas.
	fn loader_regions(&self) -> Option<LoaderRegions> {
		None
	}
}

/// `cachingCompilerHost`: a compiler host whose parses go through a [`SourceFileCache`].
struct CachingHost {
	inner: Arc<dyn CompilerHost>,
	cache: Arc<dyn SourceFileCache>,
}

impl CompilerHost for CachingHost {
	fn fs(&self) -> &dyn FS {
		self.inner.fs()
	}

	fn default_library_path(&self) -> &str {
		self.inner.default_library_path()
	}

	fn get_current_directory(&self) -> &str {
		self.inner.get_current_directory()
	}

	fn trace(&self, msg: &'static tsrs_diagnostics::Message, args: &[&dyn fmt::Display]) {
		self.inner.trace(msg, args);
	}

	fn get_source_file(&self, opts: SourceFileParseOptions) -> Option<P<SourceFile>> {
		self.cache
			.get_source_file(opts, &|opts| self.inner.get_source_file(opts))
	}

	fn get_content_mapped_source_files(
		&self,
		parse_options: SourceFileParseOptions,
		mapper: &tsrs_contentmapper::Mapper,
	) -> Result<tsrs_contentmapper::SourceFiles, tsrs_contentmapper::Error> {
		self.inner.get_content_mapped_source_files(parse_options, mapper)
	}

	fn content_mapper_project(&self) -> Option<Arc<dyn tsrs_contentmapper::Project>> {
		self.inner.content_mapper_project()
	}

	fn get_resolved_project_reference(&self, file_name: &str, path: tspath::Path) -> Option<P<ParsedCommandLine>> {
		self.inner.get_resolved_project_reference(file_name, path)
	}
}

/// `solutionCompileCache.wrap`: `inner` with its parses going through `cache`.
#[must_use]
pub fn caching_compiler_host(inner: Arc<dyn CompilerHost>, cache: Arc<dyn SourceFileCache>) -> Arc<dyn CompilerHost> {
	Arc::new(CachingHost { inner, cache })
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
	load_project_in_cached(tsconfig, project_dir, options, None)
}

/// [`load_project_in`] whose program parses through `cache`.
///
/// # Errors
///
/// As [`load_project`].
pub fn load_project_in_cached(
	tsconfig: &StdPath,
	project_dir: Option<&str>,
	options: &ProjectOptions,
	cache: Option<Arc<dyn SourceFileCache>>,
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
	let mut host = new_cached_fs_compiler_host(&directory, fs, &bundled::default_lib_path(), None, None, None);
	// A shared cache's parses must outlive this program, so they go to the cache's regions or
	// stay in the thread arenas; a program of its own frees its loader's allocations with it.
	let shared_regions = cache.as_ref().map(|cache| cache.loader_regions());
	if let Some(cache) = cache {
		host = Arc::new(CachingHost { inner: host, cache });
	}
	// Union members print in Go's order only with the fork's orderings (`compat::rotor_fork`).
	tsrs_core::compat::set_rotor_fork(true);
	let mut program_options = ProgramOptions::new(P::new(parsed), host);
	// A tracked program's checkers allocate in regions of their own, which its scope frees.
	program_options.checker_regions = tracking_programs();
	match shared_regions {
		None => track_loader_regions(&mut program_options, None),
		Some(Some(regions)) => track_loader_regions(&mut program_options, Some(regions)),
		Some(None) => {}
	}
	let program = new_program(program_options);
	track_program(program, true);
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
		if !std::ptr::eq(next, derived) {
			track_program(next, !kept);
		}
		derived = next;
		reused &= kept;
	}
	if !std::ptr::eq(derived, program) {
		bind_outside_checkers(derived);
	}
	Ok((derived, reused))
}

/// Binds `program`'s files now, as [`load_project_in_cached`] does. A file a derived program
/// parsed again would otherwise be bound by its first checker, in that checker's region (tsrs's
/// `checker_regions`) under `singleThreaded`, and a later program that keeps the file would read
/// binder data the region's release freed.
fn bind_outside_checkers(program: &'static Program) {
	program.bind_source_files();
}

/// Derives a program from `program` whose files at `changed` are asked of `host` again, in that
/// order (Go `Program.UpdateProgramFilesWithCheckers`, through tsrs's
/// `update_program_files_with_checkers`).
///
/// The derived pool gets `checkers` checkers (`None` keeps `program`'s count). The flag is whether
/// `program`'s files were reused; without it the program was built again in full over `host`. A
/// new program is recorded in this thread's [`ProgramScope`], as [`update_with_overlays`] records
/// its programs.
pub fn update_program_files(
	program: &'static Program,
	changed: &[tspath::Path],
	host: Arc<dyn CompilerHost>,
	checkers: Option<i64>,
) -> (&'static Program, bool) {
	let (derived, reused) = program.update_program_files_with_checkers(changed, host, None, checkers);
	if !std::ptr::eq(derived, program) {
		track_program(derived, !reused);
		bind_outside_checkers(derived);
	}
	(derived, reused)
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
