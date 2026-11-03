//! `project.go`.
//!
//! One project end to end. The program ([`new_project_program_with_options`]), the project context
//! the transformer reads ([`new_project_context`]), the project options, and the per-file pipeline
//! [`compile_project`] runs over every source file: pre-emit diagnostics, then the transform and
//! render, one `MultiState` per checker group.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use rotor_config::dotenv::Env;
use rotor_rojo::{NetworkType, PathTranslator, RbxPathLookup, RojoResolver, RojoResolverCache};
use rotor_support::{gopath, logservice};
use rotor_transformer::{MultiState, NodeModulesPathMapping, ProjectType, RojoContext, State, StateOptions};
use rotor_tsrs::ast::{self, SourceFile};
use rotor_tsrs::compiler::Program;
use rotor_tsrs::tsoptions::ParsedCommandLine;
use rotor_tsrs::tsoptions::outputpaths;
use rotor_tsrs::{Context, P, tspath};

use crate::census::CensusCollector;
use crate::compile::{
	DiagnosticInfo, Error, Failure, comment_directive_diagnostics, pre_emit_project_file_diagnostics_with_options,
	string_diagnostics, transform_and_render_source_map_detailed, ts_diagnostic_infos,
};
use crate::crossproject::add_project_real_path_aliases;
use crate::declaration_prefetch::DeclarationPrefetch;
use crate::filemacro::FileResolver;
use crate::flamework_pipeline::prepare_compile_pipeline;
use crate::luau_types::effective_luau_types;
use crate::native_types::effective_native_types;
use crate::output::maybe_copy_include;
use crate::overlayfs::{match_program_overlays, rekey_overlays_to_program};
use crate::package_layout::{PackageLayout, prepare_package_layout};
use crate::packageexports::create_node_modules_path_mapping;
use crate::rbxts_options::{RbxtsOptions, merge_rbxts_options, read_rbxts_options};
use crate::solution::canonical_explicit_path;
use crate::source_trace::DiagnosticTraces;
use crate::timings::{BuildTimings, Stage};

/// The checker recurses deeply; tsrs runs it on 512 MiB stacks.
pub(crate) const CHECKER_STACK_BYTES: usize = 512 << 20;

/// `packageNameRegex` (`createProjectData.ts` `PACKAGE_REGEX`, `^@[a-z0-9-]*/`): a scoped package
/// name compiles as `ProjectType.Package`.
fn is_scoped_package_name(name: &str) -> bool {
	let Some(rest) = name.strip_prefix('@') else {
		return false;
	};
	let scope_end = rest
		.bytes()
		.position(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
		.unwrap_or(rest.len());
	rest.as_bytes().get(scope_end) == Some(&b'/')
}

/// `filenameWarnings` (`Shared/constants.ts` `FILENAME_WARNINGS`): `init.*` file names collide
/// with Rojo's directory-script convention; `checkFileName` suggests the `index.*` spelling.
fn filename_warning(base_name: &str) -> Option<String> {
	for script_type in [".server", ".client", ""] {
		for file_type in [".ts", ".tsx", ".d.ts"] {
			if base_name == format!("init{script_type}{file_type}") {
				return Some(format!("index{script_type}{file_type}"));
			}
		}
	}
	None
}

/// `projectContext`: upstream `ProjectData` plus the `compileFiles.ts` locals computed once per
/// pass, packaged as the transformer's [`RojoContext`].
pub struct ProjectContext {
	/// The absolute slash-separated project directory (upstream `projectPath`).
	pub dir: String,
	/// The project type.
	pub project_type: ProjectType,
	/// The transformer's Rojo context.
	pub rojo_context: RojoContext,
	/// The `$env` environment, reloaded per pass.
	pub env: Env,
	/// The `$file` reader, rooted at the project directory.
	pub files: Arc<FileResolver>,
	/// The resolved `nativeTypes` opt-in.
	pub native_types: bool,
	/// The resolved experimental `luauTypes` opt-in.
	pub luau_types: bool,
	/// Maps each file a transformer reprinted back to the text on disk; empty without transformer
	/// plugins.
	pub source_traces: DiagnosticTraces,
}

/// A cancellation flag: Go's `context.Context` for the parts of a build that check it.
#[derive(Clone, Debug, Default)]
pub struct Cancellation {
	flag: Arc<AtomicBool>,
	/// `context.WithTimeout`'s parent and deadline.
	parent: Option<Box<Self>>,
	deadline: Option<std::time::Instant>,
	#[cfg(test)]
	countdown: Option<Arc<Countdown>>,
}

/// A test cancellation that turns cancelled from its (after+1)th check on (Go's
/// `countdownContext`).
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct Countdown {
	after: i64,
	pub(crate) calls: std::sync::atomic::AtomicI64,
}

impl Cancellation {
	/// `context.WithTimeout(parent, timeout)`: cancelled with `parent` or once `timeout` passed.
	#[must_use]
	pub fn with_timeout(parent: Option<&Self>, timeout: std::time::Duration) -> Self {
		Self {
			parent: parent.map(|parent| Box::new(parent.clone())),
			deadline: std::time::Instant::now().checked_add(timeout),
			..Self::default()
		}
	}

	/// Cancels the work holding this flag.
	pub fn cancel(&self) {
		self.flag.store(true, Ordering::Relaxed);
	}

	/// Whether [`Cancellation::cancel`] ran, the parent was cancelled, or the deadline passed.
	#[must_use]
	pub fn is_cancelled(&self) -> bool {
		self.err().is_some()
	}

	/// `ctx.Err()`'s text: `context canceled` once cancelled, `context deadline exceeded` once
	/// the deadline passed first.
	#[must_use]
	pub fn err(&self) -> Option<&'static str> {
		#[cfg(test)]
		if let Some(countdown) = &self.countdown {
			if countdown.calls.fetch_add(1, Ordering::Relaxed) < countdown.after {
				return None;
			}
			self.cancel();
		}
		if self.flag.load(Ordering::Relaxed) {
			return Some("context canceled");
		}
		if let Some(error) = self.parent.as_ref().and_then(|parent| parent.err()) {
			return Some(error);
		}
		self.deadline
			.is_some_and(|deadline| std::time::Instant::now() >= deadline)
			.then_some("context deadline exceeded")
	}

	/// A cancellation that reports cancelled from its (after+1)th check on.
	#[cfg(test)]
	pub(crate) fn countdown(after: i64) -> (Self, Arc<Countdown>) {
		let countdown = Arc::new(Countdown {
			after,
			..Countdown::default()
		});
		(
			Self {
				countdown: Some(Arc::clone(&countdown)),
				..Self::default()
			},
			countdown,
		)
	}
}

/// A persistence step a solution build defers until every project built. A failed step stays
/// queued and runs again on the next drain.
pub type PendingPersist = Box<dyn FnMut() -> Result<(), Error> + Send>;

