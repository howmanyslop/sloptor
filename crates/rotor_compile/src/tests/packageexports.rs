//! `packageexports_test.go`

use rotor_rojo::canonical_file_name;
use rotor_support::gopath;
use rotor_transformer::NodeModulesPathMapping;

use super::Temp;
use crate::packageexports::{create_node_modules_path_mapping, package_real_path, resolve_package_exports};

/// `filepath.Join`
fn join(parts: &[&str]) -> String {
	gopath::join(parts)
}

/// `writePackageFile`
fn write_package_file(pkg_dir: &str, name: &str, contents: &str) {
	let path = std::path::Path::new(pkg_dir).join(name);
	std::fs::create_dir_all(path.parent().expect("a parent")).expect("the package directory");
	std::fs::write(path, contents).expect("the package file writes");
}

/// `writePackageManifest`
fn write_package_manifest(scope_path: &str, name: &str, manifest: &str) -> String {
	let pkg_dir = join(&[scope_path, name]);
	write_package_file(&pkg_dir, "package.json", manifest);
	pkg_dir
}

/// `assertPackageMapping`
fn assert_package_mapping(mapping: &NodeModulesPathMapping, types_path: &str, runtime_path: &str) {
	let got = mapping.lookup(types_path, true).unwrap_or_default();
	assert_eq!(got, runtime_path, "mapping.Lookup({types_path:?})");
}

/// `assertNoPackageMapping`
fn assert_no_package_mapping(mapping: &NodeModulesPathMapping, types_path: &str) {
	if let Some(got) = mapping.lookup(types_path, true) {
		panic!("mapping.Lookup({types_path:?}) = {got:?}, want no mapping");
	}
}

fn conditions(names: &[&str]) -> Vec<String> {
	names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn resolve_package_exports_cases() {
	for (name, manifest, want_types, want_runtime) in [
		(
			"uses types and the first runtime condition",
			r#"{"exports":{".":{"types":"./dist/index.d.ts","import":"./dist/import.luau","require":"./dist/require.luau","default":"./dist/default.luau"}}}"#,
			"dist/index.d.ts",
			"dist/import.luau",
		),
		(
			"uses an adjacent declaration when exports omits types",
			r#"{"exports":"./dist/init.luau"}"#,
			"dist/init.d.ts",
			"dist/init.luau",
		),
		(
			"falls back to legacy types before typings and main",
			r#"{"types":"./types/index.d.ts","typings":"./typings/index.d.ts","main":"./out/init.lua"}"#,
			"types/index.d.ts",
			"out/init.lua",
		),
	] {
		let temp = Temp::new();
		let pkg_dir = write_package_manifest(&temp.dir(), "pkg", manifest);

		let (types_path, runtime_path) = resolve_package_exports(&pkg_dir, &[])
			.unwrap_or_else(|error| panic!("{name}: resolvePackageExports: {error}"));
		assert_eq!(
			types_path,
			join(&[&pkg_dir, &gopath::from_slash(want_types)]),
			"{name}: types path"
		);
		assert_eq!(
			runtime_path,
			join(&[&pkg_dir, &gopath::from_slash(want_runtime)]),
			"{name}: runtime path"
		);
	}
}

#[test]
fn package_exports_mapping_maps_subpaths_and_custom_condition_targets() {
	let temp = Temp::new();
	let scope_path = join(&[&temp.dir(), "@rbxts"]);
	let pkg_dir = write_package_manifest(
		&scope_path,
		"pkg",
		r#"{
			"exports": {
				".": {
					"types": "./dist/index.d.ts",
					"source": "./src/index.ts",
					"import": "./dist/init.luau"
				},
				"./utils": {
					"types": "./dist/utils.d.ts",
					"require": "./dist/utils.luau"
				},
				"./features/*": {
					"types": "./dist/features/*.d.ts",
					"import": "./dist/features/*.luau"
				}
			}
		}"#,
	);

	let mapping = create_node_modules_path_mapping(&[scope_path], true, &conditions(&["source"]));

	assert_package_mapping(
		&mapping,
		&join(&[&pkg_dir, "dist", "index.d.ts"]),
		&join(&[&pkg_dir, "dist", "init.luau"]),
	);
	assert_package_mapping(
		&mapping,
		&join(&[&pkg_dir, "src", "index.ts"]),
		&join(&[&pkg_dir, "dist", "init.luau"]),
	);
	assert_package_mapping(
		&mapping,
		&join(&[&pkg_dir, "dist", "utils.d.ts"]),
		&join(&[&pkg_dir, "dist", "utils.luau"]),
	);
	assert_package_mapping(
		&mapping,
		&join(&[&pkg_dir, "dist", "features", "a", "b.d.ts"]),
		&join(&[&pkg_dir, "dist", "features", "a", "b.luau"]),
	);
	assert!(
		!mapping.exact.contains_key(&canonical_file_name(
			&join(&[&pkg_dir, "dist", "features", "*.d.ts"]),
			true
		)),
		"wildcard export subpath should not create an exact mapping"
	);
}

