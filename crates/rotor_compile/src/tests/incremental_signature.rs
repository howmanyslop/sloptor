//! `incremental_signature_test.go`: the declaration-signature selection rule.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use rotor_support::gopath;
use rotor_tsrs::P;
use rotor_tsrs::ast::{SourceFile, SourceFileParseOptions};
use rotor_tsrs::tscore::ScriptKind;
use rotor_tsrs::tspath;

use crate::compile::Error;
use crate::incremental_manifest::{IncrementalFileState, IncrementalManifest, select_incremental_source_files};
use crate::incremental_signature::{
	DeclarationSignatureSelection, changed_source_paths, select_by_declaration_signature,
};
use crate::output::DeclarationEmitFile;
use crate::outputwriter::OutputWriter;
use crate::package_layout::hex_sha256;

/// `contentHash`
fn content_hash(text: &str) -> String {
	hex_sha256(text.as_bytes())
}

/// `fakeSourceFiles` (`sidecar_roots_test.go`): each name parsed from `export {};`.
fn fake_source_files(names: &[&str]) -> Vec<P<SourceFile>> {
	names
		.iter()
		.map(|name| {
			rotor_tsrs::parser::parse_source_file(
				SourceFileParseOptions {
					file_name: (*name).to_owned(),
					path: tspath::to_path(name, "/", true),
					..SourceFileParseOptions::default()
				},
				"export {};\n",
				ScriptKind::TS,
			)
		})
		.collect()
}

/// `selectedNames`
fn selected_names(files: &[P<SourceFile>]) -> Vec<String> {
	let mut names: Vec<String> = files.iter().map(|file| gopath::base(file.file_name())).collect();
	names.sort();
	names
}

fn path(slash: &str) -> String {
	gopath::from_slash(slash).into_owned()
}

fn state(hash: &str, refs: &[&str]) -> IncrementalFileState {
	IncrementalFileState {
		hash: hash.to_owned(),
		refs: refs.iter().map(|reference| path(reference)).collect(),
	}
}

fn manifest(files: BTreeMap<String, IncrementalFileState>) -> IncrementalManifest {
	IncrementalManifest {
		version: 2,
		salt: "salt".to_owned(),
		files,
		outputs: BTreeMap::new(),
	}
}

/// `signatureFixture`: `main.ts -> util.ts` plus an unrelated `side.ts`, with `util.ts` changed.
fn signature_fixture() -> (IncrementalManifest, IncrementalManifest) {
	let current = manifest(BTreeMap::from([
		(path("/proj/src/main.ts"), state("main-1", &["/proj/src/util.ts"])),
		(path("/proj/src/util.ts"), state("util-2", &[])),
		(path("/proj/src/side.ts"), state("side-1", &[])),
	]));
	let previous = manifest(BTreeMap::from([
		(path("/proj/src/main.ts"), state("main-1", &["/proj/src/util.ts"])),
		(path("/proj/src/util.ts"), state("util-1", &[])),
		(path("/proj/src/side.ts"), state("side-1", &[])),
	]));
	(current, previous)
}

fn base_without_ts(file_name: &str) -> String {
	let base = gopath::base(file_name);
	base.strip_suffix(".ts").unwrap_or(&base).to_owned()
}

