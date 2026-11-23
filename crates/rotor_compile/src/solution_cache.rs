//! `solution_cache.go`: the drain-scoped parse cache of a solution.
//!
//! Every project's program parses the `.d.ts` and `.json` files the projects share once, through
//! a [`rotor_tsrs::project::SourceFileCache`]; ordinary sources stay project-local.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use rotor_support::gopath;
use rotor_tsrs::ast::{SourceFile, SourceFileParseOptions};
use rotor_tsrs::{P, tspath};

use crate::project::lock;
use crate::sidecar::normalize_overlay_path;

/// `declarationSourceFileParseOptions`
pub(crate) fn declaration_source_file_parse_options(file_name: &str) -> SourceFileParseOptions {
	let normalized = tspath::normalize_path(&gopath::to_slash(file_name));
	SourceFileParseOptions {
		path: tspath::to_path(
			&normalized,
			"",
			rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names(),
		),
		file_name: normalized,
		..SourceFileParseOptions::default()
	}
}

/// A parse another caller started; its waiters block until it lands.
#[derive(Default)]
struct InFlight {
	done: Mutex<bool>,
	landed: Condvar,
}

type PackageLayoutKey = (Vec<String>, Vec<String>, bool);
type PackageLayoutMap = HashMap<PackageLayoutKey, Arc<crate::package_layout::PackageLayout>>;

/// `solutionCompileCache`: one per drain.
#[derive(Default)]
pub struct SolutionCompileCache {
	sources: Mutex<HashMap<SourceFileParseOptions, P<SourceFile>>>,
	inflight: Mutex<HashMap<SourceFileParseOptions, Arc<InFlight>>>,
	hits: AtomicI64,
	misses: AtomicI64,
	/// Incremented whenever any project in the solution writes/cleans/copies outputs so downstream
	/// consumers rebuild [`PackageLayout`] against the updated disk state.
	write_generation: AtomicU64,
	package_layouts: Mutex<(u64, PackageLayoutMap)>,
	inflight_layouts: Mutex<HashMap<(u64, PackageLayoutKey), Arc<InFlight>>>,
	/// In a long-lived session, the drain's programs parse into these, so the shared parses are
	/// freed with the drain rather than kept in the thread arenas for good.
	regions: Option<rotor_tsrs::project::LoaderRegions>,
}

impl SolutionCompileCache {
	/// `newSolutionCompileCache`
	pub(crate) fn new() -> Self {
		Self {
			regions: rotor_tsrs::project::long_lived().then(rotor_tsrs::project::LoaderRegions::new),
			..Self::default()
		}
	}

	/// The cache's hit count.
	pub(crate) fn hits(&self) -> i64 {
		self.hits.load(Ordering::Relaxed)
	}

	/// The cache's miss count.
	pub(crate) fn misses(&self) -> i64 {
		self.misses.load(Ordering::Relaxed)
	}

	/// `cache.fs.ReadFile`: the OS file system's text of `path`.
	pub(crate) fn read_file(path: &str) -> Option<String> {
		rotor_tsrs::vfs::osvfs::fs().read_file(&gopath::to_slash(path))
	}

	/// `getSourceFile`: one parse per options, however many callers ask at once. A failed parse
	/// is not cached.
	pub(crate) fn get_source_file(
		&self,
		opts: &SourceFileParseOptions,
		parse: impl FnOnce(SourceFileParseOptions) -> Option<P<SourceFile>>,
	) -> Option<P<SourceFile>> {
		let cached = lock(&self.sources).get(opts).copied();
		if let Some(file) = cached {
			self.hits.fetch_add(1, Ordering::Relaxed);
			return Some(file);
		}
		let waiting = {
			let mut inflight = lock(&self.inflight);
			// A parse may have landed between the two locks.
			let landed = lock(&self.sources).get(opts).copied();
			if let Some(file) = landed {
				drop(inflight);
				self.hits.fetch_add(1, Ordering::Relaxed);
				return Some(file);
			}
			let waiting = inflight.get(opts).map(Arc::clone);
			if waiting.is_none() {
				inflight.insert(opts.clone(), Arc::new(InFlight::default()));
			}
			waiting
		};
		if let Some(waiting) = waiting {
			let mut done = lock(&waiting.done);
			while !*done {
				done = waiting
					.landed
					.wait(done)
					.unwrap_or_else(std::sync::PoisonError::into_inner);
			}
			drop(done);
			self.hits.fetch_add(1, Ordering::Relaxed);
			return lock(&self.sources).get(opts).copied();
		}
		let file = parse(opts.clone());
		if let Some(file) = file {
			lock(&self.sources).insert(opts.clone(), file);
		}
		let landed = lock(&self.inflight).remove(opts);
		if let Some(landed) = landed {
			*lock(&landed.done) = true;
			landed.landed.notify_all();
		}
		self.misses.fetch_add(1, Ordering::Relaxed);
		file
	}

	/// Records that a project in the solution modified outputs on disk, invalidating cached
	/// [`crate::package_layout::PackageLayout`] entries for subsequent consumers.
	pub(crate) fn note_project_wrote(&self) {
		self.write_generation.fetch_add(1, Ordering::AcqRel);
	}

