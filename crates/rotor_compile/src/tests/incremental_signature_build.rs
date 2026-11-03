//! `incremental_signature_build_test.go`: the declaration-signature rule, end to end.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::incremental::emitted_file_bases;
use super::output::{build, patch_out_dir, set_mtime};
use super::vector_math::build_audit_project;
use super::{Temp, write_project};
use crate::ProjectOptions;
use crate::package_layout::hex_sha256;
use crate::timings::{BuildTimings, BuildTimingsReport};

/// `enableDeclarationIncrementalBuilds`
fn enable_declaration_incremental_builds(temp: &Temp) {
	patch_out_dir(
		temp,
		",\n\t\t\"declaration\": true,\n\t\t\"incremental\": true,\n\t\t\"tsBuildInfoFile\": \"out/cache.tsbuildinfo\"",
	);
}

/// `writeSignatureFixture`: `util.ts <- main.ts <- consumer.ts`, plus a file that imports none.
fn write_signature_fixture(temp: &Temp) {
	write_files(
		temp,
		&[
			("src/util.ts", "export const VALUE = 1;\n"),
			(
				"src/main.ts",
				"import { VALUE } from \"./util\";\nexport const main = VALUE;\n",
			),
			(
				"src/consumer.ts",
				"import { main } from \"./main\";\nexport const consumer = main;\n",
			),
			("src/unrelated.ts", "export const unrelated = 1;\n"),
		],
	);
}

/// `writeFiles`
fn write_files(temp: &Temp, files: &[(&str, &str)]) {
	for (rel, text) in files {
		temp.write(rel, text);
	}
}

/// `readOutput`
fn read_output(temp: &Temp, name: &str) -> String {
	fs::read_to_string(temp.join("out").join(name)).unwrap_or_else(|error| panic!("read out/{name}: {error}"))
}

fn walk_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
	for entry in fs::read_dir(dir).expect("the directory reads") {
		let entry = entry.expect("a directory entry");
		if entry.file_type().expect("the entry's type").is_dir() {
			walk_files(&entry.path(), files);
		} else {
			files.push(entry.path());
		}
	}
}

/// `outputTreeHashes`. The build info and the copy-files ledger record the project's absolute
/// path, so they are left out.
fn output_tree_hashes(temp: &Temp) -> BTreeMap<String, String> {
	let out_dir = temp.join("out");
	let mut files = Vec::new();
	walk_files(&out_dir, &mut files);
	files
		.into_iter()
		.filter(|path| {
			let name = path
				.file_name()
				.map(|name| name.to_string_lossy().into_owned())
				.unwrap_or_default();
			!name.ends_with(".tsbuildinfo") && name != "rbxts.copyfiles.json"
		})
		.map(|path| {
			let relative = path
				.strip_prefix(&out_dir)
				.expect("under out")
				.to_string_lossy()
				.replace('\\', "/");
			(relative, hex_sha256(&fs::read(&path).expect("the output reads")))
		})
		.collect()
}

/// `buildForSignatureTest`: the timings report and the emitted files' base names.
fn build_for_signature_test(temp: &Temp) -> (BuildTimingsReport, Vec<String>) {
	let timings = Arc::new(BuildTimings::new());
	let result = build(
		&temp.dir(),
		&ProjectOptions {
			timings: Some(Arc::clone(&timings)),
			..ProjectOptions::default()
		},
	);
	assert!(
		result.diagnostics.is_empty(),
		"build diagnostics: {:?}",
		result.diagnostics
	);
	timings.finish();
	(timings.report(), emitted_file_bases(&result))
}

/// `touchOutputsIntoThePast`
fn touch_outputs_into_the_past(temp: &Temp) {
	let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
	let mut files = Vec::new();
	walk_files(&temp.join("out"), &mut files);
	for path in files {
		set_mtime(&path, old);
	}
}

fn contains(wrote: &[String], name: &str) -> bool {
	wrote.iter().any(|written| written == name)
}