/// `ProjectOptions`: the CLI-controlled knobs of upstream `ProjectOptions` that affect
/// compilation. The default is upstream's defaults without include emission.
#[derive(Clone, Default)]
#[expect(clippy::struct_excessive_bools, reason = "Go's ProjectOptions fields")]
pub struct ProjectOptions {
	pub(crate) package_layout: Option<Arc<PackageLayout>>,
	pub(crate) rojo_cache: Option<Arc<RojoResolverCache>>,
	/// The build's timing report.
	pub timings: Option<Arc<BuildTimings>>,
	/// Cancels build work; `None` is the CLI's process-owned lifecycle.
	pub context: Option<Cancellation>,
	pub(crate) project_reference_paths: Option<Arc<HashMap<String, String>>>,
	pub(crate) project_reference_salt_paths: Option<Arc<HashMap<String, String>>>,
	pub(crate) pending_solution_persists: Option<Arc<Mutex<Vec<PendingPersist>>>>,
	/// Persistence a dependant reads, published as soon as the project built.
	pub(crate) pending_solution_dependency_persists: Option<Arc<Mutex<Vec<PendingPersist>>>>,
	pub(crate) defer_rojo_cache_persist: bool,
	/// The drain's shared parse cache.
	pub(crate) compile_cache: Option<Arc<crate::solution_cache::SolutionCompileCache>>,
	/// Set only by a solution census: the solution-wide overlay matches.
	pub(crate) solution_overlays: Option<Arc<crate::census_solution::SolutionOverlayMatches>>,
	/// The one-shot CLI build's parallel transformer token.
	pub(crate) cli_transformer_build: Option<Arc<crate::sidecar_parallel::CliTransformerBuild>>,
	/// Runs transformer requests in an API client instead of the Node sidecar (Go carries it on
	/// the context).
	pub(crate) transformer_callback: Option<Arc<crate::transformer_host_sidecar::CallbackTransformerHost>>,
	/// Runs when this build selected source files to compile, before any stage.
	pub(crate) on_compile_start: Option<Arc<dyn Fn() + Send + Sync>>,
	/// The raw `--includePath`; empty means `<projectDir>/include`.
	pub include_path: String,
	/// Whether to write the runtime library (`copyInclude.ts`).
	pub emit_include_files: bool,
	/// The `--type` override; `None` infers it.
	pub project_type: Option<ProjectType>,
	/// A custom config file; empty means `<projectDir>/tsconfig.json`.
	pub ts_config_path: String,
	/// Overrides `compilerOptions.checkers`.
	pub checkers: Option<i64>,
	/// Overrides the solution builder count.
	pub builders: Option<i64>,
	/// Overrides `compilerOptions.singleThreaded`.
	pub single_threaded: Option<bool>,
	/// The `--rojo` override; empty auto-discovers.
	pub rojo_config_path: String,
	/// `--logTruthyChanges`
	pub log_truthy_changes: bool,
	/// `--allowCommentDirectives`
	pub allow_comment_directives: bool,
	/// Flamework's `noSemanticDiagnostics`: skips the per-file semantic pre-emit check.
	pub skip_semantic_diagnostics: bool,
	/// rotor's loop-carried `Array.push` index optimization (default off).
	pub optimized_array_appends: bool,
	/// The fallback layer of `nativeTypes`.
	pub native_types: bool,
	/// The fallback layer of `luauTypes`.
	pub luau_types: bool,
	/// The inverse of upstream `optimizedLoops`.
	pub no_optimized_loops: bool,
	/// The inverse of upstream `luau`: emit `.lua`.
	pub lua_extension: bool,
	/// The command-line layer of the rbxts option merge.
	pub solution_argv: Option<RbxtsOptions>,
	/// Leaves unchanged outputs and copies untouched on disk.
	pub write_only_changed: bool,
	/// Emits declarations only.
	pub emit_declaration_only: bool,
	pub(crate) census: Option<Arc<CensusCollector>>,
	/// Replaces the on-disk text of source files for one compile, keyed by absolute path.
	pub overlays: BTreeMap<String, String>,
	/// Drops overlay keys that name no file of the program instead of failing.
	pub ignore_unmatched_overlays: bool,
	pub(crate) declaration_prefetch: Option<Arc<DeclarationPrefetch>>,
}

impl ProjectOptions {
	/// `context().Err() != nil`
	pub(crate) fn cancelled(&self) -> bool {
		self.context.as_ref().is_some_and(Cancellation::is_cancelled)
	}

	pub(crate) fn timings(&self) -> Option<&BuildTimings> {
		self.timings.as_deref()
	}
}

/// `newProjectProgramWithOptions`: the program over the sanitized config, shared by
/// [`crate::compile::compile_file`] and [`compile_project`]. Returns the absolute
/// slash-separated project directory and the program.
///
/// # Errors
///
/// The config's diagnostics, rbxtsc's validation message, or an overlay that matches no file.
pub fn new_project_program_with_options(
	project_dir: &str,
	ts_config_path: &str,
	opts: &ProjectOptions,
) -> Result<(String, &'static Program), Failure> {
	let dir = gopath::abs(project_dir).map_err(|error| Failure::bare(Error::message(error.to_string())))?;
	let dir = gopath::to_slash(&dir).into_owned();

	let config_path = if ts_config_path.is_empty() {
		format!("{dir}/tsconfig.json")
	} else {
		let abs = gopath::abs(ts_config_path).map_err(|error| Failure::bare(Error::message(error.to_string())))?;
		gopath::to_slash(&abs).into_owned()
	};

	let load_options = rotor_tsrs::ProjectOptions {
		checkers: opts.checkers,
		single_threaded: opts.single_threaded,
		skip_validation: false,
	};
	let cache = opts.compile_cache.as_ref().map(|cache| {
		let mut skip = crate::solution_cache::synthetic_decl_skip(&config_path);
		skip.extend(opts.overlays.keys().cloned());
		Arc::new(crate::solution_cache::ProjectParseCache::new(Arc::clone(cache), skip))
			as Arc<dyn rotor_tsrs::project::SourceFileCache>
	});
	let program = load_program_cached(&dir, &config_path, &load_options, cache)?;
	if opts.overlays.is_empty() {
		return Ok((dir, program));
	}
	let (overlays, mut unmatched) = rekey_overlays_to_program(program, &opts.overlays);
	if !unmatched.is_empty() && opts.solution_overlays.is_none() && !opts.ignore_unmatched_overlays {
		unmatched.sort();
		let message = format!(
			"compile: overlay matches no file in the program: {}",
			unmatched.join(", ")
		);
		return Err(Failure::strings(vec![message.clone()], Error::message(message)));
	}
	// Go builds a fresh program over the overlay file system with the configured checker count,
	// so the overlaid program keeps the base program's count rather than a derived pool's.
	let (program, _) =
		rotor_tsrs::project::update_with_overlays_checkers(program, &overlays, None).map_err(|error| {
			let message = error.to_string();
			Failure::strings(vec![message.clone()], Error::message(message))
		})?;
	let matched = match_program_overlays(program, opts).map_err(|error| {
		let message = error.to_string();
		Failure::strings(vec![message], error)
	})?;
	if let Some(census) = &opts.census {
		census.set_overlay_matches(matched);
	}
	Ok((dir, program))
}

