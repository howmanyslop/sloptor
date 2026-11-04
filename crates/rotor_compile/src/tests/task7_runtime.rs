//! `task7_runtime_test.go`

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use super::sidecar::build;
use super::{Temp, copy_dir, repo_root};
use crate::project::ProjectOptions;

#[test]
fn task7_runtime_fixture_matches_pinned_flamework_upstream_oracle() {
	// Given: one real lifecycle/DI project and the installed upstream v1.3.2 transformer.
	let repo = repo_root();
	let fixture = repo.join("testdata/flamework/task7-runtime");
	let modules = repo.join("testdata/transformers/project/node_modules");
	let version_data = std::fs::read(modules.join("rbxts-transformer-flamework/package.json"))
		.unwrap_or_else(|error| panic!("pinned Flamework transformer is unavailable: {error}"));
	let manifest: serde_json::Value = serde_json::from_slice(&version_data).expect("package.json");
	let version = manifest
		.get("version")
		.and_then(serde_json::Value::as_str)
		.unwrap_or_default();
	assert_eq!(version, "1.3.2", "rbxts-transformer-flamework version");

	let oracle_dir = task7_project_dir(&repo, &modules);
	copy_dir(&fixture, &oracle_dir.join(""));
	symlink(&modules, &oracle_dir.join("node_modules"));

	// When: the upstream rbxtsc CLI and native phase compile independent copies.
	let oracle = Command::new(oracle_dir.join("node_modules/.bin/rbxtsc"))
		.args(["-p", "tsconfig.json"])
		.current_dir(oracle_dir.join(""))
		.output()
		.expect("upstream rbxtsc");
	assert!(
		oracle.status.success(),
		"upstream rbxtsc: {}\n{}{}",
		oracle.status,
		String::from_utf8_lossy(&oracle.stdout),
		String::from_utf8_lossy(&oracle.stderr)
	);
	let oracle_outputs = read_task7_luau_tree(&oracle_dir.join("out"));
	assert_eq!(oracle_outputs.len(), 6, "upstream transformer-owned Luau output count");
	let oracle_build_info =
		std::fs::read_to_string(oracle_dir.join("flamework.build")).expect("oracle flamework.build");
	assert_eq!(oracle_build_info.len(), 579, "upstream flamework.build size");

	let native_dir = task7_project_dir(&repo, &modules);
	copy_dir(&fixture, &native_dir.join(""));
	symlink(&modules, &native_dir.join("node_modules"));
	let tsconfig = std::fs::read_to_string(native_dir.join("tsconfig.json")).expect("tsconfig");
	let plugin_start = tsconfig
		.find(r#""plugins": ["#)
		.expect("legacy transformer entry not found in runtime fixture");
	let plugin_end = tsconfig[plugin_start..]
		.find("]\n")
		.expect("legacy transformer entry not found in runtime fixture")
		+ plugin_start
		+ 1;
	let native_config = format!(
		"{}{}{}",
		&tsconfig[..plugin_start],
		r#""plugins": []"#,
		&tsconfig[plugin_end..]
	);
	assert_ne!(
		native_config, tsconfig,
		"legacy transformer entry not found in runtime fixture"
	);
	native_dir.write("tsconfig.json", native_config);
	native_dir.write(
		"rotor.toml",
		"[flamework]\nidGenerationMode = \"short\"\nsalt = \"task7-runtime-fixed-salt\"\n",
	);
	let (native_result, native_diags, error) = build(&native_dir.dir(), &ProjectOptions::default());
	assert!(
		error.is_none() && native_diags.is_empty(),
		"native build: {error:?} (diags: {native_diags:?})"
	);
	let native_result = native_result.expect("a result");
	let native_build_info =
		std::fs::read_to_string(native_dir.join("flamework.build")).expect("native flamework.build");
	assert_eq!(
		native_result.outputs.len(),
		6,
		"native transformer-owned Luau output count"
	);
	assert_eq!(native_build_info.len(), 579, "native flamework.build size");

	// Then: every transformer-owned final Luau file and deterministic build artifact is
	// byte-identical.
	let native_outputs: BTreeMap<String, String> = native_result
		.outputs
		.iter()
		.map(|(path, bytes)| (path.clone(), String::from_utf8_lossy(bytes).into_owned()))
		.collect();
	assert_eq!(
		strip_task7_compiler_banners(&oracle_outputs),
		strip_task7_compiler_banners(&native_outputs),
		"final Luau tree mismatch (oracle, native)"
	);
	assert_eq!(
		oracle_build_info, native_build_info,
		"flamework.build mismatch (oracle, native)"
	);
}

/// `task7ProjectDir`: a temporary project on the volume of `modules`. Upstream Flamework treats a
/// `node_modules` real path on another volume as project source.
fn task7_project_dir(repo: &Path, modules: &Path) -> Temp {
	let volume = |path: &Path| rotor_support::gopath::volume_name(&path.to_string_lossy());
	if volume(&std::env::temp_dir()).eq_ignore_ascii_case(&volume(modules)) {
		return Temp::new();
	}
	let parent = repo.join("tmp");
	std::fs::create_dir_all(&parent).expect("the repository's temporary directory");
	Temp::new_in(&parent)
}

fn symlink(target: &Path, link: &Path) {
	#[cfg(unix)]
	std::os::unix::fs::symlink(target, link).expect("symlink");
	#[cfg(windows)]
	std::os::windows::fs::symlink_dir(target, link).expect("symlink");
}

/// `readTask7LuauTree`: every `.luau` file under `root`, keyed by its path from `root`'s parent.
fn read_task7_luau_tree(root: &Path) -> BTreeMap<String, String> {
	fn walk(base: &Path, directory: &Path, outputs: &mut BTreeMap<String, String>) {
		for entry in std::fs::read_dir(directory).expect("read dir").flatten() {
			let path = entry.path();
			if path.is_dir() {
				walk(base, &path, outputs);
				continue;
			}
			if path.extension().and_then(|extension| extension.to_str()) != Some("luau") {
				continue;
			}
			let relative = path
				.strip_prefix(base)
				.map(|relative| relative.to_string_lossy().replace('\\', "/"))
				.unwrap_or_default();
			outputs.insert(relative, std::fs::read_to_string(&path).expect("read output"));
		}
	}
	let mut outputs = BTreeMap::new();
	walk(root.parent().unwrap_or(root), root, &mut outputs);
	outputs
}

/// `stripTask7CompilerBanners`
fn strip_task7_compiler_banners(outputs: &BTreeMap<String, String>) -> BTreeMap<String, String> {
	outputs
		.iter()
		.map(|(path, output)| {
			let (banner, body) = output
				.split_once('\n')
				.filter(|(banner, _)| banner.starts_with("-- Compiled with "))
				.unwrap_or_else(|| panic!("{path} is missing the compiler banner"));
			let _ = banner;
			(path.clone(), body.to_owned())
		})
		.collect()
}
