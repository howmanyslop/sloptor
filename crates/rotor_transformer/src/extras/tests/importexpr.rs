//! Rojo-path edge cases of `importexpr.go`: Go's stored nil path (a tree-root `$path` naming a
//! module file) is a miss wherever Go checks `!ok || path == nil`.

use rotor_rojo::{PathTranslator, ProjectFiles, RojoResolver};
use rotor_support::gopath;

use super::write_fixture;
use crate::extras::importexpr::find_relative_rbx_path;
use crate::nodemodulespath::NodeModulesPathMapping;
use crate::state::{ProjectType, RojoContext};
use crate::statementlist::transform_statement_list;
use crate::tests::with_state;

const NIL_PACKAGE_PROJECT: &str = r#"{"name":"pkg","tree":{"$path":"init.lua"}}"#;

/// A type root that is itself a project whose tree root names `init.lua`: Go maps it to a nil
/// path.
fn write_nil_package(root: &std::path::Path, scope: &str) {
	write_fixture(
		root,
		&format!("node_modules/{scope}/default.project.json"),
		NIL_PACKAGE_PROJECT,
	);
	write_fixture(root, &format!("node_modules/{scope}/init.lua"), "return {}\n");
	write_fixture(
		root,
		&format!("node_modules/{scope}/index.d.ts"),
		"export declare const value: number;\n",
	);
}

#[test]
fn find_relative_rbx_path_skips_nil_package_paths() {
	let directory = tempfile::tempdir().expect("a temporary directory");
	let root = rotor_support::fsutil::real_path(directory.path()).expect("the directory resolves");
	write_nil_package(&root, "@nil");
	write_fixture(
		&root,
		"node_modules/@found/default.project.json",
		r#"{"name":"pkg","tree":{"Pkg":{"$path":"init.lua"}}}"#,
	);
	write_fixture(&root, "node_modules/@found/init.lua", "return {}\n");
	let root = root.to_string_lossy().into_owned();
	let nil = RojoResolver::package_imports(&gopath::join(&[&root, "node_modules/@nil"]), &ProjectFiles::new());
	let found = RojoResolver::package_imports(&gopath::join(&[&root, "node_modules/@found"]), &ProjectFiles::new());

	let nil_module = gopath::join(&[&root, "node_modules/@nil/init.luau"]);
	assert_eq!(
		find_relative_rbx_path(&nil_module, std::slice::from_ref(&nil)),
		None,
		"a nil path is a miss"
	);
	let found_module = gopath::join(&[&root, "node_modules/@found/init.luau"]);
	assert_eq!(
		find_relative_rbx_path(&found_module, &[nil, found]),
		Some(vec!["Pkg".to_owned()]),
		"the next package resolver is tried"
	);
}

/// A package project importing a package whose `default.project.json` is
/// `{"tree":{"$path":"init.lua"}}` at its type root: the package resolver stores a nil path,
/// which Go reports as `noRojoData`.
#[test]
fn package_import_of_nil_rojo_path_reports_no_rojo_data() {
	let directory = tempfile::tempdir().expect("a temporary directory");
	let root = rotor_support::fsutil::real_path(directory.path()).expect("the directory resolves");
	write_fixture(
		&root,
		"tsconfig.json",
		r#"{"compilerOptions":{"target":"ESNext","strict":true,"types":[],"moduleDetection":"force","rootDir":"src","outDir":"out"},"include":["src"]}"#,
	);
	write_nil_package(&root, "@scope");
	write_fixture(
		&root,
		"src/main.ts",
		"import { value } from \"../node_modules/@scope\";\ndeclare function print(v: unknown): void;\nprint(value);\n",
	);
	let root_text = root.to_string_lossy().into_owned();
	let type_root = gopath::join(&[&root_text, "node_modules/@scope"]);
	let rojo: &'static RojoContext = Box::leak(Box::new(RojoContext {
		resolver: RojoResolver::synthetic(&gopath::join(&[&root_text, "out"])),
		path_translator: PathTranslator::new(
			&gopath::join(&[&root_text, "src"]),
			&gopath::join(&[&root_text, "out"]),
			"",
			false,
			true,
		),
		runtime_lib_rbx_path: None,
		project_path: root_text.clone(),
		pkg_rojo_resolvers: vec![RojoResolver::package_imports(&type_root, &ProjectFiles::new())],
		node_modules_path_mapping: NodeModulesPathMapping::default(),
		project_reference_paths: std::collections::HashMap::new(),
		node_modules_path: gopath::join(&[&root_text, "node_modules"]),
		type_roots: vec![type_root],
		use_case_sensitive_file_names: true,
	}));
	let codes = with_state(&root, "src/main.ts", |state| {
		state.set_rojo_context(rojo, Some(ProjectType::Package));
		let file = state.source_file;
		transform_statement_list(state, Some(file.as_node()), file.statements().nodes(), None);
		assert!(
			state.internal_error().is_none(),
			"internal error: {:?}",
			state.internal_error()
		);
		state
			.diags
			.flush()
			.into_iter()
			.map(|diagnostic| diagnostic.code)
			.collect::<Vec<_>>()
	});
	assert_eq!(
		codes,
		["noRojoData"],
		"the nil package path is reported as missing Rojo data"
	);
}