/// `signatureSelection`: runs `select_by_declaration_signature` with a fake emitter producing
/// `declaration_text` by base name; returns the selected names, the declarations and the bases
/// the emitter saw.
fn run_selection(
	files: &[P<SourceFile>],
	current: &IncrementalManifest,
	previous: &IncrementalManifest,
	declaration_text: &HashMap<&str, String>,
	recorded: &[(&str, String)],
) -> (Vec<String>, Vec<DeclarationEmitFile>, Vec<String>) {
	let recorded: BTreeMap<String, String> = recorded
		.iter()
		.map(|(key, hash)| ((*key).to_owned(), hash.clone()))
		.collect();
	let emitted = RefCell::new(Vec::new());
	let declaration_path =
		|source_path: &str| gopath::join(&[&path("/proj/out"), &format!("{}.d.ts", base_without_ts(source_path))]);
	let emit = |files: &[P<SourceFile>]| -> Result<Vec<DeclarationEmitFile>, Error> {
		let mut produced = Vec::with_capacity(files.len());
		for file in files {
			let base = base_without_ts(file.file_name());
			emitted.borrow_mut().push(base.clone());
			let Some(text) = declaration_text.get(base.as_str()) else {
				continue;
			};
			produced.push(DeclarationEmitFile {
				file_name: gopath::join(&[&path("/proj/out"), &format!("{base}.d.ts")]),
				text: text.clone(),
			});
		}
		Ok(produced)
	};
	let selection = DeclarationSignatureSelection {
		project_dir: path("/proj"),
		declaration_path: &declaration_path,
		previous_outputs: &recorded,
		emit: &emit,
	};
	let (selected, declarations) = select_by_declaration_signature(
		files,
		&changed_source_paths(current, previous),
		current,
		previous,
		&selection,
	)
	.unwrap_or_else(|error| panic!("{error}"));
	(selected_names(&selected), declarations, emitted.into_inner())
}

