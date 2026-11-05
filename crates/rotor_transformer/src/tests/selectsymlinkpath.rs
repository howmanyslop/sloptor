//! `selectsymlinkpath_test.go`

use crate::selectsymlinkpath::select_symlink_path;

#[test]
fn select_symlink_path_cases() {
	let cases: [(&str, &[&str], &str, &str); 5] = [
		(
			"project node_modules wins",
			&["/cache/pkg", "/project/node_modules/pkg"],
			"/project/node_modules",
			"/project/node_modules/pkg",
		),
		(
			"first cache entry is fallback",
			&["/cache/first", "/cache/second"],
			"/project/node_modules",
			"/cache/first",
		),
		(
			"node_modules prefix without directory boundary is cache entry",
			&["/project/node_modules-cache/pkg", "/cache/first"],
			"/project/node_modules",
			"/project/node_modules-cache/pkg",
		),
		("empty cache", &[], "/project/node_modules", ""),
		// Go's nil and empty slices are the same Rust slice
		("nil cache", &[], "/project/node_modules", ""),
	];
	for (name, symlink_paths, node_modules_path, want) in cases {
		let symlink_paths: Vec<String> = symlink_paths.iter().map(|&path| path.to_owned()).collect();
		assert_eq!(select_symlink_path(&symlink_paths, node_modules_path), want, "{name}");
	}

	let project_path = "project/node_modules";
	let paths = ["cache/pkg".to_owned(), format!("{project_path}/pkg")];
	assert_eq!(
		select_symlink_path(&paths, project_path),
		format!("{project_path}/pkg"),
		"relative project symlink"
	);
}