/// `newProjectProgramFromFSWithOptions` over the OS file system: loads and binds the program,
/// mapping the load errors to Go's diagnostics and errors.
/// A solution's shared parse cache, when set, parses the files the projects share.
fn load_program_cached(
	dir: &str,
	config_path: &str,
	options: &rotor_tsrs::ProjectOptions,
	cache: Option<Arc<dyn rotor_tsrs::project::SourceFileCache>>,
) -> Result<&'static Program, Failure> {
	match rotor_tsrs::project::load_project_in_cached(Path::new(config_path), Some(dir), options, cache) {
		Ok(project) => Ok(project.program),
		Err(rotor_tsrs::LoadError::Config(diagnostics)) => Err(Failure::strings(
			diagnostics,
			Error::message("compile: tsconfig.json has errors"),
		)),
		Err(rotor_tsrs::LoadError::Invalid(message)) => Err(Failure::strings(
			vec![message],
			Error::message("compile: invalid tsconfig.json configuration"),
		)),
		Err(error) => Err(Failure::bare(Error::message(error.to_string()))),
	}
}

/// `projectIsPackage` (`createProjectData.ts` L15-26): `package.json` discovery walks up from the
/// project; a missing one is an error, an unreadable one just means "not a package".
pub(crate) fn project_is_package(dir: &str) -> Result<(String, bool), Error> {
	let Some(pkg_json_path) = find_package_json(dir) else {
		return Err(Error::message("compile: Unable to find package.json"));
	};
	let is_package = std::fs::read(&pkg_json_path)
		.ok()
		.and_then(|data| package_json_name(&data))
		.is_some_and(|name| is_scoped_package_name(&name));
	Ok((pkg_json_path, is_package))
}

/// `json.Unmarshal` into a struct whose `Name string` field is tagged `json:"name"`: the key matches
/// case-insensitively and the last match wins; `None` where the unmarshal fails (not an object,
/// or a matching value that is not a string or `null`).
pub(crate) fn package_json_name(data: &[u8]) -> Option<String> {
	let mut name = String::new();
	match crate::gojson::parse(data).ok()? {
		crate::gojson::Value::Null => {}
		crate::gojson::Value::Object(members) => {
			for (key, value) in members {
				if !key.eq_ignore_ascii_case("name") {
					continue;
				}
				match value {
					crate::gojson::Value::String(text) => name = text,
					crate::gojson::Value::Null => {}
					_ => return None,
				}
			}
		}
		_ => return None,
	}
	Some(name)
}

/// `resolveIncludePath` (`createProjectData.ts` L29): empty falls back to `<projectPath>/include`;
/// otherwise resolved against the working directory like Node's `path.resolve`.
pub(crate) fn resolve_include_path(dir: &str, include_path: &str) -> Result<String, Error> {
	if include_path.is_empty() {
		return Ok(gopath::join(&[&gopath::from_slash(dir), "include"]));
	}
	let abs = gopath::abs(&gopath::from_slash(include_path)).map_err(|error| Error::message(error.to_string()))?;
	Ok(canonical_explicit_path(&abs))
}

/// `newProjectContext` (`compileFiles.ts` L56-100): the Rojo resolver, `checkRojoConfig` and
/// `checkFileName`, the project type, and the runtime library path's validation.
///
/// # Errors
///
/// The four plain-text emit failures, the project configuration diagnostics, and setup errors.
pub fn new_project_context(
	dir: &str,
	program: &'static Program,
	opts: &ProjectOptions,
) -> Result<ProjectContext, Failure> {
	new_project_context_with_import_context(dir, program, opts, true)
}

