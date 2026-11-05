//! `sloptor` against the frozen output of the pinned roblox-ts fork (`internal/forkparity`).
//!
//! The fork's transformer goldens and project transcripts were captured from `roblox-ts.zip`
//! (`@isentinel/roblox-ts@4.0.11`) and are frozen under `testdata/forkparity`. Every matrix case
//! has one row in `divergence-ledger.json` saying whether sloptor must match the fork or why it
//! does not. An `upstream-corrected` row may differ in bytes only once its named runtime test
//! passes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::{first_difference, has_extension, read_tree, repo_root, run_sloptor, runtime, symlink_dir, write_file};

/// The SHA-256 of the committed `roblox-ts.zip`.
const ZIP_DIGEST: &str = "a0f097baefaa02ae411f18ea19a150c0076bfd2adbc6645dcef02e2c162cea5a";
/// The package the archive holds.
const COMPILER_VERSION: &str = "@isentinel/roblox-ts@4.0.11";

const CLASSIFICATIONS: [&str; 4] = [
	"fork-authoritative",
	"rbxtsc-3.0-compatible",
	"upstream-corrected",
	"go-inapplicable",
];

/// The tsconfig every transformer fixture compiles under (`matrixTSConfig`).
const TRANSFORMER_TS_CONFIG: &str = r#"{
  "compilerOptions": {
    "allowSyntheticDefaultImports": true,
    "downlevelIteration": true,
    "module": "commonjs",
    "moduleDetection": "force",
    "moduleResolution": "Node",
    "noLib": true,
    "outDir": "out",
    "rootDir": "src",
    "strict": true,
    "target": "ESNext",
    "typeRoots": ["node_modules/@rbxts"]
  },
  "include": ["src"]
}
"#;

