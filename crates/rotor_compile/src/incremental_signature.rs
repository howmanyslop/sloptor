//! `incremental_signature.go`.
//!
//! TypeScript's declaration-signature rule. An incremental build re-emits the `.d.ts` of a file it
//! is considering and stops propagating to that file's importers when the declaration's hash
//! matches the one recorded last time, so rotor selects strictly fewer files than rbxtsc for the
//! same edit and writes the same bytes.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use rotor_rojo::PathTranslator;
use rotor_support::{gopath, logservice};
use rotor_tsrs::P;
use rotor_tsrs::ast::SourceFile;
use rotor_tsrs::compiler::Program;
use rotor_tsrs::tsoptions::outputpaths;
use rotor_tsrs::tspath;

use crate::compile::Error;
use crate::incremental_manifest::{IncrementalManifest, accumulate_reverse_deps, normalize_source_file_path};
use crate::output::{DeclarationEmitFile, emit_declaration_texts, emits_declaration_output};
use crate::outputwriter_manifest::OutputPresenceIndex;
use crate::package_layout::hex_sha256;
use crate::timings::{BuildTimings, Stage, log_stage_named};

/// `declarationSignatureSelection`
pub(crate) struct DeclarationSignatureSelection<'a> {
	pub(crate) project_dir: String,
	pub(crate) declaration_path: &'a dyn Fn(&str) -> String,
	pub(crate) previous_outputs: &'a std::collections::BTreeMap<String, String>,
	pub(crate) emit: &'a Emit<'a>,
}

/// The declaration emit of one selection wave.
pub(crate) type Emit<'a> = dyn Fn(&[P<SourceFile>]) -> Result<Vec<DeclarationEmitFile>, Error> + 'a;

impl DeclarationSignatureSelection<'_> {
	/// `signatureHeld`: the file's re-emitted declaration hashes to the recorded one.
	fn signature_held(&self, source_file: P<SourceFile>, texts: &HashMap<String, &str>) -> bool {
		let key = self.output_key(&(self.declaration_path)(source_file.file_name()));
		if key.is_empty() {
			return false;
		}
		let (Some(previous), Some(text)) = (self.previous_outputs.get(&key), texts.get(&key)) else {
			return false;
		};
		hex_sha256(text.as_bytes()) == *previous
	}

	/// `outputKey`: project-relative and slash-separated.
	pub(crate) fn output_key(&self, path: &str) -> String {
		if self.project_dir.is_empty() || path.is_empty() {
			return String::new();
		}
		let cleaned = gopath::clean(&gopath::from_slash(path));
		gopath::rel(&self.project_dir, &cleaned).map_or_else(
			|_| gopath::to_slash(&cleaned).into_owned(),
			|relative| gopath::to_slash(&relative).into_owned(),
		)
	}
}

/// `selectByDeclarationSignature`: waves of re-emission from the seeds, propagating to importers
/// only past files whose declaration changed.
pub(crate) fn select_by_declaration_signature(
	source_files: &[P<SourceFile>],
	seeds: &BTreeSet<String>,
	current: &IncrementalManifest,
	previous: &IncrementalManifest,
	selection: &DeclarationSignatureSelection<'_>,
) -> Result<(Vec<P<SourceFile>>, Vec<DeclarationEmitFile>), Error> {
	let by_path: HashMap<String, P<SourceFile>> = source_files
		.iter()
		.map(|&file| (normalize_source_file_path(file.file_name()), file))
		.collect();
	let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
	accumulate_reverse_deps(&mut reverse, Some(current));
	accumulate_reverse_deps(&mut reverse, Some(previous));

	let mut selected: HashSet<String> = HashSet::with_capacity(seeds.len());
	let mut pending: Vec<String> = seeds.iter().cloned().collect();
	let mut declarations = Vec::new();
	while !pending.is_empty() {
		let paths = std::mem::take(&mut pending);
		let mut wave = Vec::with_capacity(paths.len());
		for path in paths {
			if !selected.insert(path.clone()) {
				continue;
			}
			match by_path.get(&path) {
				Some(&file) => wave.push(file),
				None => pending.extend(reverse.get(&path).into_iter().flatten().cloned()),
			}
		}
		if wave.is_empty() {
			continue;
		}
		let emitted = (selection.emit)(&wave)?;
		{
			let texts: HashMap<String, &str> = emitted
				.iter()
				.map(|file| (selection.output_key(&file.file_name), file.text.as_str()))
				.collect();
			for &file in &wave {
				if selection.signature_held(file, &texts) {
					continue;
				}
				pending.extend(
					reverse
						.get(&normalize_source_file_path(file.file_name()))
						.into_iter()
						.flatten()
						.cloned(),
				);
			}
		}
		declarations.extend(emitted);
	}
	let result = source_files
		.iter()
		.copied()
		.filter(|file| selected.contains(&normalize_source_file_path(file.file_name())))
		.collect();
	Ok((result, declarations))
}

