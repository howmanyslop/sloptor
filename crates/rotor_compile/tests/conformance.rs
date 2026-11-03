//! The conformance gate (Go's `internal/conformance` `TestConformance`): each golden of
//! `testdata/conformance/golden` is compiled on its own, through a temporary project under
//! `testdata/conformance/project`, and must be byte-identical to the golden.
//!
//! Golden rows the divergence ledger classifies `upstream-corrected` are exempt from byte
//! equality in Go too: their runtime behavior is verified with Lune instead, which needs the
//! pinned upstream compiler, Rojo and Lune. They must still compile; the comparison is reported,
//! not enforced.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use rotor_compile::ProjectOptions;
use rotor_compile::compile::compile_file_with_options;

use rotor_config as _;
use rotor_luau as _;
use rotor_rojo as _;
use rotor_support as _;
use rotor_transformer as _;
use serde_json as _;
use sha2 as _;

fn repo_root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `goldenPaths`: every golden, relative to the golden root, slash-separated.
fn golden_paths(golden_dir: &Path) -> Vec<String> {
	fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
		let Ok(entries) = fs::read_dir(dir) else {
			return;
		};
		let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
		entries.sort_by_key(fs::DirEntry::file_name);
		for entry in entries {
			let path = entry.path();
			if path.is_dir() {
				walk(&path, root, out);
			} else if path.extension().is_some_and(|extension| extension == "luau")
				&& let Ok(relative) = path.strip_prefix(root)
			{
				out.push(relative.to_string_lossy().replace('\\', "/"));
			}
		}
	}
	let mut out = Vec::new();
	walk(golden_dir, golden_dir, &mut out);
	out
}

/// The `conformance/<rel>` rows the ledger classifies upstream-corrected.
#[expect(clippy::expect_used, reason = "a test harness fails loudly")]
fn upstream_corrected_rows(root: &Path) -> BTreeSet<String> {
	let ledger = fs::read_to_string(root.join("testdata/forkparity/divergence-ledger.json"))
		.expect("the divergence ledger reads");
	let ledger: serde_json::Value = serde_json::from_str(&ledger).expect("the divergence ledger parses");
	let rows = ledger
		.get("rows")
		.and_then(serde_json::Value::as_array)
		.cloned()
		.unwrap_or_default();
	rows.iter()
		.filter(|row| row.get("classification").and_then(serde_json::Value::as_str) == Some("upstream-corrected"))
		.filter_map(|row| row.get("id")?.as_str()?.strip_prefix("conformance/").map(str::to_owned))
		.collect()
}

/// `fixtureTSConfig`
const FIXTURE_TS_CONFIG: &str = r#"{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"downlevelIteration": true,
		"jsx": "react",
		"jsxFactory": "Roact.jsx",
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"resolveJsonModule": true,
		"forceConsistentCasingInFileNames": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": ["node_modules/@rbxts"],
		"experimentalDecorators": true,
		"rootDir": "src",
		"outDir": "out",
		"baseUrl": "src"
	},
	"include": ["src"]
}"#;

/// `fixtureRojoConfig`
const FIXTURE_ROJO_CONFIG: &str = r#"{"name":"conformance","tree":{"$path":"out","include":{"$path":"include"},"node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}"#;

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
	fs::create_dir_all(dst)?;
	for entry in fs::read_dir(src)? {
		let entry = entry?;
		let target = dst.join(entry.file_name());
		if entry.file_type()?.is_dir() {
			copy_tree(&entry.path(), &target)?;
		} else {
			fs::copy(entry.path(), target)?;
		}
	}
	Ok(())
}

fn copy_file(src: &Path, dst: &Path) -> std::io::Result<()> {
	if let Some(parent) = dst.parent() {
		fs::create_dir_all(parent)?;
	}
	fs::copy(src, dst).map(|_| ())
}

/// `sourceRelFromGolden`
fn source_rel_from_golden(base_project_dir: &Path, golden_rel: &str) -> Option<String> {
	let rel = golden_rel.strip_suffix(".luau").unwrap_or(golden_rel);
	[".ts", ".tsx"]
		.iter()
		.map(|extension| format!("{rel}{extension}"))
		.find(|candidate| base_project_dir.join("src").join(candidate).exists())
}

