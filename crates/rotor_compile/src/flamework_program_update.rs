//! `flamework_program_update.go`: programs derived from a project's program by overlaying the
//! source a native stage (Flamework, jecs) or the sidecar printed, and the printing itself.

use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::{Arc, Mutex};

use rotor_support::{fsutil, gopath};
use rotor_tsrs::P;
use rotor_tsrs::ast::{SourceFile, SourceFileParseOptions};
use rotor_tsrs::compiler::{CompilerHost, Program};
use rotor_tsrs::contentmapper;
use rotor_tsrs::printer::{EmitContext, PrintHandlers, PrinterOptions, new_printer, new_text_writer};
use rotor_tsrs::tsoptions::ParsedCommandLine;
use rotor_tsrs::tspath::{ComparePathsOptions, Path};
use rotor_tsrs::vfs::{FS, wrapvfs};

use crate::compile::Error;
use crate::incremental_manifest::normalize_source_file_path;
use crate::overlayfs::{canonical_existing_overlay_path, overlay_aliases, overlay_text};
use crate::project::lock;
use crate::sidecar::{normalize_overlay_path, remap_program_source_files};
use crate::source_trace::{DiagnosticTraces, SourceTraceMap, compose_source_trace_maps};
use crate::transformer_host::TransformerRebase;

/// `nativeSourceOverlay`: one file a native stage printed.
#[derive(Clone, Debug)]
pub(crate) struct NativeSourceOverlay {
	pub(crate) file_name: String,
	pub(crate) text: String,
	pub(crate) trace: Option<Arc<SourceTraceMap>>,
	pub(crate) rebase: Option<Arc<TransformerRebase>>,
}

/// `nativeProgramUpdate`
pub(crate) struct NativeProgramUpdate<'a> {
	pub(crate) program: &'static Program,
	pub(crate) source_files: &'a [P<SourceFile>],
	pub(crate) traces: &'a DiagnosticTraces,
	pub(crate) overlays: Vec<NativeSourceOverlay>,
}

/// `derivedProgramCheckers`: `derivedProgramDefaultCheckers` (8), unless the project configured a
/// count or runs single-threaded, which it keeps (`None`, Go's 0).
pub(crate) fn derived_program_checkers(program: &Program) -> Option<i64> {
	rotor_tsrs::project::derived_program_checkers(program)
}

/// `updateProgramWithTextOverlays`: `program` with the texts of `overlays` (keyed by file name).
///
/// # Errors
///
/// An overlay names a file the program does not hold.
pub(crate) fn update_program_with_text_overlays(
	program: &'static Program,
	overlays: &BTreeMap<String, String>,
) -> Result<(&'static Program, bool), Error> {
	if overlays.is_empty() {
		return Ok((program, true));
	}
	let case_sensitive = program.use_case_sensitive_file_names();
	let order: HashMap<String, usize> = program
		.source_files()
		.iter()
		.enumerate()
		.map(|(index, file)| (normalize_overlay_path(file.file_name(), case_sensitive), index))
		.collect();
	let mut paths: Vec<&String> = overlays.keys().collect();
	// Go ranges over its map before a stable sort, so overlays the program lacks (which sort
	// first) and paths of one file keep map order; a missing path fails either way.
	paths.sort_by_key(|path| {
		order
			.get(&normalize_overlay_path(path, case_sensitive))
			.copied()
			.unwrap_or(0)
	});

	let mut changed = Vec::with_capacity(paths.len());
	let mut host_overlays = HashMap::with_capacity(overlays.len());
	for path in paths {
		let Some(original) = program.get_source_file(path) else {
			return Err(Error::message(format!(
				"compile: overlay source missing from Program: {path}"
			)));
		};
		let text = &overlays[path];
		host_overlays.insert(normalize_overlay_path(path, case_sensitive), text.clone());
		if original.text() == text {
			continue;
		}
		changed.push(original.path().clone());
	}
	if changed.is_empty() {
		return Ok((program, true));
	}
	Ok(update_program_files_with_overlays(program, host_overlays, &changed))
}