#[test]
fn package_exports_mapping_remaps_runtime_through_a_package_rojo_path() {
	let temp = Temp::new();
	let scope_path = join(&[&temp.dir(), "@rbxts"]);
	let pkg_dir = write_package_manifest(
		&scope_path,
		"pkg",
		r#"{
			"exports": {
				".": {"types":"./dist/index.d.ts","import":"./dist/init.luau"}
			}
		}"#,
	);
	write_package_file(
		&pkg_dir,
		"default.project.json",
		r#"{"name":"pkg","tree":{"$path":"out"}}"#,
	);

	let mapping = create_node_modules_path_mapping(&[scope_path], true, &[]);

	assert_package_mapping(
		&mapping,
		&join(&[&pkg_dir, "dist", "index.d.ts"]),
		&join(&[&pkg_dir, "out", "init.luau"]),
	);
}

/// A package under `<dir>/store` symlinked into `<dir>/node_modules/@rbxts/pkg`; returns
/// `(scope_path, virtual_pkg_dir, real_pkg_dir)`, or `None` when the symlink cannot be made.
fn symlinked_package(dir: &str, manifest: &str) -> Option<(String, String, String)> {
	let real_pkg_dir = write_package_manifest(&join(&[dir, "store"]), "pkg", manifest);
	let scope_path = join(&[dir, "node_modules", "@rbxts"]);
	let virtual_pkg_dir = join(&[&scope_path, "pkg"]);
	std::fs::create_dir_all(&scope_path).expect("the scope directory");
	if let Err(error) = symlink_dir(&real_pkg_dir, &virtual_pkg_dir) {
		eprintln!("cannot create package symlink: {error}");
		return None;
	}
	Some((scope_path, virtual_pkg_dir, real_pkg_dir))
}

#[cfg(unix)]
pub fn symlink_dir(target: &str, link: &str) -> std::io::Result<()> {
	std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
pub fn symlink_dir(target: &str, link: &str) -> std::io::Result<()> {
	std::os::windows::fs::symlink_dir(target, link)
}

#[test]
fn package_exports_mapping_adds_a_real_path_key_for_a_symlinked_package() {
	let temp = Temp::new();
	let Some((scope_path, virtual_pkg_dir, real_pkg_dir)) = symlinked_package(
		&temp.dir(),
		r#"{
			"exports": {
				".": {"types":"./dist/index.d.ts","import":"./dist/init.luau"}
			}
		}"#,
	) else {
		return;
	};

	let mapping = create_node_modules_path_mapping(&[scope_path], true, &[]);

	assert_package_mapping(
		&mapping,
		&join(&[&virtual_pkg_dir, "dist", "index.d.ts"]),
		&join(&[&virtual_pkg_dir, "dist", "init.luau"]),
	);
	assert_package_mapping(
		&mapping,
		&join(&[&package_real_path(&real_pkg_dir), "dist", "index.d.ts"]),
		&join(&[&virtual_pkg_dir, "dist", "init.luau"]),
	);
}

#[test]
fn package_exports_mapping_adds_a_real_path_pattern_for_a_symlinked_package() {
	let temp = Temp::new();
	let Some((scope_path, virtual_pkg_dir, real_pkg_dir)) = symlinked_package(
		&temp.dir(),
		r#"{
			"exports": {
				"./test/*": {"testing": {"types":"./test/*.ts","default":"./out-test/test/*.luau"}}
			}
		}"#,
	) else {
		return;
	};

	let mapping = create_node_modules_path_mapping(&[scope_path], true, &conditions(&["testing"]));

	let want = join(&[&virtual_pkg_dir, "out-test", "test", "helpers", "styles.luau"]);
	assert_package_mapping(
		&mapping,
		&join(&[&virtual_pkg_dir, "test", "helpers", "styles.ts"]),
		&want,
	);
	assert_package_mapping(
		&mapping,
		&join(&[&package_real_path(&real_pkg_dir), "test", "helpers", "styles.ts"]),
		&want,
	);
}