/// `newProjectContextWithImportContext`: only an empty final transform selection can omit the
/// import-resolution setup.
#[expect(clippy::too_many_lines, reason = "one function in Go")]
pub(crate) fn new_project_context_with_import_context(
	dir: &str,
	program: &'static Program,
	opts: &ProjectOptions,
	needs_transform_import_context: bool,
) -> Result<ProjectContext, Failure> {
	let options = program.options();
	let out_dir = options.out_dir.clone();

	let (pkg_json_path, is_package) = project_is_package(dir).map_err(Failure::bare)?;
	let include_path = resolve_include_path(dir, &opts.include_path).map_err(Failure::bare)?;
	let (rojo_config_path, rojo_warnings) =
		resolve_rojo_config_path(dir, &opts.rojo_config_path).map_err(Failure::bare)?;
	for warning in &rojo_warnings {
		logservice::warn(warning);
	}

	// compileFiles.ts L61-63.
	let rojo_resolver = rojo_config_path.as_ref().map_or_else(
		|| RojoResolver::synthetic(&gopath::from_slash(&out_dir)),
		|path| {
			opts.rojo_cache
				.as_ref()
				.map_or_else(|| RojoResolver::from_path(path), |cache| (*cache.load(path)).clone())
		},
	);

	// compileFiles.ts L65-67: resolver parse warnings.
	for warning in rojo_resolver.warnings() {
		logservice::warn(warning);
	}

	let path_translator = create_path_translator(program, !opts.lua_extension);
	let import_path_map = opts.project_reference_paths.as_ref().map_or_else(
		|| create_cross_project_import_path_map(program, opts),
		|paths| (**paths).clone(),
	);

	// checkRojoConfig + checkFileName queue project-level diagnostics (compileFiles.ts L69-75);
	// upstream flushes them only after the emit failures below, so the queue is checked last.
	let root_dirs = get_root_dirs(program).map_err(Failure::bare)?;
	let mut check_diags = check_rojo_config(
		rojo_config_path.as_deref(),
		&rojo_resolver,
		&root_dirs,
		&path_translator,
	);
	let node_modules_path = gopath::join(&[&gopath::dir(&pkg_json_path), "node_modules"]);
	for source_file in program.source_files() {
		let file_name = gopath::from_slash(source_file.file_name());
		if !file_name.starts_with(&node_modules_path)
			&& let Some(message) = check_file_name(&file_name)
		{
			check_diags.push(message);
		}
	}

	// compileFiles.ts L80: data.projectOptions.type ?? inferProjectType(...).
	let project_type = opts.project_type.unwrap_or(if is_package {
		ProjectType::Package
	} else if rojo_resolver.is_game {
		ProjectType::Game
	} else {
		ProjectType::Model
	});

	// The four plain-text emit failures (compileFiles.ts L82-98).
	let emit_failure =
		|message: &str| Failure::strings(vec![message.to_owned()], Error::message("compile: emit failure"));
	if project_type != ProjectType::Package && rojo_config_path.is_none() {
		return Err(emit_failure("Non-package projects must have a Rojo project file!"));
	}
	let runtime_lib_rbx_path = if project_type == ProjectType::Package {
		None
	} else {
		let runtime_lib = gopath::join(&[&include_path, "RuntimeLib.lua"]);
		let RbxPathLookup::Found(path) = rojo_resolver.lookup_rbx_path(&runtime_lib) else {
			return Err(emit_failure("Rojo project contained no data for include folder!"));
		};
		if rojo_resolver.get_network_type(&path) != NetworkType::Unknown {
			return Err(emit_failure(
				"Runtime library cannot be in a server-only or client-only container!",
			));
		}
		if rojo_resolver.is_isolated(&path) {
			return Err(emit_failure("Runtime library cannot be in an isolated container!"));
		}
		Some(path)
	};

	if !check_diags.is_empty() {
		return Err(Failure::strings(
			check_diags,
			Error::message("compile: project configuration diagnostics"),
		));
	}

	// Import-resolution context (compileFiles.ts L77-78).
	let use_case_sensitive_file_names = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	let type_roots = options.type_roots.clone().unwrap_or_default();
	let mut pkg_rojo_resolvers = Vec::new();
	let mut node_modules_mapping = NodeModulesPathMapping::default();
	if needs_transform_import_context {
		let layout = match &opts.package_layout {
			Some(layout) => Some(Arc::clone(layout)),
			None => prepare_package_layout(dir, &options, opts)
				.map_err(Failure::bare)?
				.map(Arc::new),
		};
		match layout {
			Some(layout) => {
				pkg_rojo_resolvers.clone_from(&layout.resolvers);
				node_modules_mapping = layout.mapping.clone();
			}
			None => {
				node_modules_mapping = create_node_modules_path_mapping(
					&type_roots,
					use_case_sensitive_file_names,
					options.custom_conditions.as_deref().unwrap_or_default(),
				);
			}
		}
	}

	// $env macro environment: the .env files live next to the tsconfig.
	let env_dir = if options.config_file_path.is_empty() {
		gopath::from_slash(dir).into_owned()
	} else {
		gopath::dir(&gopath::from_slash(&options.config_file_path))
	};

	Ok(ProjectContext {
		dir: dir.to_owned(),
		project_type,
		env: Env::load(&env_dir),
		files: Arc::new(FileResolver::new(&env_dir)),
		native_types: effective_native_types(dir, &options.config_file_path, opts.native_types),
		luau_types: effective_luau_types(
			dir,
			&options.config_file_path,
			opts.solution_argv.as_ref(),
			opts.luau_types,
		),
		rojo_context: RojoContext {
			resolver: rojo_resolver,
			path_translator,
			runtime_lib_rbx_path,
			project_path: gopath::from_slash(dir).into_owned(),
			pkg_rojo_resolvers,
			node_modules_path_mapping: node_modules_mapping,
			project_reference_paths: import_path_map,
			node_modules_path,
			type_roots,
			use_case_sensitive_file_names,
		},
		source_traces: DiagnosticTraces::default(),
	})
}

/// `resolveRojoConfigPath`: auto-discovery (with its warnings) when unset, otherwise the
/// `path.resolve`d override.
pub(crate) fn resolve_rojo_config_path(
	dir: &str,
	configured_path: &str,
) -> Result<(Option<String>, Vec<String>), Error> {
	if configured_path.is_empty() {
		return Ok(rotor_rojo::find_rojo_config_file_path(&gopath::from_slash(dir)));
	}
	let abs = gopath::abs(&gopath::from_slash(configured_path)).map_err(|error| Error::message(error.to_string()))?;
	Ok((Some(canonical_explicit_path(&abs)), Vec::new()))
}

/// `createCrossProjectImportPathMap`
pub(crate) fn create_cross_project_import_path_map(
	program: &'static Program,
	opts: &ProjectOptions,
) -> HashMap<String, String> {
	create_project_reference_path_map(program, opts, true)
}

/// The referenced projects of `program`, keyed by config path (`GetResolvedProjectReferenceFor`,
/// which tsrs does not have).
fn resolved_project_references(program: &Program) -> HashMap<tspath::Path, P<ParsedCommandLine>> {
	let mut references = HashMap::new();
	program.range_resolved_project_reference(|path, config, _, _| {
		if let Some(config) = config {
			references.insert(path.clone(), config);
		}
		true
	});
	references
}

/// `createProjectReferencePathMap`: every referenced source and emitted declaration to its Luau
/// import path. Runtime lookups use OS-canonical keys and physical aliases; fingerprints keep the
/// reference config's spellings.
pub(crate) fn create_project_reference_path_map(
	program: &'static Program,
	opts: &ProjectOptions,
	canonical_keys: bool,
) -> HashMap<String, String> {
	let use_case_sensitive_file_names = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
	let file_key = |name: &str| {
		if canonical_keys {
			rotor_rojo::canonical_file_name(name, use_case_sensitive_file_names)
		} else {
			crate::incremental_manifest::normalize_source_file_path(name)
		}
	};
	let by_path = resolved_project_references(program);
	let mut result = HashMap::new();
	let mut visited: HashSet<*const ParsedCommandLine> = HashSet::new();
	let mut stack: Vec<(P<ParsedCommandLine>, bool)> = program
		.get_resolved_project_references()
		.into_iter()
		.rev()
		.flatten()
		.map(|reference| (reference, false))
		.collect();
	// Go recurses into the children before mapping the reference itself; the explicit stack
	// visits in the same order.
	while let Some((reference, children_done)) = stack.pop() {
		let key: *const ParsedCommandLine = &raw const *reference;
		if !children_done {
			if !visited.insert(key) {
				continue;
			}
			stack.push((reference, true));
			for config_path in reference.resolved_project_reference_paths().iter().rev() {
				let path = tspath::to_path(
					config_path,
					program.get_current_directory(),
					use_case_sensitive_file_names,
				);
				if let Some(&child) = by_path.get(&path) {
					stack.push((child, false));
				}
			}
			continue;
		}
		let Some(options) = reference.compiler_options() else {
			continue;
		};
		if options.out_dir.is_empty() {
			continue;
		}
		let mut root_dirs = if options.root_dir.is_empty() {
			options.root_dirs.clone().unwrap_or_default()
		} else {
			vec![options.root_dir.clone()]
		};
		if root_dirs.is_empty() {
			continue;
		}
		root_dirs.insert(0, reference.common_source_directory());
		// The reference emits with its own luau option, as in a solution build.
		let use_luau_extension = referenced_project_options(opts, reference.config_name(), false, None)
			.map_or(!opts.lua_extension, |referenced| !referenced.lua_extension);
		let translator = PathTranslator::new(
			&find_ancestor_dir(&root_dirs),
			&gopath::from_slash(&options.out_dir),
			"",
			options.get_emit_declarations(),
			use_luau_extension,
		);
		let mut paths = HashMap::new();
		for file_name in reference.file_names() {
			if tspath::is_declaration_file_name(file_name)
				&& !is_path_descendant_of(&gopath::from_slash(file_name), &translator.root_dir)
			{
				continue;
			}
			let import_path = translator.get_import_path(file_name, false);
			paths.insert(file_key(file_name), import_path.clone());
			if !tspath::is_declaration_file_name(file_name)
				&& std::path::Path::new(file_name)
					.extension()
					.is_none_or(|extension| extension != "json")
			{
				let declaration_path =
					outputpaths::get_output_declaration_file_name_worker(file_name, &options, &*reference);
				paths.insert(file_key(&declaration_path), import_path);
			}
		}
		if canonical_keys {
			add_project_real_path_aliases(
				&mut paths,
				&gopath::dir(reference.config_name()),
				use_case_sensitive_file_names,
			);
		}
		result.extend(paths);
	}
	result
}