/// `updateNativeFlameworkProgram`: the program with the overlays' texts, the source files remapped
/// into it and the traces composed with the overlays'.
///
/// # Errors
///
/// An overlay names a file the program does not hold, or a source file is missing from the
/// updated program.
pub(crate) fn update_native_flamework_program(
	input: NativeProgramUpdate<'_>,
) -> Result<(&'static Program, Vec<P<SourceFile>>, DiagnosticTraces), Error> {
	if input.overlays.is_empty() {
		return Ok((input.program, input.source_files.to_vec(), input.traces.clone()));
	}

	let case_sensitive = input.program.use_case_sensitive_file_names();
	let order: HashMap<String, usize> = input
		.source_files
		.iter()
		.enumerate()
		.map(|(index, file)| (normalize_overlay_path(file.file_name(), case_sensitive), index))
		.collect();
	let mut overlays = input.overlays;
	overlays.sort_by_key(|overlay| {
		order
			.get(&normalize_overlay_path(&overlay.file_name, case_sensitive))
			.copied()
			.unwrap_or(0)
	});

	let mut texts = HashMap::with_capacity(overlays.len());
	let mut changed = Vec::with_capacity(overlays.len());
	for overlay in &overlays {
		texts.insert(
			normalize_overlay_path(&overlay.file_name, case_sensitive),
			overlay.text.clone(),
		);
		let Some(original) = input.program.get_source_file(&overlay.file_name) else {
			return Err(Error::message(format!(
				"compile: native Flamework source missing from Program: {}",
				overlay.file_name
			)));
		};
		changed.push(original.path().clone());
	}
	let composed = compose_native_overlay_traces(input.program, input.traces, &overlays);

	let (program, _) = update_program_files_with_overlays(input.program, texts, &changed);

	let remapped = remap_program_source_files(program, input.source_files)?;
	Ok((program, remapped, composed))
}

/// The overlay lookup of a [`NativeOverlayHost`]: the lexical map, then the aliases.
#[derive(Clone)]
struct OverlayLookup {
	overlays: Arc<HashMap<String, String>>,
	aliases: Arc<HashMap<String, String>>,
	case_sensitive: bool,
}

impl OverlayLookup {
	fn read(&self, path: &str) -> Option<&str> {
		overlay_text(&self.overlays, &self.aliases, path, self.case_sensitive)
	}
}

/// `nativeOverlayHost`: the base host, with the overlay texts in place of the files they name.
pub(crate) struct NativeOverlayHost {
	base: Arc<dyn CompilerHost>,
	fs: wrapvfs::WrappedFS,
	lookup: OverlayLookup,
	pub(crate) preparsed: Option<Arc<PreparsedOverlays>>,
}

/// `newNativeOverlayHost`
pub(crate) fn new_native_overlay_host(
	base: Arc<dyn CompilerHost>,
	base_fs: &'static dyn FS,
	overlays: HashMap<String, String>,
) -> NativeOverlayHost {
	let case_sensitive = base_fs.use_case_sensitive_file_names();
	let ordered: BTreeMap<String, String> = overlays
		.iter()
		.map(|(path, text)| (path.clone(), text.clone()))
		.collect();
	let aliases = overlay_aliases(&ordered, "", case_sensitive);
	new_native_overlay_host_with_aliases(base, base_fs, overlays, aliases)
}

/// `newNativeOverlayHostWithAliases`
pub(crate) fn new_native_overlay_host_with_aliases(
	base: Arc<dyn CompilerHost>,
	base_fs: &'static dyn FS,
	overlays: HashMap<String, String>,
	aliases: HashMap<String, String>,
) -> NativeOverlayHost {
	let lookup = OverlayLookup {
		overlays: Arc::new(overlays),
		aliases: Arc::new(aliases),
		case_sensitive: base_fs.use_case_sensitive_file_names(),
	};
	let exists = lookup.clone();
	let reads = lookup.clone();
	let fs = wrapvfs::wrap(
		base_fs,
		wrapvfs::Replacements {
			file_exists: Some(Box::new(move |path: &str| {
				exists.read(path).is_some() || base_fs.file_exists(path)
			})),
			read_file: Some(Box::new(move |path: &str| {
				reads.read(path).map(str::to_owned).or_else(|| base_fs.read_file(path))
			})),
			..Default::default()
		},
	);
	NativeOverlayHost {
		base,
		fs,
		lookup,
		preparsed: None,
	}
}

