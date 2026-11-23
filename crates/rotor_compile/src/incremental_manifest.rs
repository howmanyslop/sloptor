//! `incremental_manifest.go`: what one build leaves behind for the next one to select against.
//!
//! `files` carries each source's content hash and the project files it resolved a reference to
//! (the import graph selection walks); keys are absolute in memory and `project://`-anchored on
//! disk. `outputs` carries the hash of every file the build wrote, which lets the writer skip a
//! byte-identical rewrite and doubles as the declaration signature store. `salt` invalidates the
//! whole manifest at once.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;

use rayon::prelude::*;
use rotor_support::{fsutil, gopath};
use rotor_tsrs::ast::{self, SourceFile};
use rotor_tsrs::checker::TypeChecker;
use rotor_tsrs::compiler::Program;
use rotor_tsrs::{Context, P, tspath};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::compile::Error;
use crate::gojson::{self, Value};
use crate::incremental_paths::{incremental_source_path_encoder, resolve_incremental_source_path};
use crate::outputwriter_manifest::OutputPresenceIndex;
use crate::package_layout::hex_sha256;

/// `incrementalFileState`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IncrementalFileState {
	/// The content hash.
	pub hash: String,
	/// The project files the source resolved a reference to, sorted.
	pub refs: Vec<String>,
}

/// `incrementalManifest`
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IncrementalManifest {
	/// The format version (3).
	pub version: i64,
	/// Invalidates the whole manifest.
	pub salt: String,
	/// Each input's state, by path.
	pub files: BTreeMap<String, IncrementalFileState>,
	/// Each written output's hash, by project-relative path.
	pub outputs: BTreeMap<String, String>,
}

/// `normalizeSourceFilePath`
#[must_use]
pub fn normalize_source_file_path(path: &str) -> String {
	gopath::clean(&gopath::from_slash(path))
}

impl IncrementalManifest {
	/// An empty version 3 manifest with `salt`.
	pub(crate) fn empty(salt: String) -> Self {
		Self {
			version: 3,
			salt,
			..Self::default()
		}
	}

	/// Go's `json.MarshalIndent(manifest, "", "\t")` plus the trailing newline.
	fn to_json(&self) -> String {
		let estimated = 64 + self.files.len() * 128 + self.outputs.len() * 96;
		let mut out = String::with_capacity(estimated);
		let _ = writeln!(out, "{{");
		let _ = writeln!(out, "\t\"version\": {},", self.version);
		out.push_str("\t\"salt\": ");
		gojson::write_string(&mut out, &self.salt);
		out.push_str(",\n\t\"files\": ");
		if self.files.is_empty() {
			out.push_str("{},\n");
		} else {
			out.push_str("{\n");
			let last_file = self.files.len() - 1;
			for (idx, (name, state)) in self.files.iter().enumerate() {
				out.push_str("\t\t");
				gojson::write_string(&mut out, name);
				out.push_str(": {\n\t\t\t\"hash\": ");
				gojson::write_string(&mut out, &state.hash);
				if state.refs.is_empty() {
					out.push_str("\n\t\t}");
				} else {
					out.push_str(",\n\t\t\t\"refs\": [\n");
					let last_ref = state.refs.len() - 1;
					for (ridx, item) in state.refs.iter().enumerate() {
						out.push_str("\t\t\t\t");
						gojson::write_string(&mut out, item);
						if ridx < last_ref {
							out.push(',');
						}
						out.push('\n');
					}
					out.push_str("\t\t\t]\n\t\t}");
				}
				if idx < last_file {
					out.push(',');
				}
				out.push('\n');
			}
			out.push_str("\t},\n");
		}
		out.push_str("\t\"outputs\": ");
		if self.outputs.is_empty() {
			out.push_str("{}\n");
		} else {
			out.push_str("{\n");
			let last_output = self.outputs.len() - 1;
			for (idx, (key, value)) in self.outputs.iter().enumerate() {
				out.push_str("\t\t");
				gojson::write_string(&mut out, key);
				out.push_str(": ");
				gojson::write_string(&mut out, value);
				if idx < last_output {
					out.push(',');
				}
				out.push('\n');
			}
			out.push_str("\t}\n");
		}
		out.push_str("}\n");
		out
	}