fn read_json(path: &Path) -> Value {
	let text = fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
	serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn string_field(value: &Value, field: &str) -> String {
	value.get(field).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn string_list(value: &Value, field: &str) -> Vec<String> {
	value
		.get(field)
		.and_then(Value::as_array)
		.map(|items| items.iter().filter_map(Value::as_str).map(str::to_owned).collect())
		.unwrap_or_default()
}

fn forkparity_dir() -> std::path::PathBuf {
	repo_root().join("testdata/forkparity")
}

/// One archive-captured transformer probe.
struct TransformerFixture {
	name: String,
	ts_code: String,
	expected_luau: String,
	diagnostics: Vec<String>,
}

fn transformer_fixtures() -> Vec<TransformerFixture> {
	let dir = forkparity_dir().join("transformer");
	let manifest = read_json(&dir.join("manifest.json"));
	let entries = manifest
		.get("fixtures")
		.and_then(Value::as_array)
		.cloned()
		.unwrap_or_default();
	entries
		.iter()
		.map(|entry| {
			let name = string_field(entry, "name");
			let read = |extension: &str| {
				let path = dir.join(format!("{name}.{extension}"));
				fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
			};
			TransformerFixture {
				ts_code: read("ts"),
				expected_luau: read("luau"),
				diagnostics: string_list(entry, "diagnostics"),
				name,
			}
		})
		.collect()
}

/// One archive-captured project tree and its transcript.
struct ProjectFixture {
	name: String,
	files: BTreeMap<String, Vec<u8>>,
	transcript: Value,
}

impl ProjectFixture {
	fn expected_exit_code(&self) -> i32 {
		self.transcript
			.get("expectedExitCode")
			.and_then(Value::as_i64)
			.and_then(|code| i32::try_from(code).ok())
			.unwrap_or_default()
	}

	fn checks(&self) -> &Value {
		self.transcript.get("artifactChecks").unwrap_or(&Value::Null)
	}

	fn invocations(&self) -> Vec<Vec<String>> {
		self.transcript
			.get("invocations")
			.and_then(Value::as_array)
			.map(|items| items.iter().map(|item| string_list(item, "arguments")).collect())
			.unwrap_or_default()
	}

	/// `matrixProjectArgs`: `--build` when the fork was run with it or the project has references.
	fn build_args(&self) -> Vec<&'static str> {
		let references = self
			.files
			.get("tsconfig.json")
			.is_some_and(|text| String::from_utf8_lossy(text).contains("\"references\""));
		if references
			|| self
				.invocations()
				.iter()
				.any(|args| args.iter().any(|arg| arg == "--build"))
		{
			vec!["--build"]
		} else {
			Vec::new()
		}
	}
}

fn project_fixtures() -> Vec<ProjectFixture> {
	let dir = forkparity_dir().join("project");
	string_list(&read_json(&dir.join("manifest.json")), "fixtures")
		.into_iter()
		.map(|name| ProjectFixture {
			files: read_tree(&dir.join(&name).join("tree"), &|_| true),
			transcript: read_json(&dir.join(&name).join("transcript.json")),
			name,
		})
		.collect()
}

/// One row of `divergence-ledger.json`.
#[derive(Clone, Debug, Default)]
struct LedgerRow {
	id: String,
	classification: String,
	surface: String,
	contract: String,
	verification: String,
	behavioral_test: String,
	implementation_detail: String,
	reason: String,
}

struct Ledger {
	schema_version: i64,
	zip_digest: String,
	rows: Vec<LedgerRow>,
}

fn read_ledger() -> Ledger {
	let ledger = read_json(&forkparity_dir().join("divergence-ledger.json"));
	let rows = ledger
		.get("rows")
		.and_then(Value::as_array)
		.cloned()
		.unwrap_or_default();
	Ledger {
		schema_version: ledger.get("schemaVersion").and_then(Value::as_i64).unwrap_or_default(),
		zip_digest: string_field(&ledger, "zipDigest"),
		rows: rows
			.iter()
			.map(|row| LedgerRow {
				id: string_field(row, "id"),
				classification: string_field(row, "classification"),
				surface: string_field(row, "surface"),
				contract: string_field(row, "contract"),
				verification: string_field(row, "verification"),
				behavioral_test: string_field(row, "behavioralTest"),
				implementation_detail: string_field(row, "implementationDetail"),
				reason: string_field(row, "reason"),
			})
			.collect(),
	}
}

impl Ledger {
	/// `DivergenceLedger.Validate`: every matrix case has exactly one complete row, and every
	/// upstream-corrected row names a runtime test that exists.
	fn validate(&self, expected_ids: &[String]) -> Result<(), String> {
		if self.schema_version != 1 {
			return Err(format!("ledger schema version = {}, want 1", self.schema_version));
		}
		if self.zip_digest != ZIP_DIGEST {
			return Err(format!(
				"ledger zip digest = {:?}, want {ZIP_DIGEST:?}",
				self.zip_digest
			));
		}
		let expected: BTreeSet<&str> = expected_ids.iter().map(String::as_str).collect();
		let mut seen = BTreeSet::new();
		for row in &self.rows {
			let id = row.id.as_str();
			if id.is_empty() {
				return Err("ledger row has an empty id".to_owned());
			}
			if !expected.contains(id) {
				return Err(format!("ledger row {id:?} does not identify a matrix case"));
			}
			if !seen.insert(id) {
				return Err(format!("ledger has duplicate row {id:?}"));
			}
			if !CLASSIFICATIONS.contains(&row.classification.as_str()) {
				return Err(format!(
					"ledger row {id:?} has invalid classification {:?}",
					row.classification
				));
			}
			if row.surface.is_empty() || row.contract.is_empty() || row.verification.is_empty() {
				return Err(format!(
					"ledger row {id:?} is missing surface, contract, or verification"
				));
			}
			if row.classification == "upstream-corrected" {
				if row.behavioral_test.is_empty() || row.reason.is_empty() {
					return Err(format!(
						"upstream-corrected ledger row {id:?} requires a behavioral test and reason"
					));
				}
				if !runtime::is_behavioral_test(&row.behavioral_test) {
					return Err(format!(
						"upstream-corrected ledger row {id:?} names {:?}, which is not a runtime test of rotor_cli's forkparity suite",
						row.behavioral_test
					));
				}
			}
			if row.classification == "go-inapplicable" {
				if row.implementation_detail.is_empty() {
					return Err(format!(
						"inapplicable ledger row {id:?} is missing implementation detail"
					));
				}
				if row.reason.is_empty() {
					return Err(format!("inapplicable ledger row {id:?} is missing reason"));
				}
			}
		}
		if let Some(missing) = expected.difference(&seen).next() {
			return Err(format!("matrix case {missing:?} is unclassified"));
		}
		Ok(())
	}
}

/// `matrixCaseIDs`: the fixtures plus the reference rows that have no fixture of their own.
fn matrix_case_ids(transformers: &[TransformerFixture], projects: &[ProjectFixture]) -> Vec<String> {
	let mut ids: Vec<String> = transformers
		.iter()
		.map(|fixture| format!("transformer/{}", fixture.name))
		.chain(projects.iter().map(|fixture| format!("project/{}", fixture.name)))
		.collect();
	ids.extend(
		[
			"oracle-execution/transformer-fixtures",
			"oracle-execution/project-fixtures",
			"runtime/lune-execution",
			"watch/chokidar-scheduling",
			"plugin/node-language-service-trace",
			"write/node-fs-call-order",
			"rbxtsc-3.0/differential-fixtures",
			"rbxtsc-3.0/conformance-fixtures",
			"differential/26_stringmacros",
			"differential/29_iteration",
			"differential/31_mixed3b",
			"conformance/tests/array.spec.luau",
			"conformance/tests/class.spec.luau",
			"conformance/tests/decorator.spec.luau",
			"conformance/tests/destructure.spec.luau",
			"conformance/tests/string.spec.luau",
			"conformance/tests/switch.spec.luau",
			"conformance/tests/template.spec.luau",
		]
		.map(str::to_owned),
	);
	ids
}

/// One observable difference from the fork.
#[derive(Debug)]
struct Drift {
	surface: &'static str,
	detail: String,
}

/// `surfaceForArtifact`
fn surface_for_artifact(path: &str) -> &'static str {
	if path.ends_with(".luau.map") {
		"source-map"
	} else if path.ends_with(".d.ts") {
		"declaration"
	} else if path.contains("copyfiles") || path.contains("rojocache") || path.ends_with(".tsbuildinfo") {
		"cache"
	} else {
		"byte"
	}
}