impl CompilerHost for NativeOverlayHost {
	fn fs(&self) -> &dyn FS {
		&self.fs
	}

	fn default_library_path(&self) -> &str {
		self.base.default_library_path()
	}

	fn get_current_directory(&self) -> &str {
		self.base.get_current_directory()
	}

	fn trace(&self, msg: &'static rotor_tsrs::diagnostics::Message, args: &[&dyn fmt::Display]) {
		self.base.trace(msg, args);
	}

	fn get_source_file(&self, opts: SourceFileParseOptions) -> Option<P<SourceFile>> {
		let Some(text) = self.lookup.read(&opts.file_name) else {
			// Keep the underlying host's parse cache and source loading for files this overlay
			// does not change.
			return self.base.get_source_file(opts);
		};
		if let Some(preparsed) = &self.preparsed
			&& let Some(file) = preparsed.take(&opts, text)
		{
			return Some(file);
		}
		Some(parse_overlay(opts, text))
	}

	fn get_content_mapped_source_files(
		&self,
		parse_options: SourceFileParseOptions,
		mapper: &contentmapper::Mapper,
	) -> Result<contentmapper::SourceFiles, contentmapper::Error> {
		self.base.get_content_mapped_source_files(parse_options, mapper)
	}

	fn content_mapper_project(&self) -> Option<Arc<dyn contentmapper::Project>> {
		self.base.content_mapper_project()
	}

	fn get_resolved_project_reference(&self, file_name: &str, path: Path) -> Option<P<ParsedCommandLine>> {
		self.base.get_resolved_project_reference(file_name, path)
	}
}

/// `parser.ParseSourceFile` with the script kind of the file's name.
fn parse_overlay(opts: SourceFileParseOptions, text: &str) -> P<SourceFile> {
	let script_kind = rotor_tsrs::tscore::get_script_kind_from_file_name(&opts.file_name);
	rotor_tsrs::parser::parse_source_file(opts, text, script_kind)
}

/// `updateProgramFilesWithOverlays`: the changed overlay files applied in one
/// `update_program_files` call, which asks for each changed file in turn. Unless the program is
/// single-threaded, the overlay aliases resolve and the changed files parse first, in parallel,
/// and the host gives each parse to the first request with the same options and text. The update
/// still decides membership, order, reuse and rebuilds.
pub(crate) fn update_program_files_with_overlays(
	program: &'static Program,
	overlays: HashMap<String, String>,
	changed: &[Path],
) -> (&'static Program, bool) {
	#[cfg(test)]
	PROGRAM_UPDATES.with(|updates| updates.set(updates.get() + 1));
	let base = Arc::clone(program.host());
	let base_fs: &'static dyn FS = program.host().fs();
	let checkers = derived_program_checkers(program);
	if program.single_threaded() {
		let host = new_native_overlay_host(base, base_fs, overlays);
		#[cfg(test)]
		LAST_PREPARSE.with(|last| *last.borrow_mut() = Some(None));
		return rotor_tsrs::project::update_program_files(program, changed, Arc::new(host), checkers);
	}
	let case_sensitive = base_fs.use_case_sensitive_file_names();
	let aliases = parallel_overlay_aliases(&overlays, case_sensitive);
	let preparsed = preparse_overlays(program, &overlays, changed, case_sensitive).map(Arc::new);
	let mut host = new_native_overlay_host_with_aliases(base, base_fs, overlays, aliases);
	host.preparsed.clone_from(&preparsed);
	#[cfg(test)]
	LAST_PREPARSE.with(|last| *last.borrow_mut() = Some(preparsed.clone()));
	let updated = rotor_tsrs::project::update_program_files(program, changed, Arc::new(host), checkers);
	// The updated program keeps the host; once released it parses every request again, as a host
	// without preparsed files does.
	if let Some(preparsed) = preparsed {
		preparsed.release();
	}
	updated
}

/// The preparse an overlay host holds: `None` for a host without one.
#[cfg(test)]
pub(crate) type LastPreparse = Option<Arc<PreparsedOverlays>>;