/// `resolveAgainst`: Node's `path.resolve(base, p)` for two arguments.
pub(crate) fn resolve_against(base: &str, path: &str) -> String {
	let path = gopath::from_slash(path);
	if gopath::is_abs(&path) {
		return gopath::clean(&path);
	}
	gopath::join(&[base, &path])
}

/// `ProjectOptionsForReferencedConfig`
///
/// # Errors
///
/// The referenced config's `rbxts` options do not parse.
pub fn project_options_for_referenced_config(
	entry: &ProjectOptions,
	ts_config_path: &str,
	inherit_entry_type_and_rojo: bool,
) -> Result<ProjectOptions, Error> {
	referenced_project_options(entry, ts_config_path, inherit_entry_type_and_rojo, None)
}

/// `referencedProjectOptions`: a referenced project's options. `coordinator_rbxts` is a
/// coordinator root's own rbxts key, which applies only with `inherit_entry_type_and_rojo`.
pub(crate) fn referenced_project_options(
	entry: &ProjectOptions,
	ts_config_path: &str,
	inherit_entry_type_and_rojo: bool,
	coordinator_rbxts: Option<&RbxtsOptions>,
) -> Result<ProjectOptions, Error> {
	let declared = read_rbxts_options(ts_config_path)?;
	let mut referenced = entry.clone();
	if let Some(argv) = &entry.solution_argv {
		// Same layering as a direct build, so the two agree on the incremental salt.
		reset_rbxts_options(&mut referenced);
		let layers = if inherit_entry_type_and_rojo {
			merge_rbxts_options(coordinator_rbxts.cloned(), declared.clone())
		} else {
			declared.clone()
		};
		apply_rbxts_options(
			&mut referenced,
			merge_rbxts_options(layers, Some(argv.clone())).as_ref(),
		);
	} else {
		apply_rbxts_options(&mut referenced, declared.as_ref());
	}

	// Type and Rojo config come from the project's own rbxts key; only a coordinator root hands
	// the entry's down, and only to a project that declares no rbxts key at all.
	let declared = match declared {
		Some(declared) => declared,
		None if inherit_entry_type_and_rojo => return Ok(referenced),
		None => RbxtsOptions::default(),
	};
	referenced.project_type = None;
	referenced.rojo_config_path = declared.rojo.clone().unwrap_or_default();
	if let Some(project_type) = &declared.project_type {
		referenced.project_type = parse_project_type(project_type);
	}
	Ok(referenced)
}

/// The `type` option's spelling as a [`ProjectType`].
#[must_use]
pub fn parse_project_type(text: &str) -> Option<ProjectType> {
	match text {
		"game" => Some(ProjectType::Game),
		"model" => Some(ProjectType::Model),
		"package" => Some(ProjectType::Package),
		_ => None,
	}
}

/// `resetRbxtsOptions`: the rbxts fields other than type and rojo back to
/// `DEFAULT_PROJECT_OPTIONS`.
pub(crate) fn reset_rbxts_options(options: &mut ProjectOptions) {
	options.include_path = String::new();
	options.emit_include_files = true;
	options.log_truthy_changes = false;
	options.allow_comment_directives = false;
	options.optimized_array_appends = false;
	options.native_types = false;
	options.luau_types = false;
	options.no_optimized_loops = false;
	options.lua_extension = false;
}

/// `applyRbxtsOptions`: layers the rbxts fields other than type and rojo; unset fields keep the
/// earlier value.
pub(crate) fn apply_rbxts_options(options: &mut ProjectOptions, layer: Option<&RbxtsOptions>) {
	let Some(layer) = layer else {
		return;
	};
	if let Some(include_path) = &layer.include_path {
		options.include_path.clone_from(include_path);
	}
	if let Some(value) = layer.log_truthy_changes {
		options.log_truthy_changes = value;
	}
	if let Some(value) = layer.allow_comment_directives {
		options.allow_comment_directives = value;
	}
	if let Some(value) = layer.no_include {
		options.emit_include_files = !value;
	}
	if let Some(value) = layer.optimized_array_appends {
		options.optimized_array_appends = value;
	}
	if let Some(value) = layer.native_types {
		options.native_types = value;
	}
	if let Some(value) = layer.luau_types {
		options.luau_types = value;
	}
	if let Some(value) = layer.optimized_loops {
		options.no_optimized_loops = !value;
	}
	if let Some(value) = layer.luau {
		options.lua_extension = !value;
	}
}

/// What [`compile_project`] produced: project-relative slash-separated output paths to rendered
/// Luau.
pub type Outputs = BTreeMap<String, Vec<u8>>;

/// `CompileProject`: compiles every file of the project at `project_dir`.
///
/// Must run on a large stack ([`rotor_tsrs::with_large_stack`]).
///
/// # Errors
///
/// Any diagnostic fails the compile.
pub fn compile_project(project_dir: &str) -> Result<Outputs, Failure> {
	compile_project_with_options(project_dir, &ProjectOptions::default())
}

/// `CompileProjectWithOptions`: [`compile_project`] with the CLI knobs. The include copy runs at
/// upstream's point: after the program builds, before any validation or per-file diagnostics.
///
/// # Errors
///
/// See [`compile_project`].
pub fn compile_project_with_options(project_dir: &str, opts: &ProjectOptions) -> Result<Outputs, Failure> {
	let (dir, program) = new_project_program_with_options(project_dir, &opts.ts_config_path, opts)?;
	maybe_copy_include(&dir, opts).map_err(Failure::bare)?;
	compile_project_program(&dir, program, opts)
}

