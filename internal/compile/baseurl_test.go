package compile

import (
	"testing"
)

// ----------------------------------------------------------------------------
// baseUrl -> paths rewrite, end to end (Phase 3b Task 1).
//
// Ground truth: rbxtsc 3.0.0 (TS 5.5, native baseUrl support) compiled the
// same sources 2026-06-06 through testdata/diff/project with `"baseUrl":
// "src"` added to its tsconfig and the fake @rbxts/dummy package installed
// (oracle technique; scratch artifacts deleted after). The `want` strings
// below are that output VERBATIM. rotor's sanitizer strips baseUrl (removed
// in TS7) and injects `"paths": {"*": ["./src/*"]}`; this test proves the
// rewrite preserves both resolution behaviors that matter:
//   - a non-relative project-internal import ("shared/mod") resolves through
//     the injected wildcard and emits the same TS.import rbxPath, and
//   - a scoped package import ("@rbxts/dummy") still resolves through
//     node_modules even though the wildcard pattern matches it first
//     (tsgo's tryLoadModuleUsingPaths falls through on substitution miss).
// ----------------------------------------------------------------------------

func TestCompileProjectBaseURLPathsRewrite(t *testing.T) {
	files := compileRuntimeLibProject(t, "baseurl_model")

	wants := map[string]string{
		"out/main.luau": "-- Compiled with sloptor v2.4.0\n" +
			"local TS = require(script.Parent.include.RuntimeLib)\n" +
			"local _mod = TS.import(script, script.Parent, \"shared\", \"mod\")\n" +
			"local sharedFn = _mod.sharedFn\n" +
			"local SHARED_VALUE = _mod.SHARED_VALUE\n" +
			"local dummy = TS.import(script, script.Parent, \"node_modules\", \"@rbxts\", \"dummy\", \"out\").dummy\n" +
			"print(sharedFn(), SHARED_VALUE, dummy())\n" +
			"return nil\n",
		"out/shared/mod.luau": "-- Compiled with sloptor v2.4.0\n" +
			"local SHARED_VALUE = 5\n" +
			"local function sharedFn()\n" +
			"\treturn SHARED_VALUE * 2\n" +
			"end\n" +
			"return {\n" +
			"\tsharedFn = sharedFn,\n" +
			"\tSHARED_VALUE = SHARED_VALUE,\n" +
			"}\n",
	}
	for name, want := range wants {
		if got, ok := files[name]; !ok {
			t.Errorf("%s: missing from CompileProject output (%v)", name, keys(files))
		} else if got != want {
			t.Errorf("%s:\ngot:\n%s\nwant:\n%s", name, got, want)
		}
	}
	if len(files) != len(wants) {
		t.Errorf("produced %d files, want %d (%v)", len(files), len(wants), keys(files))
	}
}
