//! `incremental_paths.go`.
//!
//! The portable spellings the incremental manifest persists. Source identities are anchored at the
//! project (`project://`), and package-store realpaths are reversed to their known links so a
//! restored cache is independent of the machine.

use std::cell::RefCell;
use std::collections::HashMap;

use rotor_support::{fsutil, gopath};
use rotor_tsrs::compiler::Program;

use crate::incremental_manifest::normalize_source_file_path;

/// `incrementalProjectPathPrefix`
pub(crate) const INCREMENTAL_PROJECT_PATH_PREFIX: &str = "project://";

/// `resolveIncrementalSourcePath`: a stored name back to an absolute path.
pub(crate) fn resolve_incremental_source_path(project_dir: &str, name: &str) -> String {
	if let Some(relative) = name.strip_prefix(INCREMENTAL_PROJECT_PATH_PREFIX) {
		return normalize_source_file_path(&gopath::join(&[project_dir, &gopath::from_slash(relative)]));
	}
	normalize_source_file_path(name)
}

/// A path encoder.
pub(crate) type Encoder<'a> = Box<dyn Fn(&str) -> String + 'a>;

/// `incrementalSourcePathEncoder`: project-relative names get the `project://` anchor.
pub(crate) fn incremental_source_path_encoder(program: &Program) -> Encoder<'_> {
	let encode = incremental_path_encoder(program.get_current_directory(), Some(program));
	Box::new(move |name: &str| {
		let relative = encode(name);
		if gopath::is_abs(&gopath::from_slash(&relative)) {
			return relative;
		}
		format!("{INCREMENTAL_PROJECT_PATH_PREFIX}{relative}")
	})
}

/// `relativeIncrementalPath`
pub(crate) fn relative_incremental_path(dir: &str, name: &str) -> String {
	if let Some(template) = incremental_path_template(name, &|prefix| relative_incremental_path(dir, prefix)) {
		return template;
	}
	if name.is_empty() {
		return String::new();
	}
	let mut name = gopath::from_slash(name).into_owned();
	if gopath::is_abs(&name)
		&& let Ok(relative) = gopath::rel(&gopath::from_slash(dir), &name)
	{
		name = relative;
	}
	gopath::to_slash(&gopath::clean(&name)).into_owned()
}

/// `incrementalRelativeSourcePathEncoder`: also tries the physical spelling of `dir` once.
fn incremental_relative_source_path_encoder(dir: &str) -> Encoder<'static> {
	let dir = dir.to_owned();
	let physical = fsutil::eval_symlinks(&dir)
		.ok()
		.filter(|physical| normalize_source_file_path(physical) != normalize_source_file_path(&dir));
	match physical {
		None => Box::new(move |name: &str| relative_incremental_path(&dir, name)),
		Some(physical_dir) => Box::new(move |name: &str| {
			let logical = relative_incremental_path(&dir, name);
			let physical = relative_incremental_path(&physical_dir, name);
			if less_incremental_path(&physical, &logical) {
				physical
			} else {
				logical
			}
		}),
	}
}