/// `matrixPlainOutput`: a run's output with colour removed and sloptor's `error[TS2322]:` code-frame
/// header spelled the fork's way, `error TS2322:`. Codes and messages still compare exactly.
fn plain_output(output: &str) -> String {
	let mut plain = String::with_capacity(output.len());
	let mut chars = output.chars().peekable();
	while let Some(c) = chars.next() {
		if c == '\x1b' && chars.peek() == Some(&'[') {
			chars.next();
			while chars.next_if(|c| ('0'..='?').contains(c)).is_some() {}
			while chars.next_if(|c| (' '..='/').contains(c)).is_some() {}
			chars.next_if(|c| ('@'..='~').contains(c));
			continue;
		}
		plain.push(c);
	}
	let mut out = String::with_capacity(plain.len());
	let mut rest = plain.as_str();
	while let Some(at) = rest.find("[TS") {
		let (head, tail) = rest.split_at(at);
		let keyword = ["error", "warning"].into_iter().find(|keyword| {
			head.strip_suffix(keyword)
				.is_some_and(|before| !before.ends_with(|c: char| c.is_alphanumeric() || c == '_'))
		});
		let digits = tail[3..].chars().take_while(char::is_ascii_digit).count();
		let close = 3 + digits;
		out.push_str(head);
		if keyword.is_some() && digits > 0 && tail[close..].starts_with("]:") {
			out.push(' ');
			out.push_str(&tail[1..close]);
			out.push(':');
			rest = &tail[close + 2..];
		} else {
			out.push('[');
			rest = &tail[1..];
		}
	}
	out.push_str(rest);
	out
}