	fn from_json(data: &[u8]) -> Option<Self> {
		let value = gojson::parse(data).ok()?;
		if !matches!(value, Value::Object(_)) {
			return None;
		}
		let version = match value.field("version") {
			Some(Value::Number(text)) => text.parse().ok()?,
			None | Some(Value::Null) => 0,
			Some(_) => return None,
		};
		let salt = match value.field("salt") {
			Some(Value::String(text)) => text.clone(),
			None | Some(Value::Null) => String::new(),
			Some(_) => return None,
		};
		let mut manifest = Self {
			version,
			salt,
			..Self::default()
		};
		if let Some(files) = value.field("files").filter(|files| **files != Value::Null) {
			for (name, state) in files.as_object()? {
				let mut refs = Vec::new();
				if let Some(list) = state.field("refs").filter(|refs| **refs != Value::Null) {
					for entry in list.as_array()? {
						refs.push(entry.as_str()?.to_owned());
					}
				}
				let hash = match state.field("hash") {
					Some(Value::String(hash)) => hash.clone(),
					None | Some(Value::Null) => String::new(),
					Some(_) => return None,
				};
				manifest.files.insert(name.clone(), IncrementalFileState { hash, refs });
			}
		}
		if let Some(outputs) = value.field("outputs").filter(|outputs| **outputs != Value::Null) {
			for (name, hash) in outputs.as_object()? {
				manifest.outputs.insert(name.clone(), hash.as_str()?.to_owned());
			}
		}
		Some(manifest)
	}
}

/// `readIncrementalManifest`: `None` for a missing, unreadable-as-JSON or other-version
/// manifest. Stored names are mapped back to the program's file names.
pub(crate) fn read_incremental_manifest(
	path: &str,
	program: Option<&Program>,
) -> Result<Option<IncrementalManifest>, Error> {
	let data = match std::fs::read(path) {
		Ok(data) => data,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
		// `os.ReadFile` opens a directory fine and fails reading it.
		Err(error) => {
			let op = if error.kind() == std::io::ErrorKind::IsADirectory {
				"read"
			} else {
				"open"
			};
			return Err(Error::message(rotor_support::gostr::path_error(op, path, &error)));
		}
	};
	let Some(mut manifest) = IncrementalManifest::from_json(&data) else {
		return Ok(None);
	};
	if manifest.version != 3 {
		return Ok(None);
	}
	let Some(program) = program else {
		return Ok(Some(manifest));
	};
	// Reuse the compiler's file names without a realpath per file on a warm build.
	let project_dir = program.get_current_directory();
	let encode = incremental_source_path_encoder(program);
	let source_paths: FxHashMap<String, String> = program
		.source_files()
		.iter()
		.map(|file| {
			let stored = resolve_incremental_source_path(project_dir, &encode(file.file_name()));
			(
				normalize_source_file_path(&stored),
				normalize_source_file_path(file.file_name()),
			)
		})
		.collect();
	let preserve_symlinks = program.options().preserve_symlinks.is_true();
	// A name recurs as every importer's ref; each converts once.
	let converted: std::cell::RefCell<FxHashMap<String, String>> = std::cell::RefCell::default();
	manifest.files = map_incremental_files(&manifest.files, &|name: &str| {
		if let Some(done) = converted.borrow().get(name) {
			return done.clone();
		}
		let absolute = resolve_incremental_source_path(project_dir, name);
		let result = source_paths.get(&normalize_source_file_path(&absolute)).map_or_else(
			|| {
				let real = if preserve_symlinks {
					None
				} else {
					fsutil::eval_symlinks(&absolute).ok()
				};
				normalize_source_file_path(real.as_deref().unwrap_or(&absolute))
			},
			Clone::clone,
		);
		converted.borrow_mut().insert(name.to_owned(), result.clone());
		result
	});
	Ok(Some(manifest))
}