#[cfg(test)]
thread_local! {
	/// The program reconstructions this thread asked for. Go's tests count them with a
	/// `CreateCheckerPool` hook the updated program inherits; a tsrs update takes its pool factory
	/// from the caller, so the port counts the updates instead.
	pub(crate) static PROGRAM_UPDATES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };

	/// The preparse of the overlay host the last update on this thread gave its program: `None`
	/// before any update, `Some(None)` for a host without one. Go's tests read it off the
	/// program's host, which tsrs hands out only as a trait object.
	pub(crate) static LAST_PREPARSE: std::cell::RefCell<Option<LastPreparse>> =
		const { std::cell::RefCell::new(None) };
}

/// A panic a parse raised, kept for the request that takes the parse.
type Panic = Box<dyn Any + Send>;

/// `overlayParse`
pub(crate) struct OverlayParse {
	pub(crate) opts: SourceFileParseOptions,
	pub(crate) text: String,
	pub(crate) file: Option<P<SourceFile>>,
	/// Go's `panicked` and `recovered`.
	pub(crate) panic: Option<Panic>,
}

impl OverlayParse {
	/// `parse`: the parse, or the panic it raised.
	pub(crate) fn parse(&mut self) {
		match catch_unwind(AssertUnwindSafe(|| parse_overlay(self.opts.clone(), &self.text))) {
			Ok(file) => self.file = Some(file),
			Err(panic) => self.panic = Some(panic),
		}
	}
}

/// `preparsedOverlays`: overlay parses made before the update asks for them. Only the first
/// request with the same parse options and overlay text takes a parse; every other request
/// parses as before.
pub(crate) struct PreparsedOverlays {
	pub(crate) files: Mutex<Option<HashMap<SourceFileParseOptions, OverlayParse>>>,
}

/// The stack each parse worker gets; the parser recurses as deeply as the source nests.
const PARSE_STACK_BYTES: usize = 256 << 20;

/// `preparseOverlays`: parses the changed overlay files the update can request: the changed
/// paths, in order, with the parse options of the program file at each path. It stops at the
/// first path where the update rebuilds the whole program before it asks for the file (a path the
/// program does not hold, or a package redirect). A request without a lexical overlay is left
/// alone. `None` with fewer than two parses.
pub(crate) fn preparse_overlays(
	program: &Program,
	overlays: &HashMap<String, String>,
	changed: &[Path],
	case_sensitive: bool,
) -> Option<PreparsedOverlays> {
	let mut jobs: Vec<OverlayParse> = Vec::with_capacity(changed.len());
	let mut seen = std::collections::HashSet::with_capacity(changed.len());
	for path in changed {
		let Some(old_file) = program.get_source_file_by_path(path) else {
			break;
		};
		// A redirect path maps to its target file, whose path differs.
		if old_file.path() != path || !program.get_redirect_targets(path).is_empty() {
			break;
		}
		let opts = old_file.parse_options().clone();
		let Some(text) = overlays.get(&normalize_overlay_path(&opts.file_name, case_sensitive)) else {
			continue;
		};
		if !seen.insert(opts.clone()) {
			continue;
		}
		jobs.push(OverlayParse {
			opts,
			text: text.clone(),
			file: None,
			panic: None,
		});
	}
	if jobs.len() < 2 {
		return None;
	}
	let workers = std::thread::available_parallelism()
		.map_or(1, std::num::NonZero::get)
		.min(jobs.len());
	let slots: Vec<Mutex<&mut OverlayParse>> = jobs.iter_mut().map(Mutex::new).collect();
	let next = std::sync::atomic::AtomicUsize::new(0);
	std::thread::scope(|scope| {
		for _ in 0..workers {
			// In a long-lived session the parses go to a region the build's program scope frees.
			let region = rotor_tsrs::project::scope_thread_region();
			let (next, slots) = (&next, &slots);
			let spawned = std::thread::Builder::new()
				.stack_size(PARSE_STACK_BYTES)
				.spawn_scoped(scope, move || {
					let _region = region.as_ref().map(rotor_tsrs::tscore::arena::Region::enter);
					loop {
						let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
						let Some(slot) = slots.get(index) else {
							break;
						};
						lock(slot).parse();
					}
				});
			if let Err(error) = spawned {
				resume_unwind(Box::new(error));
			}
		}
	});
	drop(slots);
	let files = jobs.into_iter().map(|job| (job.opts.clone(), job)).collect();
	Some(PreparsedOverlays {
		files: Mutex::new(Some(files)),
	})
}