/// `transformerFixtureDrifts`
fn transformer_drifts(
	fixture: &TransformerFixture,
	code: i32,
	output: &str,
	out_tree: &BTreeMap<String, Vec<u8>>,
) -> Vec<Drift> {
	let mut drifts = Vec::new();
	let plain = plain_output(output);
	if !fixture.diagnostics.is_empty() {
		if code == 0 {
			drifts.push(Drift {
				surface: "exit-status",
				detail: "exit code = 0, want a diagnostic failure".to_owned(),
			});
		}
		if !out_tree.is_empty() {
			drifts.push(Drift {
				surface: "byte",
				detail: "diagnostic fixture emitted output".to_owned(),
			});
		}
		for expected in &fixture.diagnostics {
			if !plain.contains(expected.as_str()) {
				drifts.push(Drift {
					surface: "diagnostic",
					detail: format!("diagnostics lack {expected:?}:\n{plain}"),
				});
			}
		}
		return drifts;
	}
	if code != 0 {
		drifts.push(Drift {
			surface: "exit-status",
			detail: format!("exit code = {code}, want 0"),
		});
		drifts.push(Drift {
			surface: "diagnostic",
			detail: plain,
		});
	}
	let luau: BTreeMap<&str, &[u8]> = out_tree
		.iter()
		.filter(|(path, _)| has_extension(path, "luau"))
		.map(|(path, contents)| (path.as_str(), contents.as_slice()))
		.collect();
	match luau.get("main.luau") {
		None => drifts.push(Drift {
			surface: "byte",
			detail: "out/main.luau missing from actual result".to_owned(),
		}),
		Some(got) if *got != fixture.expected_luau.as_bytes() => drifts.push(Drift {
			surface: "byte",
			detail: format!(
				"out/main.luau bytes differ: {}",
				first_difference(&String::from_utf8_lossy(got), &fixture.expected_luau)
			),
		}),
		Some(_) => {}
	}
	for path in luau.keys().filter(|path| **path != "main.luau") {
		drifts.push(Drift {
			surface: "byte",
			detail: format!("out/{path} exists only in actual result"),
		});
	}
	drifts
}

/// Writes `files` into `dir` and links the shared `node_modules` beside every tsconfig.
fn stage(dir: &Path, files: &BTreeMap<String, Vec<u8>>) {
	let node_modules = repo_root().join("testdata/transformers/project/node_modules");
	assert!(
		node_modules.join("typescript/package.json").is_file(),
		"install testdata/transformers/project's dependencies (bun install --no-save) first"
	);
	let mut project_dirs = BTreeSet::from([dir.to_path_buf()]);
	for (path, contents) in files {
		assert!(
			!path.starts_with('/') && !path.split('/').any(|part| part == ".."),
			"invalid fixture path {path:?}"
		);
		let full = dir.join(path);
		write_file(&full, contents);
		if path.ends_with("tsconfig.json")
			&& let Some(parent) = full.parent()
		{
			project_dirs.insert(parent.to_path_buf());
		}
	}
	for project_dir in project_dirs {
		symlink_dir(&node_modules, &project_dir.join("node_modules"));
	}
}

/// `matrixArtifactPath`: the outputs and caches a project build owns.
fn artifact_path(path: &str) -> bool {
	let parts: Vec<&str> = path.split('/').collect();
	parts.iter().enumerate().any(|(index, part)| {
		*part == "out" || *part == "include" || (*part == ".rotor" && parts.get(index + 1) == Some(&"cache"))
	})
}

/// `matrixWriteTrace`: what a second build created, removed or rewrote.
fn write_trace(before: &BTreeMap<String, Vec<u8>>, after: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
	let paths: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
	paths
		.into_iter()
		.filter_map(|path| match (before.get(path), after.get(path)) {
			(None, Some(_)) => Some(format!("create {path}")),
			(Some(_), None) => Some(format!("remove {path}")),
			(Some(old), Some(new)) if old != new => Some(format!("update {path}")),
			_ => None,
		})
		.collect()
}

/// `matrixSourceMapValid`: some `.luau.map` is a version 3 map with content and mappings.
fn source_map_valid(artifacts: &BTreeMap<String, Vec<u8>>) -> bool {
	artifacts.iter().any(|(path, contents)| {
		path.ends_with(".luau.map")
			&& serde_json::from_slice::<Value>(contents).is_ok_and(|map| {
				map.get("version").and_then(Value::as_i64) == Some(3)
					&& map
						.get("sourcesContent")
						.and_then(Value::as_array)
						.is_some_and(|items| !items.is_empty())
					&& map
						.get("mappings")
						.and_then(Value::as_str)
						.is_some_and(|mappings| !mappings.is_empty())
			})
	})
}