/// `writeIncrementalManifest`: atomic, with stored names encoded portably.
pub(crate) fn write_incremental_manifest(
	path: &str,
	manifest: &IncrementalManifest,
	program: &Program,
) -> Result<(), Error> {
	write_incremental_manifest_data(path, &encode_incremental_manifest(manifest, program))
}

/// The bytes [`write_incremental_manifest`] writes, with the source paths encoded against
/// `program`. A deferred write encodes first, so it does not keep the program alive.
pub(crate) fn encode_incremental_manifest(manifest: &IncrementalManifest, program: &Program) -> String {
	let encode = incremental_source_path_encoder(program);
	let stored = IncrementalManifest {
		files: map_incremental_files(&manifest.files, &*encode),
		..manifest.clone()
	};
	stored.to_json()
}

/// Writes encoded manifest `data` to `path` atomically.
pub(crate) fn write_incremental_manifest_data(path: &str, data: &str) -> Result<(), Error> {
	let io = |error: std::io::Error| Error::message(rotor_support::gostr::io_error_text(&error));
	let directory = gopath::dir(path);
	fsutil::mkdir_all(&directory, 0o755).map_err(io)?;
	let mut temp = tempfile::Builder::new()
		.prefix(".tsbuildinfo-")
		.suffix(".tmp")
		.tempfile_in(&directory)
		.map_err(io)?;
	std::io::Write::write_all(&mut temp, data.as_bytes()).map_err(io)?;
	// Go chmods the temp file (created 0600) to 0644 before the rename.
	fsutil::chmod(temp.as_file(), 0o644).map_err(io)?;
	temp.persist(path).map_err(|error| io(error.error))?;
	Ok(())
}

/// `mapIncrementalFiles`: converts names and refs, refs sorted again.
fn map_incremental_files(
	files: &BTreeMap<String, IncrementalFileState>,
	convert: &dyn Fn(&str) -> String,
) -> BTreeMap<String, IncrementalFileState> {
	files
		.iter()
		.map(|(name, state)| {
			let mut refs: Vec<String> = state.refs.iter().map(|reference| convert(reference)).collect();
			refs.sort();
			(
				convert(name),
				IncrementalFileState {
					hash: state.hash.clone(),
					refs,
				},
			)
		})
		.collect()
}

/// `sameIncrementalManifest`
pub(crate) fn same_incremental_manifest(a: Option<&IncrementalManifest>, b: Option<&IncrementalManifest>) -> bool {
	a == b
}

/// `resolvedNonCompiledInputs`: declarations and JSON inputs, which change an importer's output
/// without being compiled themselves.
fn resolved_non_compiled_inputs(program: &Program) -> Vec<P<SourceFile>> {
	program
		.source_files()
		.iter()
		.copied()
		.filter(|&file| {
			file.is_declaration_file()
				|| (tspath::has_json_file_extension(file.file_name())
					&& !program.is_source_from_project_reference(file.path()))
		})
		.collect()
}

