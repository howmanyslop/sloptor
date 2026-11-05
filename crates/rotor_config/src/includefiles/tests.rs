//! Port of `internal/includefiles/includefiles_test.go` and
//! `internal/includefiles/copy_unchanged_test.go`.

use std::fs;
use std::time::{Duration, SystemTime};

use rotor_support::gopath;

use super::{copy, names, read};

/// The exact compiler runtime payload.
const WANT_NAMES: [&str; 2] = ["Promise.lua", "RuntimeLib.lua"];

fn repo_root() -> String {
	gopath::join(&[env!("CARGO_MANIFEST_DIR"), "..", ".."])
}

fn temp_dir() -> (tempfile::TempDir, String) {
	let dir = tempfile::tempdir().expect("temp dir");
	let path = dir.path().to_str().expect("UTF-8").to_owned();
	(dir, path)
}

#[test]
fn names_are_the_runtime_payload() {
	assert_eq!(names(), WANT_NAMES);
}

/// Guards the drift hazard of vendoring the runtime twice: every embedded file
/// must match BOTH the repo-root `include/` file and
/// `reference/roblox-ts/include/` byte-for-byte, and `include/` must contain
/// no `.lua` file the embed is missing. Pinning against the reference copy
/// prevents the rotor copy drifting from upstream undetected.
#[test]
fn embedded_matches_vendored() {
	let root = repo_root();
	let vendored_dir = gopath::join(&[&root, "include"]);
	let reference_dir = gopath::join(&[&root, "reference", "roblox-ts", "include"]);

	for name in names() {
		let embedded = read(name).unwrap_or_else(|err| panic!("read({name:?}): {err}"));
		let vendored = fs::read(gopath::join(&[&vendored_dir, name]))
			.unwrap_or_else(|err| panic!("reading vendored {name}: {err}"));
		assert_eq!(
			embedded, vendored,
			"{name}: embedded copy differs from include/{name} - the vendored copies must stay byte-identical"
		);
		let upstream = fs::read(gopath::join(&[&reference_dir, name]))
			.unwrap_or_else(|err| panic!("reading reference {name}: {err}"));
		assert_eq!(
			embedded, upstream,
			"{name}: embedded copy differs from reference/roblox-ts/include/{name} - the runtime files must stay verbatim upstream"
		);
	}

	let lua_files = fs::read_dir(&vendored_dir)
		.expect("read include/")
		.filter_map(Result::ok)
		.filter(|entry| entry.file_name().to_string_lossy().ends_with(".lua"))
		.count();
	assert_eq!(
		lua_files,
		WANT_NAMES.len(),
		"include/ has {lua_files} .lua files, embed has {} - vendor both copies together",
		WANT_NAMES.len()
	);
}

#[test]
fn runtime_lib_allows_same_runtime_reload() {
	let runtime_lib = read("RuntimeLib.lua").expect("RuntimeLib.lua");
	let text = String::from_utf8_lossy(runtime_lib);
	assert!(
		text.contains("TS.__runtimeScript = script"),
		"RuntimeLib.lua does not identify reloads of the same runtime"
	);
	assert!(
		text.contains("registeredBy.__runtimeScript ~= script"),
		"RuntimeLib.lua rejects modules registered by a reload of the same runtime"
	);
}

fn le16(bytes: &[u8], at: usize) -> usize {
	usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
}

fn le32(bytes: &[u8], at: usize) -> usize {
	u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize
}

/// The contents of `name` in a zip archive, found through its central
/// directory. Handles stored and deflated entries, which is all
/// `roblox-ts.zip` uses.
fn zip_entry(zip: &[u8], name: &str) -> Option<Vec<u8>> {
	let eocd = zip.windows(4).rposition(|window| window == [0x50, 0x4b, 0x05, 0x06])?;
	let entries = le16(zip, eocd + 10);
	let mut at = le32(zip, eocd + 16);
	for _ in 0..entries {
		assert_eq!(&zip[at..at + 4], [0x50, 0x4b, 0x01, 0x02], "central directory header");
		let method = le16(zip, at + 10);
		let compressed_size = le32(zip, at + 20);
		let name_len = le16(zip, at + 28);
		let extra_len = le16(zip, at + 30);
		let comment_len = le16(zip, at + 32);
		let local = le32(zip, at + 42);
		let entry_name = &zip[at + 46..at + 46 + name_len];
		at += 46 + name_len + extra_len + comment_len;
		if entry_name != name.as_bytes() {
			continue;
		}
		let data_start = local + 30 + le16(zip, local + 26) + le16(zip, local + 28);
		let data = &zip[data_start..data_start + compressed_size];
		return match method {
			0 => Some(data.to_vec()),
			8 => miniz_oxide::inflate::decompress_to_vec(data).ok(),
			_ => None,
		};
	}
	None
}

#[test]
fn include_files_match_fork() {
	let archive = fs::read(gopath::join(&[&repo_root(), "roblox-ts.zip"])).expect("roblox-ts.zip");

	for name in names() {
		let fork_bytes = zip_entry(&archive, &format!("roblox-ts/include/{name}"))
			.unwrap_or_else(|| panic!("fork archive is missing {name}"));
		let embedded = read(name).expect("embedded");
		assert!(embedded == fork_bytes, "{name} differs from the fork archive");
	}
}

/// Exercises the `fs.copySync` port (`copyInclude.ts` L12-14): files land in a
/// fresh directory, and an existing directory is merged into (stale runtime
/// files overwritten, unrelated files untouched).
#[test]
fn copy_merges_into_destination() {
	let (_guard, root) = temp_dir();
	let dest = gopath::join(&[&root, "include"]);

	// Fresh directory: created, fully populated.
	copy(&dest).expect("copy");
	for name in WANT_NAMES {
		let got =
			fs::read(gopath::join(&[&dest, name])).unwrap_or_else(|err| panic!("after copy, reading {name}: {err}"));
		assert!(
			got == read(name).expect("embedded"),
			"{name}: copied bytes differ from embedded bytes"
		);
	}

	// Existing directory: a stale RuntimeLib.lua is overwritten, an unrelated
	// user file survives (fs.copySync merges, never clears the destination).
	fs::write(gopath::join(&[&dest, "RuntimeLib.lua"]), "-- stale\n").expect("write");
	fs::write(gopath::join(&[&dest, "user.lua"]), "-- mine\n").expect("write");
	copy(&dest).expect("copy over existing dir");
	let got = fs::read(gopath::join(&[&dest, "RuntimeLib.lua"])).expect("read");
	assert!(
		got == read("RuntimeLib.lua").expect("embedded"),
		"RuntimeLib.lua: stale file was not overwritten"
	);
	let user = fs::read_to_string(gopath::join(&[&dest, "user.lua"]));
	assert_eq!(user.as_deref().ok(), Some("-- mine\n"), "user.lua was disturbed");
}

#[test]
fn copy_keeps_unchanged_file_mtimes() {
	let (_guard, dir) = temp_dir();
	copy(&dir).expect("copy");
	let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
	for name in names() {
		fs::File::options()
			.write(true)
			.open(gopath::join(&[&dir, name]))
			.expect("open")
			.set_modified(old)
			.expect("set mtime");
	}
	copy(&dir).expect("copy again");
	for name in names() {
		let modified = fs::metadata(gopath::join(&[&dir, name]))
			.expect("stat")
			.modified()
			.expect("mtime");
		assert_eq!(modified, old, "{name} rewritten");
	}
}
