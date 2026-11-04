//! `oracle_corpus_test.go`

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The repository root, Go's `../..` from `internal/flamework`.
pub(super) fn repo_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Lowercase hexadecimal, Go's `hex.EncodeToString`.
pub(super) fn hex(bytes: &[u8]) -> String {
	bytes.iter().fold(String::new(), |mut out, byte| {
		let _ = write!(out, "{byte:02x}");
		out
	})
}

/// `testdata/flamework-unit/<parts>`
pub(super) fn flamework_testdata(parts: &str) -> PathBuf {
	repo_root().join("testdata/flamework-unit").join(parts)
}

fn walk_files(root: &Path, directory: &Path, out: &mut Vec<String>) {
	for entry in std::fs::read_dir(directory).expect("walk pinned source") {
		let entry = entry.expect("directory entry");
		let path = entry.path();
		if entry.file_type().expect("file type").is_dir() {
			walk_files(root, &path, out);
		} else {
			let relative = path.strip_prefix(root).expect("relative path");
			out.push(rotor_support::gopath::to_slash(&relative.to_string_lossy()).into_owned());
		}
	}
}

#[test]
fn task4_oracle_inventory_accounts_for_pinned_upstream_source() {
	// The pinned source tree and its independently recorded inventory.
	let corpus_root = flamework_testdata("task4");
	let reference_root = repo_root().join("reference/rbxts-transformer-flamework");
	let inventory = std::fs::read_to_string(corpus_root.join("inventory.tsv")).expect("open inventory");
	let records: Vec<Vec<&str>> = inventory.lines().map(|line| line.split('\t').collect()).collect();
	assert!(
		!records.is_empty() && records[0].join("\t") == "sha256\tpath\tfamily\trole",
		"inventory header = {:?}",
		records.first()
	);

	// Every inventory entry is matched to the vendored source bytes.
	let mut inventoried = BTreeSet::new();
	let mut families = BTreeSet::new();
	for record in &records[1..] {
		let [digest, source_path, family, role] = record.as_slice() else {
			panic!("inventory record = {record:?}, want four columns");
		};
		assert!(
			inventoried.insert((*source_path).to_owned()),
			"duplicate inventory path {source_path:?}"
		);
		assert!(
			!family.is_empty() && !role.is_empty(),
			"inventory path {source_path:?} has empty classification"
		);
		let contents = std::fs::read(reference_root.join(source_path)).expect("read source");
		let actual = hex(&Sha256::digest(&contents));
		assert_eq!(actual, *digest, "source {source_path:?} sha256");
		families.insert((*family).to_owned());
	}
	let mut source_paths = Vec::new();
	walk_files(&reference_root, &reference_root.join("src"), &mut source_paths);
	source_paths.sort();

	// All 67 source entries and every required behavior family are explicit.
	assert!(
		source_paths.len() == 67 && inventoried.len() == source_paths.len(),
		"source accounting = {} vendored, {} inventoried, want 67",
		source_paths.len(),
		inventoried.len()
	);
	for source_path in &source_paths {
		assert!(
			inventoried.contains(source_path),
			"vendored source {source_path:?} is not inventoried"
		);
	}
	for family in [
		"class-decorator-di",
		"component-attributes",
		"guards",
		"invalid-diagnostics",
		"macro-groups",
		"macro-paths",
		"networking",
		"package-metadata-options",
	] {
		assert!(
			families.contains(family),
			"required behavior family {family:?} is not inventoried"
		);
	}
}

#[test]
fn task4_oracle_corpus_contains_durable_behavior_artifacts() {
	// The controlled upstream inputs and the committed expected surfaces.
	let root = flamework_testdata("task4");
	let required = [
		"oracle/deterministic-random.cjs",
		"oracle/flamework.build",
		"oracle/flamework.json",
		"oracle/package.json",
		"oracle/tsconfig.json",
		"oracle/src/server/class-decorator-di.server.ts",
		"oracle/src/shared/component-attributes.ts",
		"oracle/src/shared/guards.ts",
		"oracle/src/shared/macros.ts",
		"oracle/src/shared/networking.ts",
		"expected/diagnostics.jsonl",
		"expected/transformed.ts",
		"expected/artifacts/config.json",
		"expected/artifacts/flamework.build",
		"expected/artifacts/globs.json",
		"expected/luau/server/class-decorator-di.server.luau",
		"expected/luau/shared/component-attributes.luau",
		"expected/luau/shared/guards.luau",
		"expected/luau/shared/macros.luau",
		"expected/luau/shared/networking.luau",
	];

	// Every artifact is read from disk.
	let mut artifacts = BTreeMap::new();
	for name in required {
		let contents = std::fs::read_to_string(root.join(name)).expect("read corpus artifact");
		assert!(!contents.is_empty(), "corpus artifact {name:?} is empty");
		artifacts.insert(name, contents);
	}

	// Each requested observable family has a machine-consumed oracle marker.
	let markers: [(&str, &[&str]); 4] = [
		(
			"expected/transformed.ts",
			&[
				r#"Reflect["decorate"]"#,
				r#"Flamework["resolveDependency"]"#,
				"const dedup =",
				"Networking.createEvent",
				r#""instanceGuard""#,
				r#""uuid": "00000000-0000-4000-8000-000000000004""#,
			],
		),
		(
			"expected/diagnostics.jsonl",
			&[
				r#""exitCode":1"#,
				"Path is invalid",
				"template literal which is unsupported",
			],
		),
		(
			"expected/artifacts/flamework.build",
			&[
				r#""flameworkVersion": "1.3.2""#,
				r#""identifierPrefix": "task4""#,
				r#""stringHashes""#,
			],
		),
		(
			"expected/luau/shared/macros.luau",
			&[
				"Flamework._addPaths",
				r#"payloadHash = "00000000-0000-4000-8000-000000000004""#,
			],
		),
	];
	for (name, expected_markers) in markers {
		for marker in expected_markers {
			assert!(
				artifacts[name].contains(marker),
				"corpus artifact {name:?} missing marker {marker:?}"
			);
		}
	}
}