fn texts(pairs: &[(&'static str, &str)]) -> HashMap<&'static str, String> {
	pairs.iter().map(|(base, text)| (*base, (*text).to_owned())).collect()
}

#[test]
fn select_by_declaration_signature_stops_at_unchanged_declaration() {
	let (current, previous) = signature_fixture();
	let files = fake_source_files(&["/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts"]);
	let declarations = texts(&[
		("main", "declare main;"),
		("util", "export declare const VALUE: number;"),
	]);
	let recorded = [
		("out/util.d.ts", content_hash(&declarations["util"])),
		("out/main.d.ts", content_hash(&declarations["main"])),
	];

	// The rule this replaces pulls in every transitive importer of the edited file.
	assert_eq!(
		selected_names(&select_incremental_source_files(&files, &current, Some(&previous))),
		["main.ts", "util.ts"],
		"closure rule selected, want main.ts and util.ts"
	);

	let (selected, reused, emitted) = run_selection(&files, &current, &previous, &declarations, &recorded);
	assert_eq!(
		selected,
		["util.ts"],
		"selected, want only util.ts (main.ts imports it but its declaration held)"
	);
	assert_eq!(emitted, ["util"], "emitted declarations, want only util");
	assert_eq!(reused.len(), 1, "reused declarations, want the one that was emitted");
}

#[test]
fn select_by_declaration_signature_propagates_on_changed_declaration() {
	let (current, previous) = signature_fixture();
	let files = fake_source_files(&["/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts"]);
	let declarations = texts(&[
		("main", "declare main;"),
		("util", "export declare const VALUE: string;"),
	]);
	let recorded = [
		("out/util.d.ts", content_hash("export declare const VALUE: number;")),
		("out/main.d.ts", content_hash(&declarations["main"])),
	];

	let (selected, _, _) = run_selection(&files, &current, &previous, &declarations, &recorded);
	assert_eq!(
		selected,
		["main.ts", "util.ts"],
		"selected, want util.ts and its importer main.ts"
	);
}

/// A declaration nobody recorded a hash for cannot be shown to have held, so it propagates.
#[test]
fn select_by_declaration_signature_propagates_without_recorded_hash() {
	let (current, previous) = signature_fixture();
	let files = fake_source_files(&["/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts"]);
	let declarations = texts(&[
		("main", "declare main;"),
		("util", "export declare const VALUE: number;"),
	]);

	let (selected, _, _) = run_selection(&files, &current, &previous, &declarations, &[]);
	assert_eq!(
		selected,
		["main.ts", "util.ts"],
		"selected, want the whole closure when no signature was recorded"
	);
}

/// A file that emits no declaration at all also cannot be shown to have held.
#[test]
fn select_by_declaration_signature_propagates_without_emitted_declaration() {
	let (current, previous) = signature_fixture();
	let files = fake_source_files(&["/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts"]);
	let recorded = [("out/util.d.ts", content_hash("export declare const VALUE: number;"))];

	let (selected, _, _) = run_selection(&files, &current, &previous, &HashMap::new(), &recorded);
	assert_eq!(
		selected,
		["main.ts", "util.ts"],
		"selected, want the whole closure when nothing was emitted"
	);
}

/// A deleted dependency emits nothing to compare, so its importers are seeded directly.
#[test]
fn select_by_declaration_signature_seeds_importers_of_deleted_files() {
	let (mut current, previous) = signature_fixture();
	current.files.remove(&path("/proj/src/util.ts"));
	let files = fake_source_files(&["/proj/src/main.ts", "/proj/src/side.ts"]);
	let declarations = texts(&[("main", "declare main;")]);
	let recorded = [("out/main.d.ts", content_hash(&declarations["main"]))];

	let (selected, _, _) = run_selection(&files, &current, &previous, &declarations, &recorded);
	assert_eq!(selected, ["main.ts"], "selected, want the deleted file's importer");
}

/// A changed declaration keeps propagating past the first importer.
#[test]
fn select_by_declaration_signature_propagates_transitively() {
	let (mut current, mut previous) = signature_fixture();
	current
		.files
		.insert(path("/proj/src/side.ts"), state("side-1", &["/proj/src/main.ts"]));
	previous
		.files
		.insert(path("/proj/src/side.ts"), state("side-1", &["/proj/src/main.ts"]));
	let files = fake_source_files(&["/proj/src/main.ts", "/proj/src/util.ts", "/proj/src/side.ts"]);
	let declarations = texts(&[
		("main", "export declare const main: string;"),
		("util", "export declare const VALUE: string;"),
		("side", "export declare const side: string;"),
	]);
	let recorded = [
		("out/util.d.ts", content_hash("export declare const VALUE: number;")),
		("out/main.d.ts", content_hash("export declare const main: number;")),
		("out/side.d.ts", content_hash(&declarations["side"])),
	];

	let (selected, _, _) = run_selection(&files, &current, &previous, &declarations, &recorded);
	assert_eq!(
		selected,
		["main.ts", "side.ts", "util.ts"],
		"selected, want the whole chain once each declaration moved"
	);
}

#[test]
#[ignore = "a Rust DeclarationSignatureSelection cannot lack its emitter or path mapping (non-optional references), so Go's nil guard has no counterpart"]
fn select_by_declaration_signature_rejects_an_incomplete_selection() {}

#[test]
#[ignore = "needs a swappable selection emit (Go's declarationTextsForSelection variable)"]
fn narrow_selection_stands_down_when_the_selection_emit_fails() {}

/// The rule looks a declaration up by the key the writer recorded it under, so the two have to
/// agree character for character.
#[test]
fn signature_output_key_matches_the_writer_key() {
	let project_dir = gopath::clean(&path("D:/proj"));
	let mut writer = OutputWriter::new();
	writer.project_dir.clone_from(&project_dir);
	let no_path = |_: &str| String::new();
	let no_emit = |_: &[P<SourceFile>]| -> Result<Vec<DeclarationEmitFile>, Error> { Ok(Vec::new()) };
	let no_outputs = BTreeMap::new();
	let selection = DeclarationSignatureSelection {
		project_dir,
		declaration_path: &no_path,
		previous_outputs: &no_outputs,
		emit: &no_emit,
	};

	for slash_path in [
		"D:/proj/out/util.d.ts",
		"D:\\proj\\out\\util.d.ts",
		"D:\\proj\\out\\nested/deep.d.ts",
		"D:/proj/out/./util.d.ts",
		// A different drive-letter case is a different path to `filepath.Rel`, so both sides fall
		// back the same way.
		"d:\\proj\\out\\util.d.ts",
	] {
		assert_eq!(
			selection.output_key(slash_path),
			writer.output_key(&path(slash_path)),
			"outputKey({slash_path:?})"
		);
	}
}