/// `buildIncrementalManifest`
pub(crate) fn build_incremental_manifest(
	program: &'static Program,
	source_files: &[P<SourceFile>],
	salt: &str,
	previous: Option<&IncrementalManifest>,
) -> IncrementalManifest {
	let mut input_files = source_files.to_vec();
	input_files.extend(resolved_non_compiled_inputs(program));
	let mut manifest = IncrementalManifest::empty(salt.to_owned());
	let items: Vec<(P<SourceFile>, String, String)> = if input_files.len() >= 16 && rayon::current_num_threads() > 1 {
		input_files
			.par_iter()
			.map(|&file| {
				(
					file,
					normalize_source_file_path(file.file_name()),
					hex_sha256(file.text().as_bytes()),
				)
			})
			.collect()
	} else {
		input_files
			.iter()
			.map(|&file| {
				(
					file,
					normalize_source_file_path(file.file_name()),
					hex_sha256(file.text().as_bytes()),
				)
			})
			.collect()
	};
	let current_hashes: FxHashMap<&str, &str> = items
		.iter()
		.map(|(_, path, hash)| (path.as_str(), hash.as_str()))
		.collect();
	if let Some(previous) = previous
		&& previous.salt == salt
		&& previous.files.len() == current_hashes.len()
		&& current_hashes
			.iter()
			.all(|(path, hash)| previous.files.get(*path).is_some_and(|state| state.hash == *hash))
	{
		// No source changed: reuse the previous graph verbatim.
		manifest.files.clone_from(&previous.files);
		return manifest;
	}
	let source_set: FxHashSet<String> = items.iter().map(|(_, path, _)| path.clone()).collect();
	let global_declarations = global_declaration_paths(&items);
	let ambient_module_paths = collect_ambient_module_paths(program, &input_files, &source_set);

	let mut all_refs: Vec<Vec<String>> = vec![Vec::new(); items.len()];
	let mut checker_indices = Vec::new();
	for (index, (file, path, _)) in items.iter().enumerate() {
		if tspath::has_json_file_extension(file.file_name()) {
			continue;
		}
		if file.imports().is_empty()
			&& !file
				.module_augmentations()
				.iter()
				.any(|&module_name| ast::is_string_literal(module_name))
		{
			all_refs[index] =
				referenced_project_files_without_checker(program, *file, path, &source_set, &ambient_module_paths);
		} else {
			checker_indices.push(index);
		}
	}

	if !checker_indices.is_empty() {
		let context = Context::default();
		let mut groups_by_checker: FxHashMap<u32, usize> = FxHashMap::default();
		let mut groups: Vec<Vec<usize>> = Vec::new();
		for index in checker_indices {
			let key = program.get_type_checker_for_file_exclusive(&context, items[index].0).id;
			let group_index = *groups_by_checker.entry(key).or_insert_with(|| {
				groups.push(Vec::new());
				groups.len() - 1
			});
			groups[group_index].push(index);
		}
		let single_threaded = program.single_threaded() || groups.len() <= 1;
		let group_results = crate::project::run_work_group(single_threaded, groups.len(), |group_index| {
			groups[group_index]
				.iter()
				.map(|&index| {
					let (file, path, _) = &items[index];
					let refs = referenced_project_files(program, *file, path, &source_set, &ambient_module_paths);
					(index, refs)
				})
				.collect::<Vec<_>>()
		});
		for group_result in group_results {
			for (index, refs) in group_result {
				all_refs[index] = refs;
			}
		}
	}

	for ((file, path, hash), mut refs) in items.into_iter().zip(all_refs) {
		if !tspath::has_json_file_extension(file.file_name()) && !file.is_declaration_file() {
			refs.extend(global_declarations.iter().cloned());
			refs.sort();
			refs.dedup();
		}
		manifest.files.insert(path, IncrementalFileState { hash, refs });
	}
	manifest
}

/// `globalDeclarationPaths`: declaration files that declare globals (scripts, or a global
/// augmentation).
fn global_declaration_paths(items: &[(P<SourceFile>, String, String)]) -> Vec<String> {
	items
		.iter()
		.filter(|(file, _, _)| file.is_declaration_file())
		.filter(|(file, _, _)| {
			!ast::is_external_module(*file)
				|| file
					.module_augmentations()
					.iter()
					.any(|&augmentation| !ast::is_string_literal(augmentation))
		})
		.map(|(_, path, _)| path.clone())
		.collect()
}

/// `pruneMissingOutputs`
pub(crate) fn prune_missing_outputs(index: &OutputPresenceIndex, outputs: &mut BTreeMap<String, String>) {
	outputs.retain(|path, _| index.has_regular(path));
}

