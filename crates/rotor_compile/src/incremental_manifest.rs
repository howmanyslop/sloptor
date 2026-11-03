//! `incremental_manifest.go`: what one build leaves behind for the next one to select against.
//!
//! `files` carries each source's content hash and the project files it resolved a reference to
//! (the import graph selection walks); keys are absolute in memory and `project://`-anchored on
//! disk. `outputs` carries the hash of every file the build wrote, which lets the writer skip a
//! byte-identical rewrite and doubles as the declaration signature store. `salt` invalidates the
//! whole manifest at once.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use rotor_support::{fsutil, gopath};
use rotor_tsrs::ast::{self, SourceFile};
use rotor_tsrs::checker::TypeChecker;
use rotor_tsrs::compiler::Program;
use rotor_tsrs::{Context, P, tspath};

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
		let mut files = gojson::Object::new();
		for (name, state) in &self.files {
			let mut object = gojson::Object::new().string("hash", &state.hash);
			if !state.refs.is_empty() {
				object = object.raw("refs", &gojson::string_array(&state.refs));
			}
			files = files.raw(name, &object.finish());
		}
		let compact = gojson::Object::new()
			.int("version", self.version)
			.string("salt", &self.salt)
			.raw("files", &files.finish())
			.raw("outputs", &gojson::string_map(&self.outputs))
			.finish();
		let mut text = gojson::indent(&compact, "", "\t");
		text.push('\n');
		text
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
	let source_paths: HashMap<String, String> = program
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
	manifest.files = map_incremental_files(&manifest.files, &|name: &str| {
		let mut absolute = resolve_incremental_source_path(project_dir, name);
		if let Some(source) = source_paths.get(&normalize_source_file_path(&absolute)) {
			return source.clone();
		}
		if !preserve_symlinks && let Ok(real) = fsutil::eval_symlinks(&absolute) {
			absolute = real;
		}
		normalize_source_file_path(&absolute)
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
	let current_hashes: HashMap<String, String> = input_files
		.iter()
		.map(|file| {
			(
				normalize_source_file_path(file.file_name()),
				hex_sha256(file.text().as_bytes()),
			)
		})
		.collect();
	if let Some(previous) = previous
		&& previous.salt == salt
		&& previous.files.len() == current_hashes.len()
		&& current_hashes
			.iter()
			.all(|(path, hash)| previous.files.get(path).is_some_and(|state| state.hash == *hash))
	{
		// No source changed: reuse the previous graph verbatim.
		manifest.files.clone_from(&previous.files);
		return manifest;
	}
	let source_set: HashSet<String> = current_hashes.keys().cloned().collect();
	let global_declarations = global_declaration_paths(&input_files);
	for &file in &input_files {
		let path = normalize_source_file_path(file.file_name());
		let mut refs = Vec::new();
		if !tspath::has_json_file_extension(file.file_name()) {
			refs = referenced_project_files(program, file, &source_set);
			if !file.is_declaration_file() {
				refs.extend(global_declarations.iter().cloned());
				refs.sort();
				refs.dedup();
			}
		}
		let hash = current_hashes.get(&path).cloned().unwrap_or_default();
		manifest.files.insert(path, IncrementalFileState { hash, refs });
	}
	manifest
}

/// `globalDeclarationPaths`: declaration files that declare globals (scripts, or a global
/// augmentation).
fn global_declaration_paths(files: &[P<SourceFile>]) -> Vec<String> {
	files
		.iter()
		.filter(|file| file.is_declaration_file())
		.filter(|&&file| {
			!ast::is_external_module(file)
				|| file
					.module_augmentations()
					.iter()
					.any(|&augmentation| !ast::is_string_literal(augmentation))
		})
		.map(|file| normalize_source_file_path(file.file_name()))
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
	let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
	accumulate_reverse_deps(&mut reverse, Some(current));
	accumulate_reverse_deps(&mut reverse, Some(previous));

	let mut selected: HashSet<String> = HashSet::new();
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
pub(crate) fn accumulate_reverse_deps(
	reverse: &mut HashMap<String, Vec<String>>,
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

/// `referencedProjectFiles`: the project files `file` references through imports, triple-slash
/// references, type reference directives, module augmentations and ambient modules.
#[expect(
	clippy::significant_drop_tightening,
	reason = "every query runs on the file's checker"
)]
fn referenced_project_files(
	program: &'static Program,
	file: P<SourceFile>,
	source_set: &HashSet<String>,
) -> Vec<String> {
	let own = normalize_source_file_path(file.file_name());
	let mut referenced: BTreeSet<String> = BTreeSet::new();
	let mut add = |path: &str| {
		let path = normalize_source_file_path(path);
		if path != own && source_set.contains(&path) {
			referenced.insert(path);
		}
	};
	let context = Context::default();
	let mut handle = program.get_type_checker_for_file_exclusive(&context, file);
	let mut checker = TypeChecker::new(&mut handle);
	let add_symbol_decls = |checker: &mut TypeChecker<'_>, node: P<ast::Node>, add: &mut dyn FnMut(&str)| {
		if let Some(symbol) = checker.get_symbol_at_location(node) {
			for &declaration in symbol.declarations() {
				if let Some(source_file) = ast::get_source_file_of_node(declaration) {
					add(source_file.file_name());
				}
			}
		}
	};
	for &import in file.imports() {
		add_symbol_decls(&mut checker, import, &mut add);
	}
	let source_file_directory = gopath::dir(&gopath::from_slash(file.file_name()));
	for reference in file.referenced_files() {
		add(&resolve_referenced_file(
			program,
			&reference.file_name,
			&source_file_directory,
		));
	}
	if let Some(type_refs) = program.get_resolved_type_reference_directives().get(file.path()) {
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
	for &module_name in file.module_augmentations() {
		if ast::is_string_literal(module_name) {
			add_symbol_decls(&mut checker, module_name, &mut add);
		}
	}
	for ambient_module in checker.get_ambient_modules() {
		for &declaration in ambient_module.declarations() {
			if let Some(source_file) = ast::get_source_file_of_node(declaration) {
				add(source_file.file_name());
			}
		}
	}
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
