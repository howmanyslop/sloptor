package main

import (
	"encoding/json"
	"path/filepath"
	"strings"
	"testing"
)

// writeCommonJSCheckableProject is writeCheckableProject with the scaffold's
// "module": "commonjs", the format TypeScript's generated-code name collision
// checks key on. files maps paths under src/ to their contents.
func writeCommonJSCheckableProject(t *testing.T, files map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	tsconfig := `{
	"compilerOptions": {
		"noLib": true,
		"strict": true,
		"target": "ESNext",
		"module": "commonjs",
		"moduleResolution": "Node",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}`
	mustWrite(t, filepath.Join(dir, "tsconfig.json"), tsconfig)
	mustWrite(t, filepath.Join(dir, "src", "globals.d.ts"), noLibGlobalStubs)
	for name, src := range files {
		mustWrite(t, filepath.Join(dir, "src", filepath.FromSlash(name)), src)
	}
	return dir
}

func checkJSON(t *testing.T, dir string) (jsonResult, int) {
	t.Helper()
	output, code := captureStdout(t, func() int {
		return cmdCheck([]string{"--json", dir})
	})
	var res jsonResult
	if err := json.Unmarshal([]byte(strings.TrimSpace(output)), &res); err != nil {
		t.Fatalf("output is not valid JSON: %v\noutput:\n%s", err, output)
	}
	return res, code
}

func TestCmdCheckAllowsTopLevelObjectBindingInCommonJSModule(t *testing.T) {
	// Given a CommonJS module that binds Object at top level, as
	// `import Object from "@rbxts/object-utils"` does
	dir := writeCommonJSCheckableProject(t, map[string]string{
		"lib/objutils.d.ts": "declare const ObjectUtils: { keys(o: object): string[] };\nexport default ObjectUtils;\n",
		"main.ts":           "import Object from \"./lib/objutils\";\n\nexport const k = Object.keys({});\n",
	})

	// When `sloptor check` typechecks it
	res, code := checkJSON(t, dir)

	// Then it is clean. TypeScript 7 reserves Object only to protect the
	// Object.defineProperty(exports, ...) preamble of its CommonJS JavaScript,
	// which Luau output never has.
	if code != 0 || len(res.Diagnostics) != 0 {
		t.Fatalf("cmdCheck exit = %d, want 0; diags: %+v", code, res.Diagnostics)
	}
}

func TestCmdCheckStillReservesRequireAndExportsInCommonJSModule(t *testing.T) {
	// Given CommonJS modules that bind require and exports at top level
	for _, src := range []string{
		"const require = 1;\nexport { require };\n",
		"const exports = 1;\nexport { exports };\n",
	} {
		dir := writeCommonJSCheckableProject(t, map[string]string{"main.ts": src})

		// When `sloptor check` typechecks them
		res, code := checkJSON(t, dir)

		// Then TS2441 still fires, as it does under rbxtsc
		if code != 1 || len(res.Diagnostics) == 0 {
			t.Fatalf("%q: cmdCheck exit = %d, want 1 with TS2441; diags: %+v", src, code, res.Diagnostics)
		}
		if got := res.Diagnostics[0].Code; got != "TS2441" {
			t.Errorf("%q: code = %q, want TS2441 (message %q)", src, got, res.Diagnostics[0].Message)
		}
	}
}