/// A comment-only edit leaves the importers out of the build, and every output lands
/// byte-for-byte where a build from scratch would put it.
#[test]
fn declaration_signature_selection_skips_importers_on_comment_only_edit() {
	let comment_edit = "export const VALUE = 1;\n// a comment nothing can observe\n";

	let temp = write_project("@scope/signature-comment-edit", "");
	enable_declaration_incremental_builds(&temp);
	write_signature_fixture(&temp);
	build_for_signature_test(&temp);
	touch_outputs_into_the_past(&temp);
	temp.write("src/util.ts", comment_edit);
	let (timings, wrote) = build_for_signature_test(&temp);

	assert_eq!(
		timings.counts.selected_sources, 1,
		"selected files, want only the edited one"
	);
	assert!(
		contains(&wrote, "util.luau"),
		"wrote {wrote:?}, expected the edited file to be recompiled"
	);
	assert!(
		timings.stages.declaration_emit_ms >= 0 && timings.stages.incremental_selection_ms >= 0,
		"stage totals went negative: declaration emit {}ms, incremental selection {}ms",
		timings.stages.declaration_emit_ms,
		timings.stages.incremental_selection_ms
	);

	// A build of the same sources from scratch is the oracle.
	let oracle = write_project("@scope/signature-comment-edit-oracle", "");
	enable_declaration_incremental_builds(&oracle);
	write_signature_fixture(&oracle);
	oracle.write("src/util.ts", comment_edit);
	build_for_signature_test(&oracle);

	let narrowed_hashes = output_tree_hashes(&temp);
	let oracle_hashes = output_tree_hashes(&oracle);
	for (path, hash) in &oracle_hashes {
		assert_eq!(
			narrowed_hashes.get(path),
			Some(hash),
			"output {path} differs: full build vs narrowed build"
		);
	}
	assert_eq!(narrowed_hashes.len(), oracle_hashes.len(), "output count");
}

/// An edit that moves the declaration output still has to reach the importers.
#[test]
fn declaration_signature_selection_rebuilds_importers_on_api_change() {
	let temp = write_project("@scope/signature-api-change", "");
	enable_declaration_incremental_builds(&temp);
	write_signature_fixture(&temp);
	build_for_signature_test(&temp);
	touch_outputs_into_the_past(&temp);

	temp.write("src/util.ts", "export const VALUE = \"one\";\n");
	let (timings, wrote) = build_for_signature_test(&temp);
	assert_eq!(
		timings.counts.selected_sources, 3,
		"selected files, want the edited file and both importers"
	);
	for want in ["util.luau", "util.d.ts", "main.d.ts"] {
		assert!(contains(&wrote, want), "wrote {wrote:?}, want it to include {want}");
	}
	assert!(
		!contains(&wrote, "unrelated.luau"),
		"wrote {wrote:?}, want the unrelated file left alone"
	);
}

/// Deleting a file has to rebuild whoever imported it, whatever their own declarations say.
#[test]
fn declaration_signature_selection_rebuilds_importers_of_a_deleted_file() {
	let temp = write_project("@scope/signature-delete", "");
	enable_declaration_incremental_builds(&temp);
	write_signature_fixture(&temp);
	build_for_signature_test(&temp);
	touch_outputs_into_the_past(&temp);

	fs::remove_file(temp.join("src/unrelated.ts")).expect("remove unrelated.ts");
	temp.write("src/main.ts", "export const main = 2;\n");
	let (timings, _) = build_for_signature_test(&temp);
	assert_eq!(
		timings.counts.selected_sources, 2,
		"selected files, want main.ts and its importer consumer.ts"
	);
}

/// A build that selected nothing writes nothing and leaves the tree as it found it.
#[test]
fn declaration_signature_selection_is_a_no_op_without_changes() {
	let temp = write_project("@scope/signature-no-change", "");
	enable_declaration_incremental_builds(&temp);
	write_signature_fixture(&temp);
	build_for_signature_test(&temp);
	let before = output_tree_hashes(&temp);
	touch_outputs_into_the_past(&temp);

	let (timings, wrote) = build_for_signature_test(&temp);
	assert_eq!(
		timings.counts.selected_sources, 0,
		"selected files with nothing changed, want none"
	);
	assert!(wrote.is_empty(), "wrote {wrote:?} with nothing changed, want nothing");
	let after = output_tree_hashes(&temp);
	for (path, hash) in &before {
		assert_eq!(after.get(path), Some(hash), "output {path} changed on a no-op build");
	}
}