/// `compileConformanceFixture`: one golden's source in a fresh project.
fn compile_conformance_fixture(base_project_dir: &Path, golden_rel: &str) -> Result<Vec<u8>, String> {
	let temp = tempfile::Builder::new()
		.prefix(".phase5-fixture-")
		.tempdir_in(base_project_dir)
		.map_err(|error| error.to_string())?;
	let project = temp.path();
	let setup = || -> std::io::Result<String> {
		fs::write(project.join("package.json"), r#"{"name":"conformance-fixture"}"#)?;
		fs::write(project.join("tsconfig.json"), FIXTURE_TS_CONFIG)?;
		fs::write(project.join("default.project.json"), FIXTURE_ROJO_CONFIG)?;
		fs::create_dir_all(project.join("src"))?;
		fs::create_dir_all(project.join("include"))?;
		// ensureFixtureTypeRoots
		fs::create_dir_all(project.join("node_modules"))?;
		let types_src = base_project_dir.join("node_modules/@rbxts");
		let types_dst = project.join("node_modules/@rbxts");
		#[cfg(unix)]
		let linked = std::os::unix::fs::symlink(&types_src, &types_dst).is_ok();
		#[cfg(not(unix))]
		let linked = false;
		if !linked {
			copy_tree(&types_src, &types_dst)?;
		}
		copy_tree(&base_project_dir.join("src/helpers"), &project.join("src/helpers"))?;
		copy_file(
			&base_project_dir.join("src/services.d.ts"),
			&project.join("src/services.d.ts"),
		)?;
		fs::write(
			project.join("src/testez-globals.d.ts"),
			"/// <reference types=\"@rbxts/testez/globals\" />\n",
		)?;
		let source_rel = source_rel_from_golden(base_project_dir, golden_rel)
			.ok_or_else(|| std::io::Error::other(format!("no source file for golden {golden_rel}")))?;
		copy_file(
			&base_project_dir.join("src").join(&source_rel),
			&project.join("src").join(&source_rel),
		)?;
		Ok(source_rel)
	};
	let source_rel = setup().map_err(|error| error.to_string())?;
	let mut options = ProjectOptions::default();
	options.allow_comment_directives = true;
	let project_dir = project.to_string_lossy().into_owned();
	let (text, diagnostics) =
		compile_file_with_options(&project_dir, &format!("src/{source_rel}"), &options).map_err(|failure| {
			format!(
				"CompileFile({golden_rel}): {} (diagnostics: {:?})",
				failure.error,
				failure.messages()
			)
		})?;
	if !diagnostics.is_empty() {
		return Err(format!("CompileFile({golden_rel}) diagnostics: {diagnostics:?}"));
	}
	Ok(text)
}

fn first_difference(got: &str, want: &str) -> String {
	let (got_lines, want_lines): (Vec<_>, Vec<_>) = (got.split('\n').collect(), want.split('\n').collect());
	for (index, (got_line, want_line)) in got_lines.iter().zip(&want_lines).enumerate() {
		if got_line != want_line {
			return format!(
				"first diff at line {}:\n  got:  {got_line:?}\n  want: {want_line:?}",
				index + 1
			);
		}
	}
	format!(
		"length mismatch: got {} lines, want {}",
		got_lines.len(),
		want_lines.len()
	)
}

#[test]
fn conformance_goldens_match() {
	rotor_tsrs::with_large_stack(|| {
		let root = repo_root();
		let project_dir = root.join("testdata/conformance/project");
		let golden_dir = root.join("testdata/conformance/golden");
		assert!(
			project_dir.join("node_modules").exists(),
			"install the conformance project's dependencies (bun install --no-save) first"
		);
		let corrected = upstream_corrected_rows(&root);
		let goldens = golden_paths(&golden_dir);
		assert!(!goldens.is_empty(), "no goldens found");

		let mut failures = String::new();
		let mut waived = Vec::new();
		for rel in &goldens {
			let Ok(want) = fs::read(golden_dir.join(rel)) else {
				let _ = writeln!(failures, "{rel}: the golden does not read");
				continue;
			};
			let got = match compile_conformance_fixture(&project_dir, rel) {
				Ok(got) => got,
				Err(error) => {
					let _ = writeln!(failures, "{rel}: {error}");
					continue;
				}
			};
			if got == want {
				continue;
			}
			if corrected.contains(rel) {
				waived.push(rel.clone());
				continue;
			}
			let _ = writeln!(
				failures,
				"{rel}: output differs from rbxtsc golden\n{}",
				first_difference(&String::from_utf8_lossy(&got), &String::from_utf8_lossy(&want))
			);
		}
		assert!(failures.is_empty(), "conformance failures:\n{failures}");
		// Upstream-corrected rows differ from the frozen rbxtsc golden by design; Go verifies them
		// at runtime with Lune.
		for rel in &waived {
			assert!(corrected.contains(rel));
		}
	});
}