/// `changedSourcePaths`
pub(crate) fn changed_source_paths(current: &IncrementalManifest, previous: &IncrementalManifest) -> BTreeSet<String> {
	let mut changed = BTreeSet::new();
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
	changed
}

/// What [`narrow_selection_by_declaration_signature`] decided: `None` when the selection stood
/// down and the build uses the closure selection instead.
pub(crate) type Narrowed = Option<(Vec<P<SourceFile>>, Vec<DeclarationEmitFile>)>;

/// `narrowSelectionByDeclarationSignature`
#[expect(clippy::too_many_arguments, reason = "Go's signature")]
pub(crate) fn narrow_selection_by_declaration_signature(
	dir: &str,
	program: &'static Program,
	source_files: &[P<SourceFile>],
	extra_seeds: &HashSet<String>,
	current: &IncrementalManifest,
	previous: Option<&IncrementalManifest>,
	previous_outputs: &std::collections::BTreeMap<String, String>,
	timings: Option<&BuildTimings>,
) -> Narrowed {
	let previous = previous.filter(|previous| previous.salt == current.salt)?;
	if previous_outputs.is_empty() || !program.options().get_emit_declarations() {
		return None;
	}
	let mut seeds = changed_source_paths(current, previous);
	seeds.extend(declaration_importers(&seeds, current, previous));
	seeds.extend(extra_seeds.iter().cloned());
	if seeds.is_empty() {
		return Some((Vec::new(), Vec::new()));
	}
	let options = program.options();
	let declaration_path = |file_name: &str| {
		gopath::from_slash(&outputpaths::get_output_declaration_file_name_worker(
			file_name, &options, program,
		))
		.into_owned()
	};
	let emit = |files: &[P<SourceFile>]| {
		let stop = log_stage_named(&options.config_file_path, Stage::DeclarationEmit.trace_name());
		let declarations = declaration_texts_for_selection(program, files);
		let elapsed = stop();
		if let Some(timings) = timings {
			timings.report_nested_stage(Stage::IncrementalSelection, Stage::DeclarationEmit, elapsed);
		}
		declarations
	};
	let selection = DeclarationSignatureSelection {
		project_dir: gopath::clean(&gopath::from_slash(dir)),
		declaration_path: &declaration_path,
		previous_outputs,
		emit: &emit,
	};
	match select_by_declaration_signature(source_files, &seeds, current, previous, &selection) {
		Ok(selected) => Some(selected),
		Err(error) => {
			logservice::write_line_if_verbose(&[&format!("declaration signature selection stood down: {error}")]);
			None
		}
	}
}

/// `declarationImporters`: changed declaration files and their importer closure.
fn declaration_importers(
	changed: &BTreeSet<String>,
	current: &IncrementalManifest,
	previous: &IncrementalManifest,
) -> BTreeSet<String> {
	let mut affected: BTreeSet<String> = BTreeSet::new();
	let mut pending: VecDeque<String> = VecDeque::new();
	for path in changed {
		if tspath::is_declaration_file_name(path) {
			affected.insert(path.clone());
			pending.push_back(path.clone());
		}
	}
	if pending.is_empty() {
		return affected;
	}
	let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
	accumulate_reverse_deps(&mut reverse, Some(current));
	accumulate_reverse_deps(&mut reverse, Some(previous));
	while let Some(path) = pending.pop_front() {
		for importer in reverse.get(&path).into_iter().flatten() {
			if affected.insert(importer.clone()) {
				pending.push_back(importer.clone());
			}
		}
	}
	affected
}

/// `declarationTextsForSelection`: the declarations of the emittable files.
fn declaration_texts_for_selection(
	program: &'static Program,
	files: &[P<SourceFile>],
) -> Result<Vec<DeclarationEmitFile>, Error> {
	let emittable: Vec<P<SourceFile>> = files
		.iter()
		.copied()
		.filter(|&file| emits_declaration_output(file))
		.collect();
	emit_declaration_texts(program, &emittable)
}

/// `recoveredInputPaths`: the inputs of previous outputs that went missing, so they rebuild.
pub(crate) fn recovered_input_paths(
	dir: &str,
	path_translator: &PathTranslator,
	previous_outputs: &std::collections::BTreeMap<String, String>,
	presence: &OutputPresenceIndex,
) -> HashSet<String> {
	let mut recovered = HashSet::new();
	for output_path in previous_outputs.keys() {
		if presence.has_regular(output_path) {
			continue;
		}
		let absolute_path = gopath::join(&[&gopath::from_slash(dir), &gopath::from_slash(output_path)]);
		let input_path = absolute_path.strip_suffix(".map").unwrap_or(&absolute_path);
		for candidate in path_translator.get_input_paths(input_path) {
			recovered.insert(normalize_source_file_path(&candidate));
		}
	}
	recovered
}