/// `projectFixtureDrifts`
fn project_drifts(
	fixture: &ProjectFixture,
	code: i32,
	output: &str,
	artifacts: &BTreeMap<String, Vec<u8>>,
) -> Vec<Drift> {
	let mut drifts = Vec::new();
	let combined = plain_output(output);
	let expected_code = fixture.expected_exit_code();
	if code != expected_code {
		drifts.push(Drift {
			surface: "exit-status",
			detail: format!("exit code = {code}, want {expected_code}"),
		});
		drifts.push(Drift {
			surface: "diagnostic",
			detail: combined.clone(),
		});
	}
	for field in ["expectedStdout", "expectedStderr"] {
		let expected = string_field(&fixture.transcript, field);
		if !expected.is_empty() && !combined.contains(&expected) {
			drifts.push(Drift {
				surface: "diagnostic",
				detail: format!("output lacks {field} {expected:?}:\n{combined}"),
			});
		}
	}
	if expected_code != 0 {
		return drifts;
	}
	let checks = fixture.checks();
	let has_suffix = |suffix: &str| artifacts.keys().any(|path| path.ends_with(suffix));
	let has_text = |check: &Value| {
		let suffix = string_field(check, "suffix");
		let text = string_field(check, "text");
		artifacts
			.iter()
			.any(|(path, contents)| path.ends_with(&suffix) && String::from_utf8_lossy(contents).contains(&text))
	};
	if checks.get("requireLuau").and_then(Value::as_bool) == Some(true) && !has_suffix(".luau") {
		drifts.push(Drift {
			surface: "byte",
			detail: "successful project emitted no Luau artifact".to_owned(),
		});
	}
	for suffix in string_list(checks, "requiredSuffixes") {
		if !has_suffix(&suffix) {
			drifts.push(Drift {
				surface: surface_for_artifact(&suffix),
				detail: format!("required artifact suffix missing: {suffix}"),
			});
		}
	}
	for component in string_list(checks, "requiredComponents") {
		if !artifacts.keys().any(|path| path.contains(&component)) {
			drifts.push(Drift {
				surface: "cache",
				detail: format!("required artifact component missing: {component}"),
			});
		}
	}
	let text_checks = |field: &str| checks.get(field).and_then(Value::as_array).cloned().unwrap_or_default();
	for check in text_checks("requiredTexts") {
		if !has_text(&check) {
			drifts.push(Drift {
				surface: surface_for_artifact(&string_field(&check, "suffix")),
				detail: format!("required artifact text missing: {}", string_field(&check, "text")),
			});
		}
	}
	for check in text_checks("forbiddenTexts") {
		if has_text(&check) {
			drifts.push(Drift {
				surface: surface_for_artifact(&string_field(&check, "suffix")),
				detail: format!("forbidden artifact text present: {}", string_field(&check, "text")),
			});
		}
	}
	if checks.get("requireSourceMap").and_then(Value::as_bool) == Some(true) && !source_map_valid(artifacts) {
		drifts.push(Drift {
			surface: "source-map",
			detail: "source-map fixture emitted no valid Luau source map".to_owned(),
		});
	}
	drifts
}

/// `runTransformerFixture`
fn run_transformer_fixture(fixture: &TransformerFixture) -> Vec<Drift> {
	let temp = tempfile::Builder::new()
		.prefix("forkparity-matrix-")
		.tempdir()
		.expect("a temp dir");
	let dir = rotor_support::fsutil::real_path(temp.path()).expect("a canonical temp dir");
	let mut files = BTreeMap::from([
		(
			"package.json".to_owned(),
			b"{\"name\":\"@forkparity/transformer\"}\n".to_vec(),
		),
		("tsconfig.json".to_owned(), TRANSFORMER_TS_CONFIG.as_bytes().to_vec()),
		("src/main.ts".to_owned(), fixture.ts_code.as_bytes().to_vec()),
	]);
	match fixture.name.as_str() {
		"shared-table-iteration" => {
			files.insert(
				"src/sharedtable.d.ts".to_owned(),
				b"interface SharedTable extends Iterable<[string | number, unknown]> {}\n".to_vec(),
			);
		}
		"range-literal-step" => {
			files.insert(
				"src/globals.d.ts".to_owned(),
				b"declare function print(...args: unknown[]): void;\n".to_vec(),
			);
		}
		_ => {}
	}
	stage(&dir, &files);
	let dir_arg = dir.to_string_lossy().into_owned();
	let output = run_sloptor(&dir, &["build", "--project", &dir_arg]);
	let out_tree = read_tree(&dir.join("out"), &|_| true);
	transformer_drifts(fixture, output.code, &output.combined(), &out_tree)
}

