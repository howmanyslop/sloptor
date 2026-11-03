//! `luautypes_golden_test.go`: every fixture under `testdata/luautypes/src` rendered with
//! `luauTypes` on and compared byte for byte with `testdata/luautypes/golden/<same path>.luau`.
//! Go runs the fixtures as subtests of one test; here each fixture is its own test, so the ones
//! that reach another slice's stubs can wait for it.

use super::{luau_types_fixtures, render_luau_types};
use crate::tests::transformer_testdata;

/// `luauTypesGoldenPins`: lines that must appear in a fixture's rendered output no matter what
/// a golden refresh rewrites.
const LUAU_TYPES_GOLDEN_PINS: &[(&str, &[&str])] = &[
	(
		"src/locals.ts",
		&[
			"\tlocal count = 0\n",
			"\tlocal mode: \"r\" | \"w\" = \"r\"\n",
			"\tlocal label: string?\n",
			"\tlocal names = { \"a\", \"b\" }\n",
		],
	),
	(
		"src/emission.ts",
		&[
			"local fetched: (n: number) -> Promise<number> = TS.async(function(n: number): number\n",
			"local function luaTuple(): (number, string)\n",
			"local function nothing(): ()\n",
		],
	),
	(
		"src/classes.ts",
		&[
			"\tlocal box: Box<number> = pair\n",
			"export type Animal = {\n",
			"\tspeak: (self: Animal) -> string,\n",
			"\tfunction Animal.new(name: string): Animal\n",
			"\tfunction Animal.constructor(self: Animal, name: string)\n",
		],
	),
	(
		"src/imports.ts",
		&[
			"\ntype Shape = { kind: ShapeKind, area: (self: Shape) -> number }\n",
			"\ntype ShapeKind = \"circle\" | \"square\"\n",
		],
	),
	(
		"src/typefunctions.ts",
		&[
			"export type function Partial2(T: type)\n",
			"-- luauTypes: Unwrap<T> spelled as any: ",
			"export type Unwrap<T> = any\n",
		],
	),
];

/// `text` with the version of its `-- Compiled with sloptor v...` header line dropped.
fn without_version(text: &str) -> String {
	let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
	match first.strip_prefix("-- Compiled with sloptor v") {
		Some(_) => format!("-- Compiled with sloptor\n{rest}"),
		None => text.to_owned(),
	}
}

/// One fixture against its golden, plus its pins.
fn check_golden(rel_path: &str) {
	let name = rel_path
		.strip_prefix("src/")
		.and_then(|path| path.strip_suffix(".ts"))
		.expect("a src/*.ts fixture");
	let golden_path = transformer_testdata("luautypes")
		.join("golden")
		.join(format!("{name}.luau"));
	let want =
		std::fs::read_to_string(&golden_path).unwrap_or_else(|_| panic!("no golden at {}", golden_path.display()));
	let got = render_luau_types(rel_path, true);
	// The goldens were last refreshed at v3.0.0-beta.1 and Go's own test now fails on the
	// version in the header; only that line is compared without its version.
	assert_eq!(
		without_version(&got),
		without_version(&want),
		"{rel_path} differs from {}",
		golden_path.display()
	);
	for &(path, pins) in LUAU_TYPES_GOLDEN_PINS {
		if path != rel_path {
			continue;
		}
		for pin in pins {
			assert!(got.contains(pin), "{rel_path} output lost the pinned line {pin:?}");
		}
	}
}

#[test]
#[ignore = "needs slice vi: export declarations"]
fn luau_types_golden_aliases() {
	check_golden("src/aliases.ts");
}

#[test]
#[ignore = "needs slice vi: import declarations"]
fn luau_types_golden_classes() {
	check_golden("src/classes.ts");
}

#[test]
#[ignore = "needs slice vi: await expressions"]
fn luau_types_golden_emission() {
	check_golden("src/emission.ts");
}

#[test]
fn luau_types_golden_enums() {
	check_golden("src/enums.ts");
}

#[test]
fn luau_types_golden_functions() {
	check_golden("src/functions.ts");
}

#[test]
#[ignore = "needs slice vi: import declarations"]
fn luau_types_golden_imports() {
	check_golden("src/imports.ts");
}

#[test]
fn luau_types_golden_lib_shapes() {
	check_golden("src/lib/shapes.ts");
}

#[test]
#[ignore = "needs slice vi: import declarations"]
fn luau_types_golden_locals() {
	check_golden("src/locals.ts");
}

#[test]
fn luau_types_golden_primitives() {
	check_golden("src/primitives.ts");
}

#[test]
fn luau_types_golden_roblox() {
	check_golden("src/roblox.ts");
}

#[test]
#[ignore = "needs slice vi: import declarations"]
fn luau_types_golden_tables() {
	check_golden("src/tables.ts");
}

#[test]
fn luau_types_golden_typefunctions() {
	check_golden("src/typefunctions.ts");
}

#[test]
fn luau_types_golden_unions() {
	check_golden("src/unions.ts");
}

/// Every fixture has a test above, and a golden without a fixture fails, so a deleted fixture
/// cannot leave its output behind.
#[test]
fn luau_types_golden_fixture_set() {
	let fixtures = luau_types_fixtures();
	let covered = [
		"src/aliases.ts",
		"src/classes.ts",
		"src/emission.ts",
		"src/enums.ts",
		"src/functions.ts",
		"src/imports.ts",
		"src/lib/shapes.ts",
		"src/locals.ts",
		"src/primitives.ts",
		"src/roblox.ts",
		"src/tables.ts",
		"src/typefunctions.ts",
		"src/unions.ts",
	];
	assert_eq!(
		fixtures, covered,
		"a fixture under testdata/luautypes/src has no golden test"
	);
	let golden_root = transformer_testdata("luautypes").join("golden");
	let mut goldens = Vec::new();
	let mut pending = vec![golden_root.clone()];
	while let Some(dir) = pending.pop() {
		for entry in std::fs::read_dir(&dir).expect("the golden directory reads").flatten() {
			let path = entry.path();
			if path.is_dir() {
				pending.push(path);
			} else {
				let rel = path
					.strip_prefix(&golden_root)
					.expect("under the golden root")
					.to_string_lossy()
					.replace('\\', "/");
				goldens.push(rel);
			}
		}
	}
	for golden in goldens {
		let fixture = format!("src/{}", golden.trim_end_matches(".luau")) + ".ts";
		assert!(
			fixtures.contains(&fixture),
			"golden {golden} has no fixture under testdata/luautypes/src; delete it"
		);
	}
}