impl PreparsedOverlays {
	/// `take`: the parse for `opts` when its text is `text`. A kept panic is raised again here, on
	/// the requesting thread.
	pub(crate) fn take(&self, opts: &SourceFileParseOptions, text: &str) -> Option<P<SourceFile>> {
		let parsed = lock(&self.files).as_mut()?.remove(opts)?;
		if parsed.text != text {
			return None;
		}
		if let Some(panic) = parsed.panic {
			resume_unwind(panic);
		}
		parsed.file
	}

	/// `release`
	pub(crate) fn release(&self) {
		*lock(&self.files) = None;
	}
}

/// `parallelOverlayAliases`: `overlay_aliases(overlays, "", case_sensitive)` with the realpath
/// lookups run in parallel. The insertion runs in one order afterwards, so a colliding alias keeps
/// the text the serial loop leaves in it. Go ranges over its map in both; the port takes the paths
/// in order, as its `overlay_aliases` does.
pub(crate) fn parallel_overlay_aliases(
	overlays: &HashMap<String, String>,
	case_sensitive: bool,
) -> HashMap<String, String> {
	let ordered: BTreeMap<String, String> = overlays
		.iter()
		.map(|(path, text)| (path.clone(), text.clone()))
		.collect();
	if ordered.len() <= 1 {
		return overlay_aliases(&ordered, "", case_sensitive);
	}
	let mut entries: Vec<OverlayAliasEntry> = ordered
		.into_iter()
		.map(|(path, text)| OverlayAliasEntry {
			path,
			text,
			physical: None,
			panic: None,
		})
		.collect();
	let config_dir = gopath::dir(&gopath::from_slash(""));
	let canonical_config_dir = fsutil::eval_symlinks(&config_dir).ok();
	// Workers stride over the entries, so no more realpath calls block at once than there are
	// threads.
	let workers = std::thread::available_parallelism()
		.map_or(1, std::num::NonZero::get)
		.min(entries.len());
	let slots: Vec<Mutex<&mut OverlayAliasEntry>> = entries.iter_mut().map(Mutex::new).collect();
	std::thread::scope(|scope| {
		for worker in 0..workers {
			let slots = &slots;
			scope.spawn(move || {
				for slot in slots.iter().skip(worker).step_by(workers) {
					lock(slot).resolve(case_sensitive);
				}
			});
		}
	});
	drop(slots);
	merge_overlay_aliases(entries, &config_dir, canonical_config_dir.as_deref(), case_sensitive)
}

/// `overlayAliasEntry`: one overlay and its physical path, once resolved.
pub(crate) struct OverlayAliasEntry {
	pub(crate) path: String,
	pub(crate) text: String,
	/// Go's `physical` when `resolved`.
	pub(crate) physical: Option<String>,
	/// Go's `panicked` and `recovered`.
	pub(crate) panic: Option<Panic>,
}

impl OverlayAliasEntry {
	/// `resolve`
	fn resolve(&mut self, case_sensitive: bool) {
		match catch_unwind(AssertUnwindSafe(|| {
			canonical_existing_overlay_path(&self.path, case_sensitive)
		})) {
			Ok(physical) => self.physical = physical,
			Err(panic) => self.panic = Some(panic),
		}
	}
}