/// `selectIncrementalSourceFiles`: the changed files and their importer closure.
pub(crate) fn select_incremental_source_files(
	source_files: &[P<SourceFile>],
	current: &IncrementalManifest,
	previous: Option<&IncrementalManifest>,
) -> Vec<P<SourceFile>> {
	let Some(previous) = previous.filter(|previous| previous.salt == current.salt) else {
		return source_files.to_vec();
	};
	let mut changed: BTreeSet<String> = BTreeSet::new();
	for (path, state) in &current.files {
		if previous.files.get(path).is_none_or(|prev| prev.hash != state.hash) {
			changed.insert(path.clone());
		}
	}
	for path in previous.files.keys() {
		if !current.files.contains_key(path) {
			changed.insert(path.clone());
		}
	}
	if changed.is_empty() {
		return Vec::new();
	}
	let mut reverse: FxHashMap<String, Vec<String>> = FxHashMap::default();
	accumulate_reverse_deps(&mut reverse, Some(current));
	accumulate_reverse_deps(&mut reverse, Some(previous));

	let mut selected: FxHashSet<String> = FxHashSet::default();
	let mut queue: VecDeque<String> = VecDeque::new();
	for path in changed {
		if current.files.contains_key(&path) {
			selected.insert(path.clone());
		}
		queue.push_back(path);
	}
	while let Some(path) = queue.pop_front() {
		for importer in reverse.get(&path).into_iter().flatten() {
			if selected.insert(importer.clone()) {
				queue.push_back(importer.clone());
			}
		}
	}
	source_files
		.iter()
		.copied()
		.filter(|file| selected.contains(&normalize_source_file_path(file.file_name())))
		.collect()
}

/// `accumulateReverseDeps`
pub(crate) fn accumulate_reverse_deps<S: std::hash::BuildHasher>(
	reverse: &mut std::collections::HashMap<String, Vec<String>, S>,
	manifest: Option<&IncrementalManifest>,
) {
	let Some(manifest) = manifest else {
		return;
	};
	for (importer, state) in &manifest.files {
		for dependency in &state.refs {
			reverse.entry(dependency.clone()).or_default().push(importer.clone());
		}
	}
}

#[expect(
	clippy::significant_drop_tightening,
	reason = "checker borrows handle during ambient module walk"
)]
fn collect_ambient_module_paths(
	program: &'static Program,
	input_files: &[P<SourceFile>],
	source_set: &FxHashSet<String>,
) -> Vec<String> {
	input_files
		.iter()
		.copied()
		.find(|file| !tspath::has_json_file_extension(file.file_name()))
		.map_or_default(|first_file| {
			let context = Context::default();
			let mut handle = program.get_type_checker_for_file_exclusive(&context, first_file);
			let mut checker = TypeChecker::new(&mut handle);
			let mut paths = Vec::new();
			for ambient_module in checker.get_ambient_modules() {
				for &declaration in ambient_module.declarations() {
					if let Some(source_file) = ast::get_source_file_of_node(declaration) {
						let path = normalize_source_file_path(source_file.file_name());
						if source_set.contains(&path) {
							paths.push(path);
						}
					}
				}
			}
			paths.sort_unstable();
			paths.dedup();
			paths
		})
}

fn add_non_checker_refs(
	program: &Program,
	file: P<SourceFile>,
	own: &str,
	source_set: &FxHashSet<String>,
	ambient_module_paths: &[String],
	referenced: &mut BTreeSet<String>,
) {
	let mut add = |path: &str| {
		let path = normalize_source_file_path(path);
		if path != own && source_set.contains(&path) {
			referenced.insert(path);
		}
	};
	let referenced_files = file.referenced_files();
	let type_refs = program.get_resolved_type_reference_directives().get(file.path());
	if !referenced_files.is_empty() || type_refs.is_some() {
		let source_file_directory = gopath::dir(&gopath::from_slash(file.file_name()));
		for reference in referenced_files {
			add(&resolve_referenced_file(
				program,
				&reference.file_name,
				&source_file_directory,
			));
		}
		if let Some(type_refs) = type_refs {
			let mut resolved: Vec<&str> = type_refs
				.values()
				.map(|type_ref| type_ref.resolved_file_name)
				.filter(|name| !name.is_empty())
				.collect();
			resolved.sort_unstable();
			for name in resolved {
				add(&resolve_referenced_file(program, name, &source_file_directory));
			}
		}
	}
	for path in ambient_module_paths {
		if *path != own {
			referenced.insert(path.clone());
		}
	}
}