/// `compileProjectProgram`
pub(crate) fn compile_project_program(
	dir: &str,
	program: &'static Program,
	opts: &ProjectOptions,
) -> Result<Outputs, Failure> {
	let source_files = project_source_files(program);
	let pipeline = prepare_compile_pipeline(dir, program, &source_files, &opts.overlays, opts)?;
	let program = pipeline.prepared.program;
	let source_files = pipeline.prepared.source_files;
	let mut pctx = new_project_context(dir, program, opts)?;
	pctx.source_traces = pipeline.prepared.source_traces;
	let (outputs, _) = compile_project_source_files(dir, program, &pctx, &source_files, opts)?;
	Ok(outputs)
}

/// `projectSourceFiles`: not declarations, not from external libraries or project references,
/// and `.ts`/`.tsx`.
#[must_use]
pub fn project_source_files(program: &Program) -> Vec<P<SourceFile>> {
	rotor_tsrs::project::project_source_files(program)
}

/// `checkerSourceFileGroup`: the files one checker checked, with their indices.
#[derive(Default)]
struct CheckerSourceFileGroup {
	indices: Vec<usize>,
	files: Vec<P<SourceFile>>,
}

/// `compiledProjectSourceFile`
#[derive(Default)]
pub(crate) struct CompiledProjectSourceFile {
	pub(crate) rel_out: String,
	pub(crate) text: Vec<u8>,
	pub(crate) source_map: String,
	pub(crate) diags: Vec<DiagnosticInfo>,
	pub(crate) err: Option<Error>,
	/// The transformer ran to completion and returned a verdict, diagnostics included.
	pub(crate) transformed: bool,
}

/// `precheckedProjectSourceFile`
#[derive(Default)]
pub(crate) struct PrecheckedProjectSourceFile {
	pub(crate) ts_diags: Vec<P<ast::Diagnostic>>,
	pub(crate) comment_diags: Vec<String>,
}

/// Source maps by `<relOut>.map`.
pub(crate) type SourceMaps = BTreeMap<String, String>;

/// Runs `work(index)` for each of `count` groups: on scoped large-stack threads, or inline when
/// `single_threaded` (Go's `core.NewWorkGroup(singleThreaded)`). Results come back in group order.
pub(crate) fn run_work_group<T: Send>(single_threaded: bool, count: usize, work: impl Fn(usize) -> T + Sync) -> Vec<T> {
	if single_threaded || count <= 1 {
		return (0..count).map(&work).collect();
	}
	std::thread::scope(|scope| {
		let work = &work;
		// Every thread is spawned before the first join, so the groups run concurrently.
		#[expect(clippy::needless_collect, reason = "the threads must all start before any is joined")]
		let handles: Vec<_> = (0..count)
			.map(|index| {
				std::thread::Builder::new()
					.name("rotor-compile".to_owned())
					.stack_size(CHECKER_STACK_BYTES)
					.spawn_scoped(scope, move || work(index))
			})
			.collect();
		handles
			.into_iter()
			.enumerate()
			.map(|(index, handle)| {
				// Spawning failed: run the group here instead.
				handle.map_or_else(
					|_| work(index),
					|handle| {
						handle
							.join()
							.unwrap_or_else(|payload| std::panic::resume_unwind(payload))
					},
				)
			})
			.collect()
	})
}

/// `compileProjectSourceFiles`: the four compile gates, then the transform of every file, one
/// `MultiState` per checker group, and the drain in file order.
#[expect(clippy::too_many_lines, reason = "one function in Go")]
pub(crate) fn compile_project_source_files(
	dir: &str,
	program: &'static Program,
	pctx: &ProjectContext,
	source_files: &[P<SourceFile>],
	opts: &ProjectOptions,
) -> Result<(Outputs, SourceMaps), Failure> {
	let context = Context::default();
	// A collector turns census mode on; none is stock.
	let census = opts.census.as_deref();
	if let Some(census) = census {
		census.set_traces(pctx.source_traces.clone());
	}

	let stop_diagnostics = crate::timings::start_stage(opts.timings(), Stage::SemanticDiagnostics);
	// Gate 1 of 4: program-level option diagnostics.
	let ts_diags = program.get_program_diagnostics();
	if !ts_diags.is_empty() {
		let infos = ts_diagnostic_infos(&ts_diags, &pctx.source_traces);
		match census {
			None => {
				stop_diagnostics.stop();
				return Err(Failure {
					diagnostics: infos,
					error: Error::message("compile: TypeScript diagnostics"),
				});
			}
			Some(census) => census.add_project_diagnostics(infos),
		}
	}

	// compileFiles.ts L102 — note the TWO dots.
	logservice::write_line_if_verbose(&[&format!("compiling as {}..", pctx.project_type.as_str())]);

	let progress_labels = compile_project_progress_labels(source_files);

	// Go's ForEachFileOnFreeChecker balances only derived programs; a project without
	// transformers never is one, so the prechecks run per checker group.
	let groups = group_source_files_by_checker(&context, program, source_files);
	let single_threaded = program.single_threaded() || groups.len() <= 1;
	let group_prechecks = run_work_group(single_threaded, groups.len(), |group_index| {
		let group = &groups[group_index];
		let context = Context::default();
		let mut prechecks = Vec::with_capacity(group.files.len());
		for &file in &group.files {
			// A checker that saw a cancelled context panics on its next use.
			if opts.cancelled() {
				break;
			}
			prechecks.push(precheck_project_source_file(&context, program, file, opts));
		}
		prechecks
	});
	let mut prechecks: Vec<PrecheckedProjectSourceFile> = Vec::with_capacity(source_files.len());
	prechecks.resize_with(source_files.len(), PrecheckedProjectSourceFile::default);
	for (group, group_prechecks) in groups.iter().zip(group_prechecks) {
		for (&index, precheck) in group.indices.iter().zip(group_prechecks) {
			prechecks[index] = precheck;
		}
	}
	if opts.cancelled() {
		stop_diagnostics.stop();
		return Err(Failure::bare(Error::message("compile: context canceled")));
	}

	// Gate 2 of 4: any file with type errors stops the build before the transform. Every file has
	// been checked, so report all of their errors, as upstream does.
	if census.is_none() {
		let mut ts_diags = Vec::new();
		let mut comment_diags = Vec::new();
		for precheck in &prechecks {
			ts_diags.extend(ts_diagnostic_infos(&precheck.ts_diags, &pctx.source_traces));
			comment_diags.extend(string_diagnostics(precheck.comment_diags.clone()));
		}
		if !ts_diags.is_empty() {
			stop_diagnostics.stop();
			return Err(Failure {
				diagnostics: ts_diags,
				error: Error::message("compile: TypeScript diagnostics"),
			});
		}
		if !comment_diags.is_empty() {
			stop_diagnostics.stop();
			return Err(Failure {
				diagnostics: comment_diags,
				error: Error::message("compile: comment directive diagnostics"),
			});
		}
	}

	// Gate 3 of 4.
	let ts_diags = program.get_global_diagnostics(&context);
	if !ts_diags.is_empty() {
		let infos = ts_diagnostic_infos(&ts_diags, &pctx.source_traces);
		match census {
			None => {
				stop_diagnostics.stop();
				return Err(Failure {
					diagnostics: infos,
					error: Error::message("compile: TypeScript diagnostics"),
				});
			}
			Some(census) => census.add_project_diagnostics(infos),
		}
	}
	stop_diagnostics.stop();

	let stop_transform = crate::timings::start_stage(opts.timings(), Stage::NativeTransformRender);
	if !source_files.is_empty()
		&& let Some(timings) = opts.timings()
	{
		timings.mark_project_transformed();
	}
	let group_results = run_work_group(single_threaded, groups.len(), |group_index| {
		let group = &groups[group_index];
		let mut multi = MultiState::new();
		group
			.files
			.iter()
			.zip(&group.indices)
			.map(|(&file, &index)| {
				compile_project_source_file(dir, program, pctx, file, opts, &mut multi, &progress_labels[index])
			})
			.collect::<Vec<_>>()
	});
	let mut results: Vec<CompiledProjectSourceFile> = Vec::with_capacity(source_files.len());
	results.resize_with(source_files.len(), CompiledProjectSourceFile::default);
	for (group, group_results) in groups.iter().zip(group_results) {
		for (&index, result) in group.indices.iter().zip(group_results) {
			results[index] = result;
		}
	}
	stop_transform.stop();

	// Gate 4 of 4: the transform drain, in file order.
	let mut outputs = Outputs::new();
	let mut source_maps = SourceMaps::new();
	for (index, result) in results.into_iter().enumerate() {
		if let Some(census) = census {
			census.record(source_files[index], &prechecks[index], &result);
			// Only a file that transformed cleanly has output worth keeping.
			if !result.transformed || result.err.is_some() || !prechecks[index].ts_diags.is_empty() {
				continue;
			}
		} else if let Some(error) = result.err {
			return Err(Failure {
				diagnostics: result.diags,
				error,
			});
		}
		if !result.source_map.is_empty() {
			source_maps.insert(format!("{}.map", result.rel_out), result.source_map);
		}
		outputs.insert(result.rel_out, result.text);
	}
	Ok((outputs, source_maps))
}