/// `incrementalPathEncoder`: reverses the compiler's known package symlinks instead of
/// persisting pnpm's machine-specific store paths.
#[expect(clippy::too_many_lines, reason = "one function in Go")]
pub(crate) fn incremental_path_encoder<'a>(dir: &str, program: Option<&'a Program>) -> Encoder<'a> {
	let project_dir = program.map_or_else(|| dir.to_owned(), |program| program.get_current_directory().to_owned());
	let case_sensitive = program.is_none_or(Program::use_case_sensitive_file_names);
	let canonical = move |name: &str| {
		let name = normalize_source_file_path(name);
		if case_sensitive { name } else { name.to_lowercase() }
	};
	let encode_relative = incremental_relative_source_path_encoder(dir);
	let encode_project_relative = if canonical(&project_dir) == canonical(dir) {
		incremental_relative_source_path_encoder(dir)
	} else {
		incremental_relative_source_path_encoder(&project_dir)
	};
	let path_after_segments = move |name: &str, segments: &[&str]| -> Option<String> {
		let haystack = format!("/{}/", gopath::to_slash(&canonical(name)));
		let needle = format!("/{}/", segments.join("/"));
		haystack
			.find(&needle)
			.map(|index| haystack[index + needle.len()..].to_owned())
	};

	let mut aliases: HashMap<String, Vec<String>> = HashMap::new();
	let mut add_alias = |real: &str, link: &str| {
		let link = normalize_source_file_path(link);
		let entry = aliases.entry(canonical(real)).or_default();
		if !entry.contains(&link) {
			entry.push(link);
		}
	};
	if let Some(program) = program {
		let cache = program.get_symlink_cache();
		let mut pairs: Vec<(String, String)> = Vec::new();
		let mut collect =
			|real: &rotor_tsrs::tspath::Path,
			 links: &rotor_tsrs::P<rotor_tsrs::tscore::collections::SyncSet<String>>| {
				links.range(|link| {
					pairs.push((real.to_string(), link.clone()));
					true
				});
				true
			};
		cache.files_by_realpath().range(&mut collect);
		cache.directories_by_realpath().range(&mut collect);
		pairs.sort();
		for (real, link) in pairs {
			add_alias(&real, &link);
			// addHoistedPackageAlias
			let mut real = normalize_source_file_path(&real);
			let mut link = normalize_source_file_path(&link);
			let Some(suffix) = path_after_segments(&link, &["node_modules", ".pnpm", "node_modules"]) else {
				continue;
			};
			let parts: Vec<&str> = suffix.trim_end_matches('/').split('/').collect();
			let package_parts = if parts[0].starts_with('@') { 2 } else { 1 };
			if parts.len() <= package_parts {
				continue;
			}
			let mut agrees = true;
			for _ in 0..parts.len() - package_parts {
				if canonical(&gopath::base(&real)) != canonical(&gopath::base(&link)) {
					agrees = false;
					break;
				}
				real = gopath::dir(&real);
				link = gopath::dir(&link);
			}
			if agrees {
				add_alias(&real, &link);
			}
		}
	}

	let encoded: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
	let encode_inner = move |name: &str, this: &dyn Fn(&str) -> String| -> String {
		if let Some(template) = incremental_path_template(name, this) {
			return template;
		}
		if name.is_empty() {
			return String::new();
		}
		if let Some(result) = encoded.borrow().get(name) {
			return result.clone();
		}
		let mut best = encode_relative(name);
		let relative_to_project = encode_project_relative(name);
		let is_project_local = gopath::is_local(&gopath::from_slash(&relative_to_project));
		let in_node_modules = path_after_segments(&relative_to_project, &["node_modules"]).is_some();
		let in_virtual_store = path_after_segments(&relative_to_project, &["node_modules", ".pnpm"]).is_some();
		if is_project_local && !in_node_modules {
			encoded.borrow_mut().insert(name.to_owned(), best.clone());
			return best;
		}
		let mut best_link: Option<String> = None;
		let first = normalize_source_file_path(name);
		let mut queue = vec![first.clone()];
		let mut seen = std::collections::HashSet::from([first]);
		let mut depth = 0;
		while depth < aliases.len() && !queue.is_empty() {
			let mut next = Vec::new();
			for current in &queue {
				let mut ancestor = current.clone();
				loop {
					if let Some(links) = aliases.get(&canonical(&ancestor)) {
						for link in links {
							let rel = gopath::rel(&ancestor, current).unwrap_or_default();
							let candidate = gopath::join(&[link, &rel]);
							if !seen.insert(candidate.clone()) {
								continue;
							}
							next.push(candidate.clone());
							let candidate_relative = encode_project_relative(&candidate);
							if let Some(store_suffix) =
								path_after_segments(&candidate_relative, &["node_modules", ".pnpm"])
								&& (is_project_local || in_virtual_store)
								&& !store_suffix.starts_with("node_modules/")
							{
								continue;
							}
							if best_link
								.as_ref()
								.is_none_or(|best_link| less_incremental_path(&candidate_relative, best_link))
							{
								best = encode_relative(&candidate);
								best_link = Some(candidate_relative);
							}
						}
					}
					let parent = gopath::dir(&ancestor);
					if parent == ancestor {
						break;
					}
					ancestor = parent;
				}
			}
			queue = next;
			depth += 1;
		}
		encoded.borrow_mut().insert(name.to_owned(), best.clone());
		best
	};
	Box::new(move |name: &str| recurse(name, &encode_inner))
}

/// An encoder that encodes a template's prefix with itself.
type Recursive<'a> = dyn Fn(&str, &dyn Fn(&str) -> String) -> String + 'a;

/// Runs `inner` with itself as the prefix encoder.
fn recurse(name: &str, inner: &Recursive<'_>) -> String {
	inner(name, &|prefix| recurse(prefix, inner))
}

/// `incrementalPathTemplate`: only the part before the first `*` is encoded; cleaning `*/../`
/// would change its meaning.
pub(crate) fn incremental_path_template(name: &str, encode_prefix: &dyn Fn(&str) -> String) -> Option<String> {
	let name = name.replace('\\', "/");
	let (prefix, suffix) = name.split_once('*')?;
	let mut encoded = encode_prefix(prefix);
	if prefix.ends_with('/') && !encoded.ends_with('/') {
		encoded.push('/');
	}
	Some(format!("{encoded}*{suffix}"))
}

/// `lessIncrementalPath`: relative before absolute, fewer `../` first, then shorter, then
/// lexicographic.
pub(crate) fn less_incremental_path(left: &str, right: &str) -> bool {
	let (left_abs, right_abs) = (
		gopath::is_abs(&gopath::from_slash(left)),
		gopath::is_abs(&gopath::from_slash(right)),
	);
	if left_abs != right_abs {
		return !left_abs;
	}
	let parents = |mut name: &str| {
		let mut count = 0;
		while let Some(rest) = name.strip_prefix("../") {
			count += 1;
			name = rest;
		}
		count
	};
	let (left_parents, right_parents) = (parents(left), parents(right));
	if left_parents != right_parents {
		return left_parents < right_parents;
	}
	if left.len() != right.len() {
		return left.len() < right.len();
	}
	left < right
}
