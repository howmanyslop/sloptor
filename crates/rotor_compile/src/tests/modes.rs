//! The file modes Go writes with: outputs 0644 and directories 0755 (before the umask), and the
//! incremental and copy-files manifests chmodded to 0644 before their rename.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use super::output::{build, enable_incremental_builds};
use super::{Temp, write_project};
use crate::ProjectOptions;

fn mode(path: &Path) -> u32 {
	fs::metadata(path).expect("the path exists").permissions().mode() & 0o777
}

/// The process umask, read off a probe file created 0666.
fn umask(temp: &Temp) -> u32 {
	let probe = temp.join(".umask-probe");
	fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.mode(0o666)
		.open(&probe)
		.expect("the probe file");
	let umask = 0o666 & !mode(&probe);
	fs::remove_file(&probe).expect("the probe removes");
	umask
}

#[test]
fn manifests_are_0644_whatever_the_umask() {
	let temp = write_project("@scope/manifest-modes", "");
	enable_incremental_builds(&temp);
	build(&temp.dir(), &ProjectOptions::default());
	assert_eq!(
		mode(&temp.join("out/cache.rbxtsc.tsbuildinfo")),
		0o644,
		"incremental manifest"
	);
	assert_eq!(
		mode(&temp.join("out/rbxts.copyfiles.json")),
		0o644,
		"copy-files manifest"
	);
}

#[test]
fn outputs_are_0644_and_directories_0755_before_the_umask() {
	let temp = write_project("@scope/output-modes", "");
	temp.write("src/nested/deep.ts", "export const deep = 1;\n");
	let umask = umask(&temp);
	build(&temp.dir(), &ProjectOptions::default());
	assert_eq!(mode(&temp.join("out/main.luau")), 0o644 & !umask, "output file");
	assert_eq!(mode(&temp.join("out/nested")), 0o755 & !umask, "output directory");
}
