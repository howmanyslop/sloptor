package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// luauTypesEmitSource is the file every surface below compiles: one exported
// interface, which luauTypes turns into an `export type`, and one exported
// function whose signature it annotates. Nothing in it needs more than the
// noLib stubs.
const luauTypesEmitSource = `export interface Point {
	x: number;
	y: number;
}

export function scale(point: Point, factor: number): Point {
	return { x: point.x * factor, y: point.y * factor };
}
`

// luauTypesEmitAlias and luauTypesEmitSignature are the two lines that only
// the option-on output carries.
const (
	luauTypesEmitAlias     = "export type Point = { x: number, y: number }\n"
	luauTypesEmitSignature = "local function scale(point: Point, factor: number): Point\n"
)

// luauTypesEmitGlobals is noLibGlobalStubs with a member on the array
// interfaces. The checker's isArrayLikeType, which the type builder uses to
// spell `{ T }`, is assignability to ReadonlyArray<any>; against the empty
// stubs every object type passes and Point would be spelled `{ any }`, as it
// never is under @rbxts/types.
const luauTypesEmitGlobals = noLibGlobalStubs +
	"interface Array<T> { size(): number }\ninterface ReadonlyArray<T> { size(): number }\n"

// luauTypesEmitTsconfig is the fixture tsconfig with `rbxts.luauTypes` set
// to declared, or left undeclared for "".
func luauTypesEmitTsconfig(declared string) string {
	rbxts := ""
	if declared != "" {
		rbxts = `,
	"rbxts": {"luauTypes": ` + declared + `}`
	}
	return `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]` + rbxts + `
}`
}

// The compile honours the resolved luauTypes value end to end, through the
// public single-file path, not just the precedence helper: with the option
// on the output carries the alias and the annotated signature, and with it
// off, on every surface, the output is byte-identical to the option-free
// build. The surfaces walk the precedence order of effectiveLuauTypes:
// ProjectOptions.LuauTypes is the fallback, rotor.toml beats it, tsconfig
// beats rotor.toml, and the command line (SolutionArgv) beats everything.
func TestCompileFileLuauTypesOption(t *testing.T) {
	writeFile := func(t *testing.T, dir, name, contents string) {
		t.Helper()
		path := filepath.Join(dir, name)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	removeFile := func(t *testing.T, dir, name string) {
		t.Helper()
		if err := os.Remove(filepath.Join(dir, name)); err != nil {
			t.Fatal(err)
		}
	}

	dir := writeProject(t, "@scope/luau-types-e2e", "")
	writeFile(t, dir, filepath.Join("src", "globals.d.ts"), luauTypesEmitGlobals)
	writeFile(t, dir, filepath.Join("src", "point.ts"), luauTypesEmitSource)
	render := func(t *testing.T, opts ProjectOptions) string {
		t.Helper()
		text, diags, err := CompileFileDetailedWithOptions(dir, "src/point.ts", opts)
		if err != nil {
			t.Fatalf("compile: %v (diagnostics: %v)", err, diags)
		}
		return text
	}
	assertOn := func(t *testing.T, surface string, got string) {
		t.Helper()
		if !strings.Contains(got, luauTypesEmitAlias) || !strings.Contains(got, luauTypesEmitSignature) {
			t.Fatalf("%s: output lacks the alias or the annotated signature:\n%s", surface, got)
		}
	}

	baseline := render(t, ProjectOptions{})
	if strings.Contains(baseline, "export type") || strings.Contains(baseline, ": number") {
		t.Fatalf("the option-free output must stay unannotated:\n%s", baseline)
	}
	if !strings.Contains(baseline, "local function scale(point, factor)\n") {
		t.Fatalf("the option-free output lost the plain signature:\n%s", baseline)
	}
	assertOff := func(t *testing.T, surface string, got string) {
		t.Helper()
		if got != baseline {
			t.Fatalf("%s: output differs from the option-free build:\n--- got ---\n%s\n--- baseline ---\n%s", surface, got, baseline)
		}
	}

	// The caller's fallback alone turns the option on; no project file is
	// needed for library builds and tests.
	assertOn(t, "ProjectOptions.LuauTypes", render(t, ProjectOptions{LuauTypes: true}))

	writeFile(t, dir, "rotor.toml", "luauTypes = true\n")
	assertOn(t, "rotor.toml luauTypes = true", render(t, ProjectOptions{}))

	// tsconfig wins over rotor.toml, including when it says false.
	writeFile(t, dir, "tsconfig.json", luauTypesEmitTsconfig("false"))
	assertOff(t, "tsconfig luauTypes = false over rotor.toml true", render(t, ProjectOptions{}))

	writeFile(t, dir, "tsconfig.json", luauTypesEmitTsconfig("true"))
	assertOn(t, "tsconfig luauTypes = true", render(t, ProjectOptions{}))

	// The command line beats tsconfig in both directions.
	assertOff(t, "--luauTypes=false over tsconfig true", render(t, ProjectOptions{SolutionArgv: luauTypesArgv(false)}))
	writeFile(t, dir, "tsconfig.json", luauTypesEmitTsconfig("false"))
	assertOn(t, "--luauTypes over tsconfig false", render(t, ProjectOptions{SolutionArgv: luauTypesArgv(true)}))

	// An argv layer that does not mention the flag defers to the files.
	assertOff(t, "silent argv over tsconfig false", render(t, ProjectOptions{LuauTypes: true, SolutionArgv: &RbxtsOptions{}}))

	// With the tsconfig silent, rotor.toml false is the effective value and
	// beats the caller's fallback.
	writeFile(t, dir, "tsconfig.json", luauTypesEmitTsconfig(""))
	removeFile(t, dir, "rotor.toml")
	writeFile(t, dir, "rotor.toml", "luauTypes = false\n")
	assertOff(t, "rotor.toml luauTypes = false over ProjectOptions true", render(t, ProjectOptions{LuauTypes: true}))

	// Back to an option-free project: the output is the baseline again.
	removeFile(t, dir, "rotor.toml")
	assertOff(t, "option-free project", render(t, ProjectOptions{}))
}
