//! Port of `internal/rojo/pathtranslator_test.go`.

use rotor_support::gopath;

use super::PathTranslator;

/// Joins path segments with the platform separator, mirroring how upstream's
/// Node `path` module would build the expected strings on the host OS.
fn j(parts: &[&str]) -> String {
	gopath::join(parts)
}

fn new_test_translator(use_luau: bool) -> PathTranslator {
	PathTranslator::new(&j(&["proj", "src"]), &j(&["proj", "out"]), "", false, use_luau)
}

#[test]
fn get_output_path() {
	let pt = new_test_translator(true);
	let tests = [
		// .ts -> .luau, rebased rootDir -> outDir (PathTranslator.ts L64-80).
		(
			"ts file",
			j(&["proj", "src", "foo.ts"]),
			j(&["proj", "out", "foo.luau"]),
		),
		(
			"tsx file",
			j(&["proj", "src", "foo.tsx"]),
			j(&["proj", "out", "foo.luau"]),
		),
		(
			"nested ts",
			j(&["proj", "src", "a", "b.ts"]),
			j(&["proj", "out", "a", "b.luau"]),
		),
		// index -> init rename.
		(
			"index",
			j(&["proj", "src", "index.ts"]),
			j(&["proj", "out", "init.luau"]),
		),
		(
			"nested index",
			j(&["proj", "src", "a", "index.ts"]),
			j(&["proj", "out", "a", "init.luau"]),
		),
		// sub-extensions are preserved (exts form a stack; only .ts(x) pops).
		(
			"server subext",
			j(&["proj", "src", "main.server.ts"]),
			j(&["proj", "out", "main.server.luau"]),
		),
		(
			"spec subext",
			j(&["proj", "src", "foo.spec.ts"]),
			j(&["proj", "out", "foo.spec.luau"]),
		),
		// .d.ts is NOT renamed (extsPeek(1) === D_EXT check), only rebased.
		(
			"d.ts skipped",
			j(&["proj", "src", "foo.d.ts"]),
			j(&["proj", "out", "foo.d.ts"]),
		),
		(
			"d.tsx skipped",
			j(&["proj", "src", "foo.d.tsx"]),
			j(&["proj", "out", "foo.d.tsx"]),
		),
		// Non-ts files only rebase.
		(
			"json passthrough",
			j(&["proj", "src", "data.json"]),
			j(&["proj", "out", "data.json"]),
		),
		(
			"luau passthrough",
			j(&["proj", "src", "raw.luau"]),
			j(&["proj", "out", "raw.luau"]),
		),
	];
	for (name, input, want) in tests {
		assert_eq!(pt.get_output_path(&input), want, "{name}: get_output_path({input:?})");
	}
}

#[test]
fn new_path_translator_adds_rbxtsc_build_info_infix() {
	let path = j(&["proj", "out", "tsconfig.tsbuildinfo"]);
	let pt = PathTranslator::new(&j(&["proj", "src"]), &j(&["proj", "out"]), &path, false, true);
	let want = j(&["proj", "out", "tsconfig.rbxtsc.tsbuildinfo"]);
	assert_eq!(pt.build_info_output_path, want);
	let pt = PathTranslator::new(
		&j(&["proj", "src"]),
		&j(&["proj", "out"]),
		&pt.build_info_output_path,
		false,
		true,
	);
	assert_eq!(pt.build_info_output_path, want, "idempotent build_info_output_path");
}

#[test]
fn get_output_path_lua_extension() {
	// useLuauExtension=false -> .lua (PathTranslator.ts L50-52).
	let pt = new_test_translator(false);
	let got = pt.get_output_path(&j(&["proj", "src", "foo.ts"]));
	assert_eq!(got, j(&["proj", "out", "foo.lua"]));
}

#[test]
fn get_import_path() {
	let pt = new_test_translator(true);
	let tests = [
		// Same as output path for plain .ts.
		(
			"ts file",
			j(&["proj", "src", "foo.ts"]),
			j(&["proj", "out", "foo.luau"]),
		),
		(
			"index",
			j(&["proj", "src", "index.ts"]),
			j(&["proj", "out", "init.luau"]),
		),
		// Unlike getOutputPath, .d is popped beneath .ts(x) (PathTranslator.ts L202-206).
		(
			"d.ts maps",
			j(&["proj", "src", "foo.d.ts"]),
			j(&["proj", "out", "foo.luau"]),
		),
		(
			"index.d.ts maps",
			j(&["proj", "src", "index.d.ts"]),
			j(&["proj", "out", "init.luau"]),
		),
		// Other sub-extensions are preserved.
		(
			"spec.d.ts",
			j(&["proj", "src", "foo.spec.d.ts"]),
			j(&["proj", "out", "foo.spec.luau"]),
		),
	];
	for (name, input, want) in tests {
		assert_eq!(
			pt.get_import_path(&input, false),
			want,
			"{name}: get_import_path({input:?}, false)"
		);
	}
}

