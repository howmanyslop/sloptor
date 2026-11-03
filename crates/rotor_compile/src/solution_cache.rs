//! `solution_cache.go`: the drain-scoped parse cache of a solution.
//!
//! Every project's program parses the `.d.ts` and `.json` files the projects share once, through
//! a [`rotor_tsrs::project::SourceFileCache`]; ordinary sources stay project-local.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, Ordering};
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

/// `solutionCompileCache`: one per drain.
#[derive(Default)]
pub struct SolutionCompileCache {
	sources: Mutex<HashMap<SourceFileParseOptions, P<SourceFile>>>,
	inflight: Mutex<HashMap<SourceFileParseOptions, Arc<InFlight>>>,
	hits: AtomicI64,
	misses: AtomicI64,
}

impl SolutionCompileCache {
	/// `newSolutionCompileCache`
	pub(crate) fn new() -> Self {
		Self::default()
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