/// `compileProjectProgressLabels`: the verbose `i/n compile <path>` labels.
fn compile_project_progress_labels(source_files: &[P<SourceFile>]) -> Vec<String> {
	if !logservice::verbose() {
		return vec![String::new(); source_files.len()];
	}
	let total = source_files.len();
	let width = format!("{total}/{total}").len();
	let cwd = std::env::current_dir()
		.ok()
		.map(|cwd| cwd.to_string_lossy().into_owned());
	source_files
		.iter()
		.enumerate()
		.map(|(index, file)| {
			let progress = format!("{:>width$}", format!("{}/{total}", index + 1));
			let mut rel_name = gopath::from_slash(file.file_name()).into_owned();
			if let Some(cwd) = &cwd
				&& let Ok(rel) = gopath::rel(cwd, &rel_name)
			{
				rel_name = rel;
			}
			format!("{progress} compile {rel_name}")
		})
		.collect()
}

/// `groupSourceFilesByChecker`: files grouped by the checker that checks them, groups in order of
/// their first file.
fn group_source_files_by_checker(
	context: &Context,
	program: &'static Program,
	source_files: &[P<SourceFile>],
) -> Vec<CheckerSourceFileGroup> {
	let mut groups_by_checker: HashMap<u32, usize> = HashMap::new();
	let mut groups: Vec<CheckerSourceFileGroup> = Vec::new();
	for (index, &file) in source_files.iter().enumerate() {
		let key = {
			let checker = program.get_type_checker_for_file_exclusive(context, file);
			checker.id
		};
		let group_index = *groups_by_checker.entry(key).or_insert_with(|| {
			groups.push(CheckerSourceFileGroup::default());
			groups.len() - 1
		});
		groups[group_index].indices.push(index);
		groups[group_index].files.push(file);
	}
	groups
}

/// The number of checker groups [`group_source_files_by_checker`] forms, for the tests.
#[cfg(test)]
pub(crate) fn group_source_files_for_test(program: &'static Program, source_files: &[P<SourceFile>]) -> usize {
	group_source_files_by_checker(&Context::default(), program, source_files).len()
}

/// `precheckProjectSourceFile`
fn precheck_project_source_file(
	context: &Context,
	program: &'static Program,
	source_file: P<SourceFile>,
	opts: &ProjectOptions,
) -> PrecheckedProjectSourceFile {
	let ts_diags = pre_emit_project_file_diagnostics_with_options(context, program, source_file, opts);
	let comment_diags = if ts_diags.is_empty() && !opts.allow_comment_directives {
		comment_directive_diagnostics(source_file)
	} else {
		Vec::new()
	};
	PrecheckedProjectSourceFile {
		ts_diags,
		comment_diags,
	}
}

/// `compileProjectSourceFile`: one file's transform and render on its checker, with the group's
/// `MultiState`.
#[expect(
	clippy::significant_drop_tightening,
	reason = "the state borrows the checker until the file is rendered"
)]
fn compile_project_source_file(
	dir: &str,
	program: &'static Program,
	pctx: &ProjectContext,
	source_file: P<SourceFile>,
	opts: &ProjectOptions,
	multi: &mut MultiState,
	progress_label: &str,
) -> CompiledProjectSourceFile {
	logservice::benchmark_if_verbose(progress_label, || {
		let mut result = CompiledProjectSourceFile::default();
		let context = Context::default();
		let mut checker = program.get_type_checker_for_file(&context, source_file);
		let options = StateOptions {
			rojo: Some(&pctx.rojo_context),
			project_type: Some(pctx.project_type),
			env: Some(&pctx.env),
			files: Some(&*pctx.files),
			log_truthy_changes: opts.log_truthy_changes,
			optimized_array_appends: opts.optimized_array_appends,
			native_types: pctx.native_types,
			luau_types: pctx.luau_types,
			no_optimized_loops: opts.no_optimized_loops,
			skip_semantic_diagnostics: opts.skip_semantic_diagnostics,
		};
		let mut state = State::new(program, &mut checker, source_file, multi, options);
		// Macro registration audit (digest §6): the pass's first state built the MacroManager.
		let missing = state.multi.macro_registration_failures();
		if !missing.is_empty() {
			result.diags = string_diagnostics(missing);
			result.err = Some(Error::message("compile: macro registration failure"));
			return result;
		}

		let source_map_file = program.options().source_map.is_true().then_some(source_file);
		let (text, source_map, diags) = match transform_and_render_source_map_detailed(&mut state, source_map_file) {
			Ok(rendered) => rendered,
			Err(failure) => {
				result.err = Some(failure.error);
				return result;
			}
		};
		drop(state);
		result.transformed = true;
		if !diags.is_empty() {
			// Node-located diagnostics index the reprinted text the way TypeScript's do.
			result.diags = pctx.source_traces.remap_all(diags, source_file.text());
			result.err = Some(Error::message("compile: transformer diagnostics"));
			return result;
		}

		let out_path = pctx
			.rojo_context
			.path_translator
			.get_output_path(source_file.file_name());
		let rel_out = gopath::rel(&gopath::from_slash(dir), &out_path).unwrap_or(out_path);
		result.rel_out = gopath::to_slash(&rel_out).into_owned();
		result.text = text;
		result.source_map = source_map;
		result
	})
}