/// `runProjectFixture`: one build, and for `build-no-change` a second that must write nothing.
fn run_project_fixture(fixture: &ProjectFixture) -> Vec<Drift> {
	let temp = tempfile::Builder::new()
		.prefix("forkparity-matrix-")
		.tempdir()
		.expect("a temp dir");
	let dir = rotor_support::fsutil::real_path(temp.path()).expect("a canonical temp dir");
	stage(&dir, &fixture.files);
	let dir_arg = dir.to_string_lossy().into_owned();
	let mut args = vec!["build"];
	args.extend(fixture.build_args());
	args.extend(["--project", dir_arg.as_str()]);
	let output = run_sloptor(&dir, &args);
	let artifacts = read_tree(&dir, &artifact_path);
	let mut drifts = project_drifts(fixture, output.code, &output.combined(), &artifacts);
	if fixture.name == "build-no-change" && drifts.is_empty() {
		let second = run_sloptor(&dir, &args);
		let after = read_tree(&dir, &artifact_path);
		if second.code != fixture.expected_exit_code() {
			drifts.push(Drift {
				surface: "exit-status",
				detail: format!("second build exit code = {}:\n{}", second.code, second.combined()),
			});
		}
		let trace = write_trace(&artifacts, &after);
		let want = string_list(fixture.checks(), "expectedWriteTrace");
		if trace != want {
			drifts.push(Drift {
				surface: "write-trace",
				detail: format!("no-change write trace = {trace:?}, want {want:?}"),
			});
		}
	}
	drifts
}

/// The archive's digest and the provenance every frozen corpus records.
#[test]
fn archive_digest_and_provenance_match() {
	let root = repo_root();
	let archive = fs::read(root.join("roblox-ts.zip")).expect("roblox-ts.zip reads");
	let mut digest = String::new();
	for byte in Sha256::digest(&archive) {
		let _ = write!(digest, "{byte:02x}");
	}
	assert_eq!(digest, ZIP_DIGEST, "roblox-ts.zip digest");
	for kind in ["transformer", "project"] {
		let provenance = read_json(&forkparity_dir().join(kind).join("provenance.json"));
		assert_eq!(string_field(&provenance, "zipDigest"), ZIP_DIGEST, "{kind} provenance");
		assert_eq!(
			string_field(&provenance, "compilerVersion"),
			COMPILER_VERSION,
			"{kind} provenance"
		);
		assert!(
			!string_field(&provenance, "captureCommand").is_empty(),
			"{kind} provenance capture command"
		);
	}
}

#[test]
fn ledger_covers_every_matrix_case() {
	let ids = matrix_case_ids(&transformer_fixtures(), &project_fixtures());
	if let Err(error) = read_ledger().validate(&ids) {
		panic!("{error}");
	}
}

#[test]
fn ledger_rejects_an_unknown_case_and_a_go_behavioral_test() {
	let row = LedgerRow {
		id: "unknown/case".to_owned(),
		classification: "fork-authoritative".to_owned(),
		surface: "byte".to_owned(),
		contract: "unknown fixture".to_owned(),
		verification: "not applicable".to_owned(),
		..LedgerRow::default()
	};
	let ledger = Ledger {
		schema_version: 1,
		zip_digest: ZIP_DIGEST.to_owned(),
		rows: vec![row.clone()],
	};
	assert!(ledger.validate(&[]).is_err(), "an unknown matrix case validated");

	let corrected = LedgerRow {
		id: "differential/26_stringmacros".to_owned(),
		classification: "upstream-corrected".to_owned(),
		behavioral_test: "TestPinnedCompatibility/loops".to_owned(),
		reason: "the fork's loop is wrong".to_owned(),
		..row
	};
	let ledger = Ledger {
		schema_version: 1,
		zip_digest: ZIP_DIGEST.to_owned(),
		rows: vec![corrected],
	};
	let error = ledger
		.validate(&["differential/26_stringmacros".to_owned()])
		.expect_err("a Go test name validated");
	assert!(error.contains("not a runtime test"), "{error}");
}

