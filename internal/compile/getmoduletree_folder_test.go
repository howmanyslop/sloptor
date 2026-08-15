package compile

import (
	"testing"
)

// ----------------------------------------------------------------------------
// $getModuleTree on a FOLDER — rotor extension (no rbxtsc counterpart).
//
// Upstream rbxtsc 3.0.0 requires the macro's specifier to resolve as a
// module, so a folder works only if it contains an index.ts; upstream
// declared folder support out of scope ("implementing it for folders would
// be more work than worth it"). rotor accepts folders directly: when module
// resolution fails, the specifier is resolved to a source DIRECTORY —
// relative specifiers against the importing file's directory, non-relative
// ones through tsconfig `paths` (which covers baseUrl after the sanitizer
// rewrite) and then the project directory — and the macro emits that
// folder's instance path.
//
// This is a SUPERSET of upstream: everything that compiles under rbxtsc is
// untouched (the fallback only runs after upstream resolution failed, which
// would have been a hard diagnostic).
// ----------------------------------------------------------------------------

func TestGetModuleTreeFolder(t *testing.T) {
	files := compileRuntimeLibProject(t, "gmtree_folder_model")

	// The fixture is a model project: every rbxPath is relative
	// (getRelativeImport), and main.luau sits next to the shared/ and
	// systems2/ folders, so the folder roots are script.Parent chains.
	//   "shared/systems"     — baseUrl-relative (paths rewrite) folder
	//   "./systems2"         — source-file-relative folder
	//   "src/shared/systems" — project-root-relative folder (addPaths style)
	want := "-- Compiled with sloptor v2.3.0\n" +
		"print({ script.Parent, { \"shared\", \"systems\" } })\n" +
		"print({ script.Parent, { \"systems2\" } })\n" +
		"print({ script.Parent, { \"shared\", \"systems\" } })\n" +
		"return nil\n"
	if got := files["out/main.luau"]; got != want {
		t.Errorf("out/main.luau:\ngot:\n%s\nwant:\n%s", got, want)
	}
}