#[test]
fn declaration_signature_selection_rebuilds_importers_of_changed_declaration() {
	let caller = "import { values } from \"./tuple\";\nconst [count, text] = values();\nprint(count, text);\n";
	let declaration = "export declare function values(): LuaTuple<[number, string]>;\n";
	let temp = build_audit_project();
	enable_declaration_incremental_builds(&temp);
	write_files(
		&temp,
		&[
			(
				"src/tuple.d.ts",
				"export declare function values(): [number, string];\n",
			),
			(
				"src/tuple.luau",
				"return { values = function() return 1, \"text\" end }\n",
			),
			("src/main.ts", caller),
			("src/unrelated.ts", "export const unrelated = 1;\n"),
		],
	);
	build_for_signature_test(&temp);
	let before = read_output(&temp, "main.luau");
	write_files(&temp, &[("src/tuple.d.ts", declaration)]);
	let (timings, _) = build_for_signature_test(&temp);
	assert_eq!(
		timings.counts.selected_sources, 1,
		"selected sources, want only the declaration importer"
	);

	let oracle = build_audit_project();
	enable_declaration_incremental_builds(&oracle);
	write_files(
		&oracle,
		&[
			("src/tuple.d.ts", declaration),
			(
				"src/tuple.luau",
				"return { values = function() return 1, \"text\" end }\n",
			),
			("src/main.ts", caller),
			("src/unrelated.ts", "export const unrelated = 1;\n"),
		],
	);
	build_for_signature_test(&oracle);
	let want = read_output(&oracle, "main.luau");
	assert!(
		before != want && want.contains("local count, text = values()"),
		"fixture does not distinguish tuple lowering:\ninitial:\n{before}\ncold:\n{want}"
	);
	assert_eq!(
		read_output(&temp, "main.luau"),
		want,
		"declaration-only warm build differs from cold build"
	);
	let (timings, wrote) = build_for_signature_test(&temp);
	assert!(
		timings.counts.selected_sources == 0 && wrote.is_empty(),
		"unchanged declaration build selected {} sources and wrote {wrote:?}",
		timings.counts.selected_sources
	);
}

#[test]
fn declaration_signature_selection_rebuilds_through_declaration_reexports() {
	let temp = build_audit_project();
	enable_declaration_incremental_builds(&temp);
	write_files(
		&temp,
		&[
			(
				"src/tuple.d.ts",
				"export declare function values(): [number, string];\n",
			),
			("src/bridge.d.ts", "export { values } from \"./tuple\";\n"),
			("src/bridge.luau", "return {}\n"),
			(
				"src/main.ts",
				"import { values } from \"./bridge\";\nconst [count, text] = values();\nprint(count, text);\n",
			),
		],
	);
	build_for_signature_test(&temp);
	write_files(
		&temp,
		&[(
			"src/tuple.d.ts",
			"export declare function values(): LuaTuple<[number, string]>;\n",
		)],
	);
	build_for_signature_test(&temp);
	let got = read_output(&temp, "main.luau");
	assert!(
		got.contains("local count, text = values()"),
		"declaration re-export left the caller's tuple lowering stale:\n{got}"
	);
}

#[test]
fn declaration_signature_selection_rebuilds_through_source_reexports() {
	let temp = build_audit_project();
	enable_declaration_incremental_builds(&temp);
	write_files(
		&temp,
		&[
			(
				"src/tuple.d.ts",
				"export declare function values(): [number, string];\n",
			),
			("src/tuple.luau", "return {}\n"),
			("src/bridge.ts", "export { values } from \"./tuple\";\n"),
			(
				"src/main.ts",
				"import { values } from \"./bridge\";\nconst [count, text] = values();\nprint(count, text);\n",
			),
		],
	);
	build_for_signature_test(&temp);
	write_files(
		&temp,
		&[(
			"src/tuple.d.ts",
			"export declare function values(): LuaTuple<[number, string]>;\n",
		)],
	);
	build_for_signature_test(&temp);
	let got = read_output(&temp, "main.luau");
	assert!(
		got.contains("local count, text = values()"),
		"source re-export left the caller's tuple lowering stale:\n{got}"
	);
}

#[test]
fn declaration_signature_selection_rebuilds_for_changed_global_declaration() {
	for (name, declaration) in [
		("script", "declare function values(): RETURN_TYPE;\n"),
		(
			"augmentation",
			"export {}; declare global { function values(): RETURN_TYPE; }\n",
		),
	] {
		let temp = build_audit_project();
		enable_declaration_incremental_builds(&temp);
		write_files(
			&temp,
			&[
				(
					"src/values.d.ts",
					&declaration.replace("RETURN_TYPE", "[number, string]"),
				),
				("src/main.ts", "const [count, text] = values();\nprint(count, text);\n"),
			],
		);
		build_for_signature_test(&temp);
		write_files(
			&temp,
			&[(
				"src/values.d.ts",
				&declaration.replace("RETURN_TYPE", "LuaTuple<[number, string]>"),
			)],
		);
		build_for_signature_test(&temp);
		let got = read_output(&temp, "main.luau");
		assert!(
			got.contains("local count, text = values()"),
			"{name}: global declaration left the caller's tuple lowering stale:\n{got}"
		);
	}
}