/// `TestProjectFixtures_coverForkScenarios` and the invocation counts of
/// `TestProjectFixtureManifest`: the corpus still holds the scenarios the matrix is for.
#[test]
fn project_fixtures_cover_the_fork_scenarios() {
	let fixtures = project_fixtures();
	let names: Vec<&str> = fixtures.iter().map(|fixture| fixture.name.as_str()).collect();
	assert_eq!(
		names,
		[
			"build-basic",
			"build-declarations",
			"build-no-change",
			"cross-project-dts",
			"per-project-rojo",
			"transformer-declarations",
			"transformer-ordering",
			"transformer-sourcemap",
			"diagnostics",
			"duplicate-output",
		]
	);
	let by_name: BTreeMap<&str, &ProjectFixture> = fixtures
		.iter()
		.map(|fixture| (fixture.name.as_str(), fixture))
		.collect();
	for (name, file, want) in [
		("build-declarations", "tsconfig.json", "\"declaration\": true"),
		("build-no-change", "tsconfig.json", "\"composite\": true"),
		("cross-project-dts", "tsconfig.json", "\"./lib\""),
		(
			"per-project-rojo",
			"custom-pkg/tsconfig.json",
			"\"rojo\":\"./custom.project.json\"",
		),
		(
			"transformer-declarations",
			"tsconfig.json",
			"\"afterDeclarations\": true",
		),
		("transformer-ordering", "tsconfig.json", "\"after\": true"),
		("transformer-sourcemap", "tsconfig.json", "\"sourceMap\": true"),
		("diagnostics", "", "TS2322"),
		("duplicate-output", "", "duplicate output path detected"),
	] {
		let fixture = by_name[name];
		let contents = if file.is_empty() {
			format!(
				"{}{}",
				string_field(&fixture.transcript, "expectedStdout"),
				string_field(&fixture.transcript, "expectedStderr")
			)
		} else {
			String::from_utf8_lossy(&fixture.files[file]).into_owned()
		};
		assert!(contents.contains(want), "{name}: {file} lacks {want:?}");
		if file.is_empty() {
			assert_ne!(fixture.expected_exit_code(), 0, "{name} must fail");
		}
	}
	for fixture in &fixtures {
		let invocations = fixture.invocations();
		let want = if fixture.name == "build-no-change" { 2 } else { 1 };
		assert_eq!(invocations.len(), want, "{} invocations", fixture.name);
		for arguments in invocations {
			assert!(
				arguments.iter().any(|arg| arg == "--project"),
				"{}: {arguments:?}",
				fixture.name
			);
		}
	}
}

/// `TestPerformanceOutputFixture`: the frozen performance-output corpus still describes a clean
/// build, and its sources keep the BOM and CRLF line endings it exists to exercise.
#[test]
fn performance_output_fixture_is_intact() {
	let root = forkparity_dir().join("project/performance-output");
	let transcript = read_json(&root.join("transcript.json"));
	let provenance = read_json(&root.join("provenance.json"));
	assert_eq!(string_field(&provenance, "baselineCommit"), "11d0bc2");
	assert_eq!(string_field(&provenance, "compilerVersion"), COMPILER_VERSION);
	assert_eq!(string_field(&provenance, "zipDigest"), ZIP_DIGEST);
	assert_ne!(string_field(&provenance, "baselineCaptureCommand"), "");
	assert_ne!(string_field(&provenance, "sourceMapGoldenRefresh"), "");
	assert_eq!(transcript.get("exitCode").and_then(Value::as_i64), Some(0));
	assert_eq!(transcript.get("diagnostics"), Some(&Value::Array(Vec::new())));
	let golden = read_tree(&root.join("golden/tree"), &|_| true);
	let paths: Vec<&String> = golden.keys().collect();
	assert_eq!(
		paths,
		string_list(&transcript, "artifactPaths").iter().collect::<Vec<_>>()
	);
	assert_eq!(string_list(&transcript, "emittedFiles").len(), 6);
	for suffix in [".luau", ".luau.map", ".d.ts", ".d.ts.map", "rbxts.copyfiles.json"] {
		assert!(
			golden.keys().any(|path| path.ends_with(suffix)),
			"golden lacks {suffix}"
		);
	}
	let alpha = fs::read(root.join("tree/src/alpha.ts")).expect("alpha.ts reads");
	assert!(alpha.starts_with(&[0xef, 0xbb, 0xbf]) && alpha.windows(2).any(|pair| pair == b"\r\n"));
	let beta = fs::read(root.join("tree/src/beta.ts")).expect("beta.ts reads");
	assert!(beta.windows(2).any(|pair| pair == b"\r\n"));
}

