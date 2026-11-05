//! Port of `internal/rojo/resolver_walk_test.go`.

use rotor_support::gopath;

use super::RojoResolver;
use crate::config::{Tree, TreeEntry};
use crate::project_files::ProjectFiles;
use crate::test_support::{directory_link, must_rbx_path, rbx, write_project};

fn platform_link_kind() -> &'static str {
	if cfg!(windows) { "junction" } else { "symlink" }
}

#[test]
fn resolver_directory_link_cycles() {
	for kind in ["junction", "symlink"] {
		type Constructor = fn(&str) -> RojoResolver;
		let constructors: [(&str, Constructor); 2] = [
			("Synthetic", RojoResolver::synthetic),
			("PackageImports", |path| {
				RojoResolver::package_imports(path, &ProjectFiles::new())
			}),
		];
		for (name, constructor) in constructors {
			let project = write_project(&[
				("data/default.project.json", r#"{"name":"Data","tree":{"$path":"src"}}"#),
				("data/src/init.luau", "return {}"),
			]);
			if !directory_link(kind, &project.path("loop"), &project.dir) {
				continue;
			}

			let r = constructor(&project.dir);
			assert_eq!(
				r.partitions().len(),
				2,
				"{kind}/{name}: cycle produced {} partitions, want root and one nested project",
				r.partitions().len()
			);
			assert_eq!(
				must_rbx_path(&r, &project.path("data/src/init.luau")),
				rbx(&["Data"]),
				"{kind}/{name}: nested project path"
			);
		}
	}
}

#[test]
fn resolver_directory_link_cycle_through_nested_project() {
	let project = write_project(&[
		("data/default.project.json", r#"{"name":"Data","tree":{"$path":"src"}}"#),
		("data/src/init.luau", "return {}"),
	]);
	if !directory_link("symlink", &project.path("data/src/back"), &project.dir) {
		return;
	}

	let r = RojoResolver::synthetic(&project.dir);
	assert_eq!(
		r.partitions().len(),
		2,
		"cycle through nested project produced {} partitions, want 2",
		r.partitions().len()
	);
}

#[test]
fn resolver_directory_link_cycle_through_default_project() {
	let project = write_project(&[("pkg/default.project.json", r#"{"name":"Pkg","tree":{"$path":"src"}}"#)]);
	if !directory_link(platform_link_kind(), &project.path("pkg/src"), &project.path("pkg")) {
		return;
	}

	let r = RojoResolver::synthetic(&project.dir);
	assert_eq!(
		r.partitions().len(),
		1,
		"default-project cycle produced {} partitions, want only the outer root",
		r.partitions().len()
	);
	assert!(
		r.warnings().is_empty(),
		"cycle through default project must stop before filesystem errors: {:?}",
		r.warnings()
	);
	assert_eq!(
		r.state().walked_configs.len(),
		1,
		"cycle visited {} project paths, want 1",
		r.state().walked_configs.len()
	);
}

#[test]
fn resolver_workspace_package_cycle() {
	let project = write_project(&[
		(
			"a/data/default.project.json",
			r#"{"name":"Data","tree":{"$path":"src"}}"#,
		),
		("a/data/src/init.luau", "return {}"),
		("a/node_modules/", ""),
		("b/node_modules/", ""),
	]);
	let kind = platform_link_kind();
	if !directory_link(kind, &project.path("a/node_modules/b"), &project.path("b"))
		|| !directory_link(kind, &project.path("b/node_modules/a"), &project.path("a"))
	{
		return;
	}

	let r = RojoResolver::package_imports(&project.dir, &ProjectFiles::new());
	assert_eq!(
		r.partitions().len(),
		2,
		"workspace cycle produced {} partitions, want root and one nested project",
		r.partitions().len()
	);
	assert_eq!(
		must_rbx_path(&r, &project.path("a/data/src/init.luau")),
		rbx(&["a", "Data"]),
		"workspace package path"
	);
}

#[test]
fn package_imports_branching_directory_links() {
	const LEVELS: usize = 8;
	let mut files = Vec::new();
	let level_dirs: Vec<String> = (0..LEVELS).map(|level| format!("level{level}/")).collect();
	for dir in &level_dirs {
		files.push((dir.as_str(), ""));
	}
	let data_project = format!("level{}/data/default.project.json", LEVELS - 1);
	let data_init = format!("level{}/data/src/init.luau", LEVELS - 1);
	files.push((data_project.as_str(), r#"{"name":"Data","tree":{"$path":"src"}}"#));
	files.push((data_init.as_str(), "return {}"));
	let project = write_project(&files);
	let root = project.path("level0");
	let kind = platform_link_kind();
	for level in 0..LEVELS {
		let parent = project.path(&format!("level{level}"));
		if level == LEVELS - 1 {
			if !directory_link(kind, &gopath::join(&[&parent, "back"]), &root) {
				return;
			}
			continue;
		}
		let target = project.path(&format!("level{}", level + 1));
		for name in ["a", "b"] {
			if !directory_link(kind, &gopath::join(&[&parent, name]), &target) {
				return;
			}
		}
	}

	let r = RojoResolver::package_imports(&root, &ProjectFiles::new());
	let max_directories = 3 * (LEVELS + 2);
	let walked = r.state().walked_directories.len();
	assert!(
		walked <= max_directories,
		"branching links recorded {walked} watch directories, want at most {max_directories}"
	);
	assert_eq!(
		r.partitions().len(),
		2,
		"branching aliases produced {} partitions, want root and one nested project",
		r.partitions().len()
	);
}

#[test]
fn package_imports_repeated_directory_links_use_first_discovery() {
	let project = write_project(&[
		("root/", ""),
		(
			"package/data/default.project.json",
			r#"{"name":"Data","tree":{"$path":"src"}}"#,
		),
		("package/data/src/init.luau", "return {}"),
	]);
	let root = project.path("root");
	for name in ["a", "b"] {
		if !directory_link("symlink", &gopath::join(&[&root, name]), &project.path("package")) {
			return;
		}
	}

	let r = RojoResolver::package_imports(&root, &ProjectFiles::new());
	assert_eq!(
		r.partitions().len(),
		2,
		"repeated links produced {} partitions, want root and one nested project",
		r.partitions().len()
	);
	for (name, want) in [("a", rbx(&["a", "Data"])), ("b", rbx(&["b", "data", "src"]))] {
		let got = must_rbx_path(&r, &gopath::join(&[&root, name, "data", "src", "init.luau"]));
		assert_eq!(got, want, "link {name} path");
	}
	assert!(
		r.state().walked_directories.contains(&gopath::join(&[&root, "b"])),
		"skipped package link must retain its logical watch path"
	);
}

#[test]
fn from_tree_repeated_directory_mounts() {
	let project = write_project(&[
		("data/default.project.json", r#"{"name":"Data","tree":{"$path":"src"}}"#),
		("data/src/init.luau", "return {}"),
	]);
	let path = ".".to_owned();
	let mount = |name: &str| TreeEntry {
		name: name.to_owned(),
		tree: Tree {
			path: Some(path.clone()),
			..Tree::default()
		},
	};
	let r = RojoResolver::from_tree(
		&project.dir,
		&Tree {
			children: vec![mount("First"), mount("Second")],
			..Tree::default()
		},
	);
	assert_eq!(
		r.partitions().len(),
		4,
		"separate mounts produced {} partitions, want 4",
		r.partitions().len()
	);
	assert_eq!(
		must_rbx_path(&r, &project.path("data/src/init.luau")),
		rbx(&["Second", "Data"]),
		"later mount path"
	);
}

#[test]
fn resolver_discovered_project_repeated_directory_mounts() {
	let project = write_project(&[
		(
			"root/pkg/default.project.json",
			r#"{"name":"Pkg","tree":{"First":{"$path":"../../shared"},"Second":{"$path":"../../shared"}}}"#,
		),
		(
			"shared/data/default.project.json",
			r#"{"name":"Data","tree":{"$path":"src"}}"#,
		),
		("shared/data/src/init.luau", "return {}"),
	]);
	let r = RojoResolver::synthetic(&project.path("root"));
	assert_eq!(
		must_rbx_path(&r, &project.path("shared/data/src/init.luau")),
		rbx(&["Pkg", "Second", "Data"]),
		"later mount in discovered project"
	);
}
