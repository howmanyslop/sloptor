//! `flamework_noinclude_test.go`

use super::output::enable_incremental_builds;
use super::sidecar::build;
use super::task7_acceptance::task7_write;
use super::write_project;
use crate::project::ProjectOptions;

#[test]
fn build_flamework_include_artifacts_follow_emit_include_files() {
	for (name, emit_includes) in [("include enabled", true), ("noInclude", false)] {
		// Given: a native Flamework game with runtime config.
		let temp = write_project("noinclude-game", "");
		let dir = temp.dir();
		enable_incremental_builds(&temp);
		task7_write(&temp.join("rotor.toml"), "[flamework]\n");
		task7_write(
			&temp.join("default.project.json"),
			r#"{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"},"rbxts_include":{"$path":"include","node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}}}"#,
		);
		task7_write(&temp.join("flamework.json"), r#"{"profiling":false}"#);

		// When: the project builds with or without include emission.
		let opts = ProjectOptions {
			emit_include_files: emit_includes,
			..ProjectOptions::default()
		};
		let (_, diagnostics, error) = build(&dir, &opts);
		assert!(
			error.is_none() && diagnostics.is_empty(),
			"{name}: BuildProjectWithOptions = ({diagnostics:?}, {error:?})"
		);

		// Then: include/flamework follows the include switch; build info persists either way.
		for file in ["config.json", "globs.json"] {
			let exists = temp.join(&format!("include/flamework/{file}")).exists();
			if emit_includes && file == "config.json" {
				assert!(exists, "{name}: include/flamework/{file} missing");
			}
			if !emit_includes {
				assert!(!exists, "{name}: include/flamework/{file} written under noInclude");
			}
		}
		assert!(temp.join("flamework.build").exists(), "{name}: flamework.build missing");
	}
}