fn referenced_project_files_without_checker(
	program: &Program,
	file: P<SourceFile>,
	own: &str,
	source_set: &FxHashSet<String>,
	ambient_module_paths: &[String],
) -> Vec<String> {
	let mut referenced: BTreeSet<String> = BTreeSet::new();
	add_non_checker_refs(program, file, own, source_set, ambient_module_paths, &mut referenced);
	referenced.into_iter().collect()
}

/// `referencedProjectFiles`: the project files `file` references through imports, triple-slash
/// references, type reference directives, module augmentations and ambient modules.
fn referenced_project_files(
	program: &'static Program,
	file: P<SourceFile>,
	own: &str,
	source_set: &FxHashSet<String>,
	ambient_module_paths: &[String],
) -> Vec<String> {
	let mut referenced: BTreeSet<String> = BTreeSet::new();
	let context = Context::default();
	let mut handle = program.get_type_checker_for_file_exclusive(&context, file);
	{
		let mut checker = TypeChecker::new(&mut handle);
		let add_symbol_decls =
			|checker: &mut TypeChecker<'_>, node: P<ast::Node>, referenced: &mut BTreeSet<String>| {
				if let Some(symbol) = checker.get_symbol_at_location(node) {
					for &declaration in symbol.declarations() {
						if let Some(source_file) = ast::get_source_file_of_node(declaration) {
							let path = normalize_source_file_path(source_file.file_name());
							if path != own && source_set.contains(&path) {
								referenced.insert(path);
							}
						}
					}
				}
			};
		for &import in file.imports() {
			add_symbol_decls(&mut checker, import, &mut referenced);
		}
		for &module_name in file.module_augmentations() {
			if ast::is_string_literal(module_name) {
				add_symbol_decls(&mut checker, module_name, &mut referenced);
			}
		}
	}
	drop(handle);
	add_non_checker_refs(program, file, own, source_set, ambient_module_paths, &mut referenced);
	referenced.into_iter().collect()
}

/// `resolveReferencedFile`
fn resolve_referenced_file(program: &Program, file_name: &str, source_file_directory: &str) -> String {
	let redirect = program.get_parse_file_redirect(file_name);
	if !redirect.is_empty() {
		return redirect;
	}
	let native = gopath::from_slash(file_name);
	if gopath::is_abs(&native) {
		return native.into_owned();
	}
	gopath::join(&[source_file_directory, &native])
}

#[cfg(test)]
mod tests {
	use super::*;

	fn reference_to_json(manifest: &IncrementalManifest) -> String {
		let mut files = gojson::Object::new();
		for (name, state) in &manifest.files {
			let mut object = gojson::Object::new().string("hash", &state.hash);
			if !state.refs.is_empty() {
				object = object.raw("refs", &gojson::string_array(&state.refs));
			}
			files = files.raw(name, &object.finish());
		}
		let compact = gojson::Object::new()
			.int("version", manifest.version)
			.string("salt", &manifest.salt)
			.raw("files", &files.finish())
			.raw("outputs", &gojson::string_map(&manifest.outputs))
			.finish();
		let mut text = gojson::indent(&compact, "", "\t");
		text.push('\n');
		text
	}

	#[test]
	fn to_json_matches_gojson_indent() {
		let empty = IncrementalManifest::empty("salt-1".to_owned());
		assert_eq!(empty.to_json(), reference_to_json(&empty));

		let mut multi = IncrementalManifest::empty("salt<2>&\"special\"".to_owned());
		multi.files.insert(
			"project://src/a.ts".to_owned(),
			IncrementalFileState {
				hash: "abc".to_owned(),
				refs: Vec::new(),
			},
		);
		multi.files.insert(
			"project://src/b<1>.ts".to_owned(),
			IncrementalFileState {
				hash: "def".to_owned(),
				refs: vec!["project://src/a.ts".to_owned(), "project://src/c&d.ts".to_owned()],
			},
		);
		multi.outputs.insert("out/a.luau".to_owned(), "hash1".to_owned());
		multi.outputs.insert("out/b<1>.luau".to_owned(), "hash2".to_owned());
		assert_eq!(multi.to_json(), reference_to_json(&multi));
	}
}