#[test]
fn plain_output_drops_colour_and_spells_the_code_the_fork_way() {
	let output = "\x1b[1m\x1b[31merror[TS2322]:\x1b[0m Type 'number' is not assignable to type 'string'.\nwarning[TS6133]: x\nmyerror[TS1]: kept\nerror[TSx]: kept\n";
	assert_eq!(
		plain_output(output),
		"error TS2322: Type 'number' is not assignable to type 'string'.\nwarning TS6133: x\nmyerror[TS1]: kept\nerror[TSx]: kept\n"
	);
}

/// `TestFullForkParity`: every transformer and project fixture through the release binary, with
/// zero drift; the upstream-corrected rows also need their runtime test to pass.
#[test]
#[ignore = "runs sloptor over the whole corpus and the Lune runtime suites; mise run rust:forkparity"]
fn full_fork_parity() {
	let transformers = transformer_fixtures();
	let projects = project_fixtures();
	let ids = matrix_case_ids(&transformers, &projects);
	let ledger = read_ledger();
	if let Err(error) = ledger.validate(&ids) {
		panic!("validate divergence ledger: {error}");
	}
	let rows: BTreeMap<&str, &LedgerRow> = ledger.rows.iter().map(|row| (row.id.as_str(), row)).collect();

	let mut verified = BTreeSet::new();
	let mut verify = |row: &LedgerRow| {
		if verified.insert(row.behavioral_test.clone())
			&& let Err(error) = runtime::verify_behavioral_test(&row.behavioral_test)
		{
			panic!("verify upstream correction {:?}: {error}", row.id);
		}
	};

	let mut report = String::new();
	let mut accepted = Vec::new();
	let mut checked = 0;
	for fixture in &transformers {
		let row = rows[format!("transformer/{}", fixture.name).as_str()];
		let mut drifts = run_transformer_fixture(fixture);
		if row.classification == "upstream-corrected" {
			verify(row);
			let (archive, rest): (Vec<_>, Vec<_>) = drifts.into_iter().partition(|drift| drift.surface == "byte");
			accepted.extend(archive.into_iter().map(|drift| format!("{}: {}", row.id, drift.detail)));
			drifts = rest;
		}
		for drift in drifts {
			let _ = writeln!(report, "{} [{}]: {}", row.id, drift.surface, drift.detail);
		}
		checked += 1;
	}
	for fixture in &projects {
		let row = rows[format!("project/{}", fixture.name).as_str()];
		for drift in run_project_fixture(fixture) {
			let _ = writeln!(report, "{} [{}]: {}", row.id, drift.surface, drift.detail);
		}
		checked += 1;
	}
	for id in &ids {
		if id.starts_with("transformer/") || id.starts_with("project/") {
			continue;
		}
		let row = rows[id.as_str()];
		if row.classification == "upstream-corrected" {
			verify(row);
		}
		checked += 1;
	}
	assert!(report.is_empty(), "full fork parity drift:\n{report}");
	assert_eq!(checked, ids.len(), "matrix rows checked");
	if !accepted.is_empty() {
		eprintln!(
			"archive differences accepted on upstream-corrected rows:\n{}",
			accepted.join("\n")
		);
	}
}
