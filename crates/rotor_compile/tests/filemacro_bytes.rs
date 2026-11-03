//! A `$file` text file that is not valid UTF-8 (Windows-1252 here) keeps its raw bytes in the
//! output, as in Go, and source map columns count each invalid byte as one UTF-16 unit. The
//! expected bytes are the Go compiler's output for the same project.

use std::fs;
use std::path::Path;

use rotor_compile::ProjectOptions;
use rotor_compile::compile::compile_file_with_options;

use icu_normalizer as _;
use rotor_config as _;
use rotor_flamework as _;
use rotor_jecs as _;
use rotor_luau as _;
use rotor_rojo as _;
use rotor_support as _;
use rotor_transformer as _;
use serde_json as _;
use sha2 as _;

const TS_CONFIG: &str = r#"{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"SOURCE_MAP
	},
	"include": ["src"]
}"#;

const ROJO_CONFIG: &str = r#"{"name":"file-bytes","tree":{"$path":"out","include":{"$path":"include"},"node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}"#;

const HEADER: &[u8] = b"-- Compiled with sloptor v3.0.0-beta.2\n";

/// A project with the conformance project's `@rbxts` packages, `main.ts` and the data files.
#[expect(clippy::expect_used, reason = "a test fails loudly")]
fn project(main: &str, source_map: bool, files: &[(&str, &[u8])]) -> tempfile::TempDir {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/conformance/project");
	let temp = tempfile::Builder::new()
		.prefix(".file-bytes-")
		.tempdir_in(&root)
		.expect("a temporary project");
	let dir = temp.path();
	fs::write(dir.join("package.json"), r#"{"name":"file-bytes"}"#).expect("package.json");
	let tsconfig = TS_CONFIG.replace("SOURCE_MAP", if source_map { ",\n\t\t\"sourceMap\": true" } else { "" });
	fs::write(dir.join("tsconfig.json"), tsconfig).expect("tsconfig.json");
	fs::write(dir.join("default.project.json"), ROJO_CONFIG).expect("the Rojo project");
	fs::create_dir_all(dir.join("src")).expect("src");
	fs::create_dir_all(dir.join("include")).expect("include");
	fs::create_dir_all(dir.join("node_modules")).expect("node_modules");
	#[cfg(unix)]
	std::os::unix::fs::symlink(root.join("node_modules/@rbxts"), dir.join("node_modules/@rbxts"))
		.expect("the type roots");
	fs::write(dir.join("src/main.ts"), main).expect("main.ts");
	for (name, bytes) in files {
		fs::write(dir.join("src").join(name), bytes).expect("a data file");
	}
	temp
}

#[expect(clippy::panic, reason = "a test helper fails loudly")]
fn compile(dir: &Path) -> Vec<u8> {
	let (text, diagnostics) =
		compile_file_with_options(&dir.to_string_lossy(), "src/main.ts", &ProjectOptions::default())
			.unwrap_or_else(|failure| panic!("{failure}: {:?}", failure.messages()));
	assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
	text
}

#[test]
#[cfg(unix)]
fn windows_1252_text_keeps_its_bytes() {
	rotor_tsrs::with_large_stack(|| {
		let temp = project(
			"export const notes = $file(\"./notes.txt\");\nexport const quoted = $file(\"./quoted.txt\");\nexport const lines = $file(\"./lines.txt\");\n",
			false,
			&[
				("notes.txt", b"calf\xe9 \x93smart\x94 \x80uro"),
				("quoted.txt", b"say \"hi\" \xa9"),
				("lines.txt", b"line one \xe9\n]] two \xff"),
			],
		);
		let mut want = HEADER.to_vec();
		want.extend_from_slice(b"local notes = \"calf\xe9 \x93smart\x94 \x80uro\"\n");
		want.extend_from_slice(b"local quoted = 'say \"hi\" \xa9'\n");
		want.extend_from_slice(b"local lines = [=[line one \xe9\n]] two \xff]=]\n");
		want.extend_from_slice(b"return {\n\tnotes = notes,\n\tquoted = quoted,\n\tlines = lines,\n}\n");
		assert_eq!(compile(temp.path()), want);
	});
}

#[test]
#[cfg(unix)]
fn source_map_columns_count_invalid_bytes_as_one_unit() {
	rotor_tsrs::with_large_stack(|| {
		let temp = project(
			"declare function use(...args: unknown[]): void;\nconst after = 1;\nuse($file(\"./notes.txt\"), after);\n",
			true,
			&[("notes.txt", b"calf\xe9 \x93smart\x94 \x80uro")],
		);
		let dir = temp.path().to_string_lossy().into_owned();
		let outputs = match rotor_compile::compile_project(&dir) {
			Ok(outputs) => outputs,
			Err(failure) => panic!("{failure}: {:?}", failure.messages()),
		};
		let mut want = HEADER.to_vec();
		want.extend_from_slice(b"local after = 1\nuse(\"calf\xe9 \x93smart\x94 \x80uro\", after)\nreturn nil\n");
		assert_eq!(outputs.get("out/main.luau"), Some(&want));

		// compileProject keeps the map's sources absolute; the build rewrites them relative.
		let build = match rotor_compile::build_project_with_options(&dir, &ProjectOptions::default()) {
			Ok(build) => build,
			Err(failure) => panic!("{failure}"),
		};
		assert_eq!(build.outputs.get("out/main.luau"), Some(&want));
		let map = fs::read_to_string(temp.path().join("out/main.luau.map")).unwrap_or_default();
		assert_eq!(
			map,
			r#"{"version":3,"file":"main.luau","sources":["../src/main.ts"],"sourcesContent":["declare function use(...args: unknown[]): void;\nconst after = 1;\nuse($file(\"./notes.txt\"), after);\n"],"mappings":";AACA,cAAc;AACd,IAAI,sBAAsB"}"#
		);
	});
}