	/// Returns a cached [`crate::package_layout::PackageLayout`] for `(type_roots,
	/// custom_conditions, case_sensitive)` at the current write generation, or builds it once.
	pub(crate) fn get_or_build_package_layout(
		&self,
		type_roots: Vec<String>,
		custom_conditions: Vec<String>,
		case_sensitive: bool,
		build: impl FnOnce() -> crate::package_layout::PackageLayout,
	) -> Arc<crate::package_layout::PackageLayout> {
		let key = (type_roots, custom_conditions, case_sensitive);
		let generation = self.write_generation.load(Ordering::Acquire);
		{
			let layouts = lock(&self.package_layouts);
			if layouts.0 == generation
				&& let Some(layout) = layouts.1.get(&key)
			{
				return Arc::clone(layout);
			}
		}
		let inflight_key = (generation, key.clone());
		let waiting = {
			let mut inflight = lock(&self.inflight_layouts);
			let layouts = lock(&self.package_layouts);
			if layouts.0 == generation
				&& let Some(layout) = layouts.1.get(&key)
			{
				drop(inflight);
				return Arc::clone(layout);
			}
			drop(layouts);
			let waiting = inflight.get(&inflight_key).map(Arc::clone);
			if waiting.is_none() {
				inflight.insert(inflight_key.clone(), Arc::new(InFlight::default()));
			}
			waiting
		};
		if let Some(waiting) = waiting {
			let mut done = lock(&waiting.done);
			while !*done {
				done = waiting
					.landed
					.wait(done)
					.unwrap_or_else(std::sync::PoisonError::into_inner);
			}
			drop(done);
			if self.write_generation.load(Ordering::Acquire) == generation {
				let layouts = lock(&self.package_layouts);
				if layouts.0 == generation
					&& let Some(layout) = layouts.1.get(&key)
				{
					return Arc::clone(layout);
				}
			}
			return Arc::new(build());
		}
		let layout = Arc::new(build());
		if self.write_generation.load(Ordering::Acquire) == generation {
			let mut layouts = lock(&self.package_layouts);
			if layouts.0 != generation {
				layouts.0 = generation;
				layouts.1.clear();
			}
			if self.write_generation.load(Ordering::Acquire) == generation {
				layouts.1.insert(key, Arc::clone(&layout));
			}
		}
		let landed = lock(&self.inflight_layouts).remove(&inflight_key);
		if let Some(landed) = landed {
			*lock(&landed.done) = true;
			landed.landed.notify_all();
		}
		layout
	}
}

/// `cachingCompilerHost`'s policy for one project: declarations and JSON files go through the
/// shared cache, except the project's synthetic declarations and overlaid files.
pub(crate) struct ProjectParseCache {
	cache: Arc<SolutionCompileCache>,
	skip: HashSet<String>,
	case_sensitive: bool,
}

impl ProjectParseCache {
	/// `wrap`: `skip` names the files that stay project-local.
	pub(crate) fn new(cache: Arc<SolutionCompileCache>, skip: impl IntoIterator<Item = String>) -> Self {
		let case_sensitive = rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names();
		Self {
			cache,
			skip: skip
				.into_iter()
				.map(|path| normalize_overlay_path(&path, case_sensitive))
				.collect(),
			case_sensitive,
		}
	}
}

/// `solutionCompileCache.wrap`: `host` with its declarations and JSON files parsed through
/// `cache`, except those `skip` names, matched with the host's case sensitivity.
#[cfg(test)]
pub(crate) fn wrap(
	cache: &Arc<SolutionCompileCache>,
	host: Arc<dyn rotor_tsrs::compiler::CompilerHost>,
	skip: impl IntoIterator<Item = String>,
) -> Arc<dyn rotor_tsrs::compiler::CompilerHost> {
	let case_sensitive = host.fs().use_case_sensitive_file_names();
	let policy = ProjectParseCache {
		cache: Arc::clone(cache),
		skip: skip
			.into_iter()
			.map(|path| normalize_overlay_path(&path, case_sensitive))
			.collect(),
		case_sensitive,
	};
	rotor_tsrs::project::caching_compiler_host(host, Arc::new(policy))
}

impl rotor_tsrs::project::SourceFileCache for ProjectParseCache {
	fn get_source_file(
		&self,
		opts: SourceFileParseOptions,
		parse: &dyn Fn(SourceFileParseOptions) -> Option<P<SourceFile>>,
	) -> Option<P<SourceFile>> {
		let skipped = !self.skip.is_empty()
			&& self
				.skip
				.contains(&normalize_overlay_path(&opts.file_name, self.case_sensitive));
		if skipped || !cacheable_parse_name(&opts.file_name) {
			return parse(opts);
		}
		self.cache.get_source_file(&opts, parse)
	}

	fn loader_regions(&self) -> Option<rotor_tsrs::project::LoaderRegions> {
		self.cache.regions.clone()
	}
}

/// `syntheticDeclSkip`: the synthetic declarations beside the config.
pub(crate) fn synthetic_decl_skip(config_path: &str) -> Vec<String> {
	let directory = tspath::get_directory_path(&gopath::to_slash(config_path));
	vec![
		format!("{directory}/{}", rotor_tsrs::project::ENV_DECL_FILE_NAME),
		format!("{directory}/{}", rotor_tsrs::project::MACRO_DECL_FILE_NAME),
	]
}

/// `cacheableParseName`: declarations and JSON files are shared between projects.
pub(crate) fn cacheable_parse_name(file_name: &str) -> bool {
	tspath::is_declaration_file_name(file_name) || tspath::file_extension_is(file_name, ".json")
}