#[test]
fn package_exports_condition_branches_reads_a_custom_condition_branch_for_types_and_runtime() {
	let temp = Temp::new();
	let scope_path = join(&[&temp.dir(), "@rbxts"]);
	let pkg_dir = write_package_manifest(
		&scope_path,
		"pkg",
		r#"{
			"exports": {
				"./testing": {
					"testing": {
						"source": "./test/helpers/index.ts",
						"types": "./out-tsc/spec/test/helpers/index.d.ts",
						"default": "./out-test/test/helpers/index.luau"
					}
				}
			}
		}"#,
	);

	let mapping = create_node_modules_path_mapping(&[scope_path], true, &conditions(&["testing"]));

	assert_package_mapping(
		&mapping,
		&join(&[&pkg_dir, "out-tsc", "spec", "test", "helpers", "index.d.ts"]),
		&join(&[&pkg_dir, "out-test", "test", "helpers", "index.luau"]),
	);
	assert_no_package_mapping(&mapping, &join(&[&pkg_dir, "test", "helpers", "index.ts"]));
}

#[test]
fn package_exports_condition_branches_skips_a_custom_condition_branch_when_the_condition_is_off() {
	let temp = Temp::new();
	let scope_path = join(&[&temp.dir(), "@rbxts"]);
	let pkg_dir = write_package_manifest(
		&scope_path,
		"pkg",
		r#"{
			"exports": {
				"./testing": {
					"testing": {"types": "./out-tsc/x.d.ts", "default": "./out-test/x.luau"}
				}
			}
		}"#,
	);

	let mapping = create_node_modules_path_mapping(&[scope_path], true, &[]);

	assert_no_package_mapping(&mapping, &join(&[&pkg_dir, "out-tsc", "x.d.ts"]));
}

#[test]
fn package_exports_condition_branches_does_not_take_runtime_from_outside_a_matched_branch() {
	let temp = Temp::new();
	let scope_path = join(&[&temp.dir(), "@rbxts"]);
	let pkg_dir = write_package_manifest(
		&scope_path,
		"pkg",
		r#"{
			"exports": {
				"./x": {
					"testing": {"types": "./spec/x.d.ts"},
					"types": "./out/x.d.ts",
					"default": "./out/x.luau"
				}
			}
		}"#,
	);

	let mapping = create_node_modules_path_mapping(&[scope_path], true, &conditions(&["testing"]));

	assert_no_package_mapping(&mapping, &join(&[&pkg_dir, "spec", "x.d.ts"]));
	assert_no_package_mapping(&mapping, &join(&[&pkg_dir, "out", "x.d.ts"]));
}

#[test]
fn package_exports_condition_branches_substitutes_a_wildcard_inside_a_custom_condition_branch() {
	let temp = Temp::new();
	let scope_path = join(&[&temp.dir(), "@rbxts"]);
	let pkg_dir = write_package_manifest(
		&scope_path,
		"pkg",
		r#"{
			"exports": {
				"./test/*": {
					"testing": {
						"source": "./test/*.ts",
						"types": "./test/*.ts",
						"default": "./out-test/test/*.luau"
					}
				}
			}
		}"#,
	);

	let mapping = create_node_modules_path_mapping(&[scope_path], true, &conditions(&["testing"]));

	assert_package_mapping(
		&mapping,
		&join(&[&pkg_dir, "test", "helpers", "styles.ts"]),
		&join(&[&pkg_dir, "out-test", "test", "helpers", "styles.luau"]),
	);
	assert_no_package_mapping(&mapping, &join(&[&pkg_dir, "src", "helpers", "styles.ts"]));
}

#[test]
fn package_exports_condition_branches_prefers_the_longest_wildcard_prefix() {
	let temp = Temp::new();
	let scope_path = join(&[&temp.dir(), "@rbxts"]);
	let pkg_dir = write_package_manifest(
		&scope_path,
		"pkg",
		r#"{
			"exports": {
				"./*": {"types": "./out/*.d.ts", "default": "./out/*.luau"},
				"./deep/*": {"types": "./out/deep/*.d.ts", "default": "./shipped/deep/*.luau"}
			}
		}"#,
	);

	let mapping = create_node_modules_path_mapping(&[scope_path], true, &[]);

	assert_package_mapping(
		&mapping,
		&join(&[&pkg_dir, "out", "deep", "a.d.ts"]),
		&join(&[&pkg_dir, "shipped", "deep", "a.luau"]),
	);
	assert_package_mapping(
		&mapping,
		&join(&[&pkg_dir, "out", "a.d.ts"]),
		&join(&[&pkg_dir, "out", "a.luau"]),
	);
}