/// `mergeOverlayAliases`: the insertion loop of `overlay_aliases` over resolved entries, in entry
/// order. A lookup panic is raised again at its entry. `canonical_config_dir` is `None` where Go
/// has `configErr`.
pub(crate) fn merge_overlay_aliases(
	entries: Vec<OverlayAliasEntry>,
	config_dir: &str,
	canonical_config_dir: Option<&str>,
	case_sensitive: bool,
) -> HashMap<String, String> {
	let mut aliases = HashMap::with_capacity(entries.len() * 2);
	for entry in entries {
		aliases.insert(normalize_overlay_path(&entry.path, case_sensitive), entry.text.clone());
		if let Some(panic) = entry.panic {
			resume_unwind(panic);
		}
		let Some(physical) = entry.physical else {
			continue;
		};
		aliases.insert(physical.clone(), entry.text.clone());
		let Some(canonical_config_dir) = canonical_config_dir else {
			continue;
		};
		if let Ok(relative) = gopath::rel(canonical_config_dir, &gopath::from_slash(&physical))
			&& relative != ".."
			&& !relative.starts_with(&format!("..{}", gopath::NATIVE.separator()))
		{
			aliases.insert(normalize_overlay_path(&relative, case_sensitive), entry.text.clone());
			aliases.insert(
				normalize_overlay_path(&gopath::join(&[config_dir, &relative]), case_sensitive),
				entry.text,
			);
		}
	}
	aliases
}

/// `composeNativeOverlayTraces`: `traces` plus each overlay's trace composed with the incoming
/// trace for its file. A repeated file keeps its last overlay's trace.
pub(crate) fn compose_native_overlay_traces(
	program: &Program,
	traces: &DiagnosticTraces,
	overlays: &[NativeSourceOverlay],
) -> DiagnosticTraces {
	let keys: Vec<String> = overlays
		.iter()
		.map(|overlay| normalize_source_file_path(&overlay.file_name))
		.collect();
	let compose =
		|index: usize| compose_source_trace_maps(overlays[index].trace.clone(), traces.0.get(&keys[index]).cloned());
	let results: Vec<Option<Arc<SourceTraceMap>>> = if program.single_threaded() || overlays.len() <= 1 {
		(0..overlays.len()).map(compose).collect()
	} else {
		let workers = std::thread::available_parallelism()
			.map_or(1, std::num::NonZero::get)
			.min(overlays.len());
		let chunk = overlays.len().div_ceil(workers);
		std::thread::scope(|scope| {
			// Every worker starts before the first join.
			#[expect(clippy::needless_collect, reason = "the workers must all spawn before any joins")]
			let handles: Vec<_> = (0..workers)
				.map(|worker| {
					let compose = &compose;
					scope.spawn(move || {
						(worker * chunk..((worker + 1) * chunk).min(overlays.len()))
							.map(compose)
							.collect::<Vec<_>>()
					})
				})
				.collect();
			handles
				.into_iter()
				.flat_map(|handle| handle.join().unwrap_or_else(|panic| resume_unwind(panic)))
				.collect()
		})
	};
	let mut composed = traces.clone();
	for (key, result) in keys.into_iter().zip(results) {
		match result {
			Some(trace) => {
				composed.0.insert(key, trace);
			}
			None => {
				composed.0.remove(&key);
			}
		}
	}
	composed
}

/// `printNativeSource`: a natively transformed file printed, with the trace map that takes its
/// positions back to `original_text`.
///
/// # Errors
///
/// The printer's source map does not decode.
pub(crate) fn print_native_source(
	source_file: P<SourceFile>,
	original: P<SourceFile>,
	emit_context: P<EmitContext>,
	original_file_name: &str,
	original_text: &str,
) -> Result<(String, SourceTraceMap), Error> {
	let mut writer = new_text_writer("\n", 0);
	let directory = gopath::dir(source_file.file_name());
	let mut generator = rotor_tsrs::sourcemap::new_generator(
		&gopath::base(source_file.file_name()),
		"",
		&directory,
		ComparePathsOptions {
			use_case_sensitive_file_names: true,
			current_directory: directory.clone(),
		},
	);
	new_printer(
		PrinterOptions {
			source_map: true,
			inline_sources: true,
			..PrinterOptions::default()
		},
		PrintHandlers::default(),
		Some(emit_context),
	)
	.write(
		source_file.as_node(),
		Some(original),
		&mut *writer,
		Some(&mut generator),
	);
	let raw = generator.raw_source_map();
	let trace = SourceTraceMap::from_mappings(i64::from(raw.version), &raw.mappings, original_file_name, original_text)
		.map_err(Error::Message)?;
	Ok((writer.string(), trace))
}
