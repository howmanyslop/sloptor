//! `overlayfs.go`.
//!
//! Source overlays. Keys are matched against the program's files by their normalized spelling, then
//! by physical path, and every key must name a file of the program unless the caller opts out. The
//! overlaid program itself is derived with `rotor_tsrs::project::update_with_overlays`, which
//! replaces the texts of the files the keys name (Go builds a fresh program over an overlay file
//! system; the texts are the same).

use std::collections::{BTreeMap, HashMap, HashSet};

use rotor_support::{fsutil, gopath};
use rotor_tsrs::compiler::Program;

use crate::compile::Error;
use crate::project::ProjectOptions;
use crate::sidecar::normalize_overlay_path;

fn case_sensitive() -> bool {
	rotor_tsrs::vfs::osvfs::fs().use_case_sensitive_file_names()
}

/// `canonicalExistingOverlayPath`: resolves only a complete, existing absolute target, so a
/// typo through a symlinked parent is still reported.
pub(crate) fn canonical_existing_overlay_path(path: &str, case_sensitive: bool) -> Option<String> {
	let path = normalize_overlay_path(path, case_sensitive);
	if !gopath::is_abs(&path) {
		return None;
	}
	let resolved = fsutil::eval_symlinks(&path).ok()?;
	Some(normalize_overlay_path(&resolved, case_sensitive))
}

/// `overlayAliases`: the overlay set expanded once by physical path, and by config-relative
/// spellings under the config directory.
#[must_use]
pub fn overlay_aliases(
	overlays: &BTreeMap<String, String>,
	config_path: &str,
	case_sensitive: bool,
) -> HashMap<String, String> {
	let mut aliases = HashMap::with_capacity(overlays.len() * 2);
	if overlays.is_empty() {
		return aliases;
	}
	let config_dir = gopath::dir(&gopath::from_slash(config_path));
	let canonical_config_dir = fsutil::eval_symlinks(&config_dir);
	for (path, text) in overlays {
		aliases.insert(normalize_overlay_path(path, case_sensitive), text.clone());
		if let Some(physical) = canonical_existing_overlay_path(path, case_sensitive) {
			aliases.insert(physical.clone(), text.clone());
			if let Ok(canonical_config_dir) = &canonical_config_dir
				&& let Ok(relative) = gopath::rel(canonical_config_dir, &gopath::from_slash(&physical))
				&& relative != ".."
				&& !relative.starts_with(&format!("..{}", gopath::NATIVE.separator()))
			{
				// Config parsing may probe an include path before making it absolute, while
				// later source reads use the lexical config root.
				aliases.insert(normalize_overlay_path(&relative, case_sensitive), text.clone());
				aliases.insert(
					normalize_overlay_path(&gopath::join(&[&config_dir, &relative]), case_sensitive),
					text.clone(),
				);
			}
		}
	}
	aliases
}

/// `overlayText`: the caller's lexical map first, then the physical aliases.
pub fn overlay_text<'a, S: std::hash::BuildHasher>(
	overlays: &'a HashMap<String, String, S>,
	aliases: &'a HashMap<String, String, S>,
	path: &str,
	case_sensitive: bool,
) -> Option<&'a str> {
	let path = normalize_overlay_path(path, case_sensitive);
	overlays.get(&path).or_else(|| aliases.get(&path)).map(String::as_str)
}

/// The program's files by normalized spelling, and lazily by physical path.
struct ProgramFiles<'a> {
	program: &'a Program,
	case_sensitive: bool,
	by_spelling: HashMap<String, String>,
	by_physical: Option<HashMap<String, String>>,
}

impl<'a> ProgramFiles<'a> {
	fn new(program: &'a Program) -> Self {
		let case_sensitive = case_sensitive();
		let by_spelling = program
			.source_files()
			.iter()
			.map(|file| {
				(
					normalize_overlay_path(file.file_name(), case_sensitive),
					file.file_name().to_owned(),
				)
			})
			.collect();
		Self {
			program,
			case_sensitive,
			by_spelling,
			by_physical: None,
		}
	}

	/// The program file an overlay key names.
	fn find(&mut self, path: &str) -> Option<String> {
		if let Some(file_name) = self.by_spelling.get(&normalize_overlay_path(path, self.case_sensitive)) {
			return Some(file_name.clone());
		}
		let physical = canonical_existing_overlay_path(path, self.case_sensitive)?;
		let case_sensitive = self.case_sensitive;
		let program = self.program;
		let by_physical = self.by_physical.get_or_insert_with(|| {
			program
				.source_files()
				.iter()
				.filter_map(|file| {
					canonical_existing_overlay_path(file.file_name(), case_sensitive)
						.map(|canonical| (canonical, file.file_name().to_owned()))
				})
				.collect()
		});
		by_physical.get(&physical).cloned()
	}
}

/// `overlayKeysInProgram`: the keys naming a file of the program (normalized, so callers can
/// union them across projects) and those naming nothing (as spelled).
pub(crate) fn overlay_keys_in_program(
	program: &Program,
	overlays: &BTreeMap<String, String>,
) -> (HashSet<String>, Vec<String>) {
	let mut files = ProgramFiles::new(program);
	let mut matched = HashSet::with_capacity(overlays.len());
	let mut unmatched = Vec::new();
	for path in overlays.keys() {
		if files.find(path).is_some() {
			matched.insert(normalize_overlay_path(path, files.case_sensitive));
		} else {
			unmatched.push(path.clone());
		}
	}
	(matched, unmatched)
}

/// `matchOverlaysToProgram`: the number of keys naming a file of the program; any key naming
/// nothing fails the run, since it would silently report on the unmodified tree.
pub(crate) fn match_overlays_to_program(
	program: &Program,
	overlays: &BTreeMap<String, String>,
) -> Result<usize, Error> {
	let (matched, mut unmatched) = overlay_keys_in_program(program, overlays);
	if !unmatched.is_empty() {
		unmatched.sort();
		return Err(Error::message(format!(
			"compile: overlay matches no file in the program: {}",
			unmatched.join(", ")
		)));
	}
	Ok(matched.len())
}

/// `matchProgramOverlays`: the single-project rule, lifted by `ignore_unmatched_overlays`, or the
/// solution-wide rule of a solution census.
pub(crate) fn match_program_overlays(program: &Program, opts: &ProjectOptions) -> Result<usize, Error> {
	if let Some(tracker) = &opts.solution_overlays {
		return Ok(crate::census_solution::match_solution_overlays_to_program(
			program,
			&opts.overlays,
			tracker,
		));
	}
	if opts.ignore_unmatched_overlays {
		let (matched, _) = overlay_keys_in_program(program, &opts.overlays);
		return Ok(matched.len());
	}
	match_overlays_to_program(program, &opts.overlays)
}

/// `rekeyOverlaysToProgram`: caller spellings to the exact file names the program reads. Keys
/// are the program's file names (Go keys them normalized; `update_with_overlays` looks files up
/// by name either way).
pub(crate) fn rekey_overlays_to_program(
	program: &Program,
	overlays: &BTreeMap<String, String>,
) -> (BTreeMap<String, String>, Vec<String>) {
	let mut files = ProgramFiles::new(program);
	let mut resolved = BTreeMap::new();
	let mut unmatched = Vec::new();
	for (path, text) in overlays {
		match files.find(path) {
			Some(file_name) => {
				resolved.insert(file_name, text.clone());
			}
			None => unmatched.push(path.clone()),
		}
	}
	(resolved, unmatched)
}
