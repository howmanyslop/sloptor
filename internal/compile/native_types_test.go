package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The opt-in has two declaration surfaces with a fixed precedence:
// tsconfig.json's `rbxts.nativeTypes`, then rotor.toml's `nativeTypes`, then
// the caller's fallback (off unless set).
func TestEffectiveNativeTypesPrecedence(t *testing.T) {
	writeFile := func(t *testing.T, dir, name, contents string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(dir, name), []byte(contents), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	cases := []struct {
		name     string
		tsconfig string // "" leaves the file without a nativeTypes declaration
		toml     string // "" leaves the project without a rotor.toml
		fallback bool
		want     bool
	}{
		{name: "default off", want: false},
		{name: "fallback only", fallback: true, want: true},
		{name: "tsconfig true", tsconfig: `"nativeTypes": true`, want: true},
		{name: "tsconfig false beats fallback", tsconfig: `"nativeTypes": false`, fallback: true, want: false},
		{name: "rotor.toml true", toml: "nativeTypes = true\n", want: true},
		{name: "rotor.toml false beats fallback", toml: "nativeTypes = false\n", fallback: true, want: false},
		{name: "tsconfig true beats rotor.toml false", tsconfig: `"nativeTypes": true`, toml: "nativeTypes = false\n", want: true},
		{name: "tsconfig false beats rotor.toml true", tsconfig: `"nativeTypes": false`, toml: "nativeTypes = true\n", want: false},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			dir := t.TempDir()
			rbxts := ""
			if tc.tsconfig != "" {
				rbxts = `,"rbxts": {` + tc.tsconfig + `}`
			}
			writeFile(t, dir, "tsconfig.json", `{"compilerOptions": {}`+rbxts+`}`)
			if tc.toml != "" {
				writeFile(t, dir, "rotor.toml", tc.toml)
			}
			tsConfigPath := filepath.Join(dir, "tsconfig.json")

			if got := effectiveNativeTypes(dir, tsConfigPath, tc.fallback); got != tc.want {
				t.Errorf("effectiveNativeTypes = %v, want %v", got, tc.want)
			}
			// Without an active tsconfig the same precedence applies to the
			// remaining surfaces.
			if tc.tsconfig == "" {
				if got := effectiveNativeTypes(dir, "", tc.fallback); got != tc.want {
					t.Errorf("effectiveNativeTypes without an active tsconfig = %v, want %v", got, tc.want)
				}
			}
		})
	}
}

// The compile honours the resolved value end to end, not just the helper:
// starting from an option-free project, each surface flips the annotation and
// tsconfig beats rotor.toml in both directions.
func TestCompileFileNativeTypesOption(t *testing.T) {
	nativeSource := `//!native
export function scale(x: number, factor: number): number {
	return x * factor;
}
`

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
	tsconfig := func(rbxts string) string {
		declared := ""
		if rbxts != "" {
			declared = `,
	"rbxts": {"nativeTypes": ` + rbxts + `}`
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
	"include": ["src"]` + declared + `
}`
	}

	dir := writeProject(t, "@scope/native-types-e2e", "")
	writeFile(t, dir, filepath.Join("src", "native.ts"), nativeSource)
	render := func(t *testing.T) string {
		t.Helper()
		text, diags, err := CompileFileDetailedWithOptions(dir, "src/native.ts", ProjectOptions{})
		if err != nil {
			t.Fatalf("compile: %v (diagnostics: %v)", err, diags)
		}
		return text
	}

	if got := render(t); strings.Contains(got, ": number") || !strings.Contains(got, "--!native\n") {
		t.Fatalf("default output must keep the directive and stay unannotated:\n%s", got)
	}

	writeFile(t, dir, "rotor.toml", "nativeTypes = true\n")
	if got := render(t); !strings.Contains(got, "local function scale(x: number, factor: number): number\n") {
		t.Fatalf("rotor.toml nativeTypes = true did not annotate:\n%s", got)
	}

	// tsconfig wins over rotor.toml, including when it says false.
	writeFile(t, dir, "tsconfig.json", tsconfig("false"))
	if got := render(t); strings.Contains(got, ": number") {
		t.Fatalf("tsconfig nativeTypes = false must beat rotor.toml:\n%s", got)
	}

	writeFile(t, dir, "tsconfig.json", tsconfig("true"))
	if got := render(t); !strings.Contains(got, "local function scale(x: number, factor: number): number\n") {
		t.Fatalf("tsconfig nativeTypes = true did not annotate:\n%s", got)
	}

	// With the tsconfig silent, rotor.toml false is the effective value.
	if err := os.Remove(filepath.Join(dir, "rotor.toml")); err != nil {
		t.Fatal(err)
	}
	writeFile(t, dir, "rotor.toml", "nativeTypes = false\n")
	writeFile(t, dir, "tsconfig.json", tsconfig(""))
	if got := render(t); strings.Contains(got, ": number") {
		t.Fatalf("rotor.toml nativeTypes = false must stay unannotated:\n%s", got)
	}

	// A caller that sets the option directly (no project declaration at all)
	// gets it, so library builds and tests do not need a rotor.toml.
	if err := os.Remove(filepath.Join(dir, "rotor.toml")); err != nil {
		t.Fatal(err)
	}
	text, diags, err := CompileFileDetailedWithOptions(dir, "src/native.ts", ProjectOptions{NativeTypes: true})
	if err != nil {
		t.Fatalf("compile with ProjectOptions.NativeTypes: %v (diagnostics: %v)", err, diags)
	}
	if !strings.Contains(text, "local function scale(x: number, factor: number): number\n") {
		t.Fatalf("ProjectOptions.NativeTypes was ignored:\n%s", text)
	}
}