#[test]
fn get_import_path_node_module() {
	// isNodeModule=true: the result stays a sibling of the input (no
	// rootDir->outDir rebase): node_modules d.ts files were already remapped
	// to the shipped .lua location (PathTranslator.ts L216-218). Use a rootDir
	// that contains node_modules so a buggy rebase would visibly move the path
	// into outDir.
	let pt = PathTranslator::new(&j(&["proj"]), &j(&["proj", "out"]), "", false, true);
	let input = j(&["proj", "node_modules", "@rbxts", "pkg", "src", "index.d.ts"]);
	let want = j(&["proj", "node_modules", "@rbxts", "pkg", "src", "init.luau"]);
	assert_eq!(pt.get_import_path(&input, true), want);
	// The same path with rebasing lands inside outDir (isNodeModule=false).
	let rebased = j(&["proj", "out", "node_modules", "@rbxts", "pkg", "src", "init.luau"]);
	assert_eq!(pt.get_import_path(&input, false), rebased);
	// Non-ts input joins unchanged.
	let input2 = j(&["proj", "node_modules", "@rbxts", "pkg", "main.lua"]);
	assert_eq!(pt.get_import_path(&input2, true), input2, "want unchanged");
}

#[test]
fn get_output_declaration_path() {
	let pt = new_test_translator(true);
	let tests = [
		(j(&["proj", "src", "foo.ts"]), j(&["proj", "out", "foo.d.ts"])),
		(j(&["proj", "src", "foo.tsx"]), j(&["proj", "out", "foo.d.ts"])),
		// An existing .d.ts only rebases.
		(j(&["proj", "src", "foo.d.ts"]), j(&["proj", "out", "foo.d.ts"])),
	];
	for (input, want) in tests {
		assert_eq!(
			pt.get_output_declaration_path(&input),
			want,
			"get_output_declaration_path({input:?})"
		);
	}
}

#[test]
fn get_output_transformed_path() {
	let pt = new_test_translator(true);
	let tests = [
		// .transformed inserted before the final ext (PathTranslator.ts L104-118).
		(j(&["proj", "src", "foo.ts"]), j(&["proj", "out", "foo.transformed.ts"])),
		// .d.ts: inserted before .d.
		(
			j(&["proj", "src", "foo.d.ts"]),
			j(&["proj", "out", "foo.transformed.d.ts"]),
		),
	];
	for (input, want) in tests {
		assert_eq!(
			pt.get_output_transformed_path(&input),
			want,
			"get_output_transformed_path({input:?})"
		);
	}
}

#[test]
fn get_input_paths() {
	let pt = new_test_translator(true);

	// foo.luau -> foo.ts, foo.tsx, foo.luau (identity last).
	assert_eq!(
		pt.get_input_paths(&j(&["proj", "out", "foo.luau"])),
		vec![
			j(&["proj", "src", "foo.ts"]),
			j(&["proj", "src", "foo.tsx"]),
			j(&["proj", "src", "foo.luau"])
		],
		"get_input_paths(foo.luau)"
	);

	// init.luau additionally maps back to index.ts(x).
	assert_eq!(
		pt.get_input_paths(&j(&["proj", "out", "init.luau"])),
		vec![
			j(&["proj", "src", "init.ts"]),
			j(&["proj", "src", "init.tsx"]),
			j(&["proj", "src", "index.ts"]),
			j(&["proj", "src", "index.tsx"]),
			j(&["proj", "src", "init.luau"]),
		],
		"get_input_paths(init.luau)"
	);

	// index.luau cannot come from a .ts file: identity only.
	assert_eq!(
		pt.get_input_paths(&j(&["proj", "out", "index.luau"])),
		vec![j(&["proj", "src", "index.luau"])],
		"get_input_paths(index.luau)"
	);

	// .d.ts reverse mapping only when declaration=true.
	let dpt = PathTranslator::new(&j(&["proj", "src"]), &j(&["proj", "out"]), "", true, true);
	assert_eq!(
		dpt.get_input_paths(&j(&["proj", "out", "foo.d.ts"])),
		vec![
			j(&["proj", "src", "foo.ts"]),
			j(&["proj", "src", "foo.tsx"]),
			j(&["proj", "src", "foo.d.ts"])
		],
		"get_input_paths(foo.d.ts, declaration)"
	);
}
