//! `overlayfs_test.go`, through the overlay route `rotor_compile` takes: Go's tests count calls
//! on `newOverlayFS`, which Rust does not have. The overlaid program is derived with
//! `rotor_tsrs::project::update_with_overlays_checkers` over an overlay file system on tsrs's
//! cached host, so these tests check what a compile with overlays sees.

use std::collections::BTreeMap;

use super::{Temp, expect_compiled, large_stack, output, write_project};
use crate::ProjectOptions;
use crate::project::{
	compile_project_with_options, group_source_files_for_test, new_project_program_with_options, project_source_files,
};

fn overlay_options(temp: &Temp, rel: &str, text: &str) -> ProjectOptions {
	ProjectOptions {
		overlays: BTreeMap::from([(temp.join(rel).to_string_lossy().into_owned(), text.to_owned())]),
		..ProjectOptions::default()
	}
}

#[test]
#[ignore = "counts calls on Go's newOverlayFS metadata cache; Rust overlays a wrapvfs on tsrs's cached host"]
fn overlay_program_metadata_cached_once() {}

/// Each overlaid build sees the file system as it is now: a file added between two builds is in
/// the second build's program.
#[test]
fn overlay_program_cache_is_build_scoped() {
	let temp = write_project("@scope/overlay-scope", "");
	let opts = overlay_options(&temp, "src/main.ts", "export const value = 1;\n");
	let first = expect_compiled(large_stack(|| compile_project_with_options(&temp.dir(), &opts)));
	assert!(
		!first.contains_key("out/generated.luau"),
		"first program saw a file before it existed"
	);

	temp.write("src/generated.ts", "export const generated = true;\n");
	let second = expect_compiled(large_stack(|| compile_project_with_options(&temp.dir(), &opts)));
	assert!(
		second.contains_key("out/generated.luau"),
		"second program retained the first program's file set"
	);
}

/// The overlay's text wins over the file on disk.
#[test]
fn overlay_program_overlay_wins() {
	let temp = write_project("@scope/overlay-wins", "");
	temp.write("src/main.ts", "export const fromDisk = 1;\n");
	let opts = overlay_options(&temp, "src/main.ts", "export const transformed = true;\n");
	let files = expect_compiled(large_stack(|| compile_project_with_options(&temp.dir(), &opts)));
	let main = output(&files, "out/main.luau");
	assert!(
		main.contains("transformed") && !main.contains("fromDisk"),
		"overlay did not replace the file text:\n{main}"
	);
}

/// The checker groups of `dir`'s program, with `opts`.
fn checker_groups(dir: &str, opts: &ProjectOptions) -> usize {
	large_stack(|| {
		let (_, program) = match new_project_program_with_options(dir, "", opts) {
			Ok(loaded) => loaded,
			Err(failure) => panic!(
				"newProjectProgramWithOptions: {failure} (diags: {:?})",
				failure.messages()
			),
		};
		group_source_files_for_test(program, &project_source_files(program))
	})
}

/// Go builds the overlaid program fresh, with the configured checker count, so its checker
/// groups (and with them the declaration emit order) are the plain build's.
#[test]
fn overlay_program_keeps_the_configured_checker_count() {
	for (config, want) in [("", 4), (",\n\t\t\"checkers\": 2", 2)] {
		let temp = write_project("@scope/overlay-checkers", "");
		if !config.is_empty() {
			let tsconfig = std::fs::read_to_string(temp.join("tsconfig.json")).expect("the config reads");
			temp.write(
				"tsconfig.json",
				tsconfig.replacen("\n\t},\n\t\"include\"", &format!("{config}\n\t}},\n\t\"include\""), 1),
			);
		}
		for name in ["a", "b", "c", "d", "e", "f"] {
			temp.write(&format!("src/{name}.ts"), "export {};\n");
		}
		let plain = checker_groups(&temp.dir(), &ProjectOptions::default());
		let overlaid = checker_groups(
			&temp.dir(),
			&overlay_options(&temp, "src/a.ts", "export const changed = 1;\n"),
		);
		assert_eq!(plain, want, "{config:?}: plain build checker groups");
		assert_eq!(overlaid, want, "{config:?}: overlaid build checker groups");
	}
}