/// The part of a program output path translation reads (`projectOutputHost`).
pub trait ProjectOutputHost {
	/// The compiler options.
	fn options(&self) -> P<rotor_tsrs::CompilerOptions>;
	/// The common source directory.
	fn common_source_directory(&self) -> String;
}

impl ProjectOutputHost for Program {
	fn options(&self) -> P<rotor_tsrs::CompilerOptions> {
		Self::options(self)
	}

	fn common_source_directory(&self) -> String {
		Self::common_source_directory(self).to_owned()
	}
}

/// `createPathTranslator` (`createPathTranslator.ts`): the root is the common ancestor of the
/// common source directory and the configured root dirs.
#[must_use]
pub fn create_path_translator(program: &dyn ProjectOutputHost, use_luau_extension: bool) -> PathTranslator {
	let options = program.options();
	let mut dirs = vec![program.common_source_directory()];
	dirs.extend(get_root_dirs_of(&options).unwrap_or_default());
	let root_dir = find_ancestor_dir(&dirs);
	let out_dir = gopath::from_slash(&options.out_dir).into_owned();
	let current_directory = if options.config_file_path.is_empty() {
		root_dir.clone()
	} else {
		gopath::dir(&gopath::from_slash(&options.config_file_path))
	};
	let build_info_output_path = outputpaths::get_build_info_file_name(
		&options,
		&tspath::ComparePathsOptions {
			current_directory,
			use_case_sensitive_file_names: rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names(),
		},
	);
	PathTranslator::new(
		&root_dir,
		&out_dir,
		&gopath::from_slash(&build_info_output_path),
		options.declaration.is_true(),
		use_luau_extension,
	)
}

/// `getRootDirs` (`getRootDirs.ts`): `rootDir` if set, else `rootDirs`. Validation already
/// rejected configs with neither, so the error is upstream's unreachable assert.
pub(crate) fn get_root_dirs(program: &dyn ProjectOutputHost) -> Result<Vec<String>, Error> {
	get_root_dirs_of(&program.options())
}

fn get_root_dirs_of(options: &rotor_tsrs::CompilerOptions) -> Result<Vec<String>, Error> {
	if !options.root_dir.is_empty() {
		return Ok(vec![options.root_dir.clone()]);
	}
	if let Some(root_dirs) = &options.root_dirs {
		return Ok(root_dirs.clone());
	}
	Err(Error::message(
		"compile: getRootDirs: neither rootDir nor rootDirs is set",
	))
}

/// `findAncestorDir` (`findAncestorDir.ts`): the deepest directory containing every input.
#[must_use]
pub fn find_ancestor_dir(dirs: &[String]) -> String {
	let separator = gopath::NATIVE.separator().to_string();
	let normalized: Vec<String> = dirs
		.iter()
		.map(|dir| {
			let mut dir = gopath::clean(&gopath::from_slash(dir));
			if !dir.ends_with(&separator) {
				dir.push_str(&separator);
			}
			dir
		})
		.collect();
	let Some(mut current_dir) = normalized.first().cloned() else {
		return ".".to_owned();
	};
	while !normalized.iter().all(|dir| dir.starts_with(&current_dir)) {
		let parent = gopath::join(&[&current_dir, ".."]) + &separator;
		if parent == current_dir {
			break;
		}
		current_dir = parent;
	}
	gopath::clean(&current_dir)
}

/// `findPackageJSON`: walks up from `dir` (`ts.findPackageJson`); `None` when no ancestor has one.
pub(crate) fn find_package_json(dir: &str) -> Option<String> {
	let mut current = gopath::clean(&gopath::from_slash(dir));
	loop {
		let candidate = gopath::join(&[&current, "package.json"]);
		if std::fs::metadata(&candidate).is_ok_and(|metadata| metadata.is_file()) {
			return Some(candidate);
		}
		let parent = gopath::dir(&current);
		if parent == current {
			return None;
		}
		current = parent;
	}
}

/// `checkFileName` (`checkFileName.ts`): the `incorrectFileName` message, if any.
pub(crate) fn check_file_name(file_path: &str) -> Option<String> {
	let base_name = gopath::base(file_path);
	let suggestion = filename_warning(&base_name)?;
	Some(rotor_transformer::diagnostics::incorrect_file_name(&base_name, &suggestion, file_path).message)
}

/// `checkRojoConfig` (`checkRojoConfig.ts`): a Rojo `$path` partition inside a TypeScript root
/// dir means the user mapped `src` instead of `out`.
pub(crate) fn check_rojo_config(
	rojo_config_path: Option<&str>,
	resolver: &RojoResolver,
	root_dirs: &[String],
	path_translator: &PathTranslator,
) -> Vec<String> {
	let Some(rojo_config_path) = rojo_config_path else {
		return Vec::new();
	};
	let mut messages = Vec::new();
	let rojo_config_dir = gopath::dir(rojo_config_path);
	for partition in resolver.partitions() {
		for root_dir in root_dirs {
			if is_path_descendant_of(&partition.fs_path, &gopath::from_slash(root_dir)) {
				let out_path = path_translator.get_output_path(&partition.fs_path);
				let input_path = rel_or_self(&rojo_config_dir, &partition.fs_path);
				let suggested_path = rel_or_self(&rojo_config_dir, &out_path);
				messages.push(rotor_transformer::diagnostics::rojo_path_in_src(&input_path, &suggested_path).message);
			}
		}
	}
	messages
}

/// `isPathDescendantOf` (`isPathDescendantOf.ts`).
#[must_use]
pub fn is_path_descendant_of(file_path: &str, dir_path: &str) -> bool {
	if dir_path == file_path {
		return true;
	}
	gopath::rel(dir_path, file_path).is_ok_and(|rel| !rel.starts_with(".."))
}

/// `relOrSelf`
fn rel_or_self(base: &str, target: &str) -> String {
	gopath::rel(base, target).unwrap_or_else(|_| target.to_owned())
}

/// Locks a mutex, recovering from poisoning as Go's mutexes have none.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