#[test]
fn declaration_signature_selection_rebuilds_for_changed_external_declaration() {
	let temp = build_audit_project();
	enable_declaration_incremental_builds(&temp);
	let declaration_path = "node_modules/@rbxts/tuple/index.d.ts";
	temp.write(
		declaration_path,
		"export declare function values(): [number, string];\n",
	);
	write_files(
		&temp,
		&[
			(
				"src/main.ts",
				"import { values } from \"@rbxts/tuple\";\nconst [count, text] = values();\nprint(count, text);\n",
			),
			("src/unrelated.ts", "export const unrelated = 1;\n"),
		],
	);
	build_for_signature_test(&temp);
	temp.write(
		declaration_path,
		"export declare function values(): LuaTuple<[number, string]>;\n",
	);
	let (timings, _) = build_for_signature_test(&temp);
	let got = read_output(&temp, "main.luau");
	assert!(
		got.contains("local count, text = values()"),
		"external declaration left the caller's tuple lowering stale:\n{got}"
	);
	assert_eq!(
		timings.counts.selected_sources, 1,
		"selected sources, want only the external declaration importer"
	);
}

/// `enableJSONModules`
fn enable_json_modules(temp: &Temp) {
	let config = fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
	let updated = config.replacen(
		r#""rootDir": "src""#,
		"\"rootDir\": \"src\",\n\t\t\"resolveJsonModule\": true",
		1,
	);
	temp.write("tsconfig.json", updated);
}

/// A `.json` module's inferred type reaches the importer's declaration, so selection seeds the
/// importers of a changed data file.
#[test]
fn declaration_signature_selection_rebuilds_importers_of_changed_json() {
	let temp = write_project("@scope/signature-json", "");
	enable_json_modules(&temp);
	enable_declaration_incremental_builds(&temp);
	write_files(
		&temp,
		&[
			("src/data.json", "{\n\t\"value\": 1\n}\n"),
			(
				"src/main.ts",
				"import data from \"./data.json\";\nexport const value = data.value;\n",
			),
			("src/other.ts", "export const other = 1;\n"),
		],
	);
	build_for_signature_test(&temp);
	touch_outputs_into_the_past(&temp);

	let luau_before = read_output(&temp, "main.luau");
	write_files(&temp, &[("src/data.json", "{\n\t\"value\": \"text\"\n}\n")]);
	let (timings, _) = build_for_signature_test(&temp);

	assert_eq!(
		timings.counts.selected_sources, 1,
		"selected files, want the importer of the changed data file"
	);
	assert_eq!(
		read_output(&temp, "main.d.ts"),
		"export declare const value: string;\n",
		"declaration, the importer was not rebuilt for the changed JSON"
	);
	// The value is a runtime require of the copied file, so the Luau cannot have moved.
	assert_eq!(
		read_output(&temp, "main.luau"),
		luau_before,
		"a JSON value change moved the importer's Luau"
	);
	assert_eq!(
		read_output(&temp, "data.json"),
		"{\n\t\"value\": \"text\"\n}\n",
		"copied data file"
	);
}

/// With `declarationDir` the rule compares the declarations where they were written.
#[test]
fn declaration_signature_selection_uses_declaration_dir() {
	let temp = write_project("@scope/signature-declaration-dir", "");
	enable_declaration_incremental_builds(&temp);
	let config = fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
	temp.write(
		"tsconfig.json",
		config.replacen(
			"\"declaration\": true,",
			"\"declaration\": true,\n\t\t\"declarationDir\": \"types\",",
			1,
		),
	);
	write_signature_fixture(&temp);
	build_for_signature_test(&temp);
	fs::metadata(temp.join("types/util.d.ts")).expect("types/util.d.ts exists");
	temp.write("src/util.ts", "export const VALUE = 1;\n// comment\n");
	let (timings, _) = build_for_signature_test(&temp);
	assert_eq!(
		timings.counts.selected_sources, 1,
		"selected files, want only the edited one"
	);
}
