package compile

import (
	"os"
	"path/filepath"
	"testing"
)

// The experimental opt-in has three declaration surfaces with a fixed
// precedence: the command line's `--luauTypes` (the argv layer), then
// tsconfig.json's `rbxts.luauTypes`, then rotor.toml's `luauTypes`, then the
// caller's fallback (off unless set).
func TestEffectiveLuauTypesPrecedence(t *testing.T) {
	writeFile := func(t *testing.T, dir, name, contents string) {
		t.Helper()
		if err := os.WriteFile(filepath.Join(dir, name), []byte(contents), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	cases := []struct {
		name     string
		argv     *RbxtsOptions // nil, or a layer whose LuauTypes may itself be nil
		tsconfig string        // "" leaves the file without a luauTypes declaration
		toml     string        // "" leaves the project without a rotor.toml
		fallback bool
		want     bool
	}{
		{name: "default off", want: false},
		{name: "fallback only", fallback: true, want: true},
		{name: "tsconfig true", tsconfig: `"luauTypes": true`, want: true},
		{name: "tsconfig false beats fallback", tsconfig: `"luauTypes": false`, fallback: true, want: false},
		{name: "rotor.toml true", toml: "luauTypes = true\n", want: true},
		{name: "rotor.toml false beats fallback", toml: "luauTypes = false\n", fallback: true, want: false},
		{name: "tsconfig true beats rotor.toml false", tsconfig: `"luauTypes": true`, toml: "luauTypes = false\n", want: true},
		{name: "tsconfig false beats rotor.toml true", tsconfig: `"luauTypes": false`, toml: "luauTypes = true\n", want: false},
		{name: "argv true beats tsconfig false", argv: luauTypesArgv(true), tsconfig: `"luauTypes": false`, want: true},
		{name: "argv false beats tsconfig true", argv: luauTypesArgv(false), tsconfig: `"luauTypes": true`, want: false},
		{name: "argv true beats rotor.toml false", argv: luauTypesArgv(true), toml: "luauTypes = false\n", want: true},
		{name: "argv false beats rotor.toml true and fallback", argv: luauTypesArgv(false), toml: "luauTypes = true\n", fallback: true, want: false},
		{name: "argv without the flag defers to tsconfig", argv: &RbxtsOptions{}, tsconfig: `"luauTypes": true`, want: true},
		{name: "argv without the flag defers to rotor.toml", argv: &RbxtsOptions{}, toml: "luauTypes = false\n", fallback: true, want: false},
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

			if got := effectiveLuauTypes(dir, tsConfigPath, tc.argv, tc.fallback); got != tc.want {
				t.Errorf("effectiveLuauTypes = %v, want %v", got, tc.want)
			}
			// Without an active tsconfig the same precedence applies to the
			// remaining surfaces.
			if tc.tsconfig == "" {
				if got := effectiveLuauTypes(dir, "", tc.argv, tc.fallback); got != tc.want {
					t.Errorf("effectiveLuauTypes without an active tsconfig = %v, want %v", got, tc.want)
				}
			}
		})
	}
}

// luauTypesArgv is the command-line layer with `--luauTypes=<value>` passed.
func luauTypesArgv(value bool) *RbxtsOptions {
	return &RbxtsOptions{LuauTypes: &value}
}

// The project context carries the resolved value to every file's State, so
// the option reaches the transformer the same way whichever surface set it.
func TestProjectContextResolvesLuauTypes(t *testing.T) {
	dir := writeProject(t, "@scope/luau-types-context", "")
	absDir, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (diags: %v)", err, diags)
	}
	resolve := func(t *testing.T, opts ProjectOptions) bool {
		t.Helper()
		pctx, diags, err := newProjectContext(absDir, program, opts)
		if err != nil {
			t.Fatalf("newProjectContext: %v (diags: %v)", err, diags)
		}
		return pctx.luauTypes
	}

	if resolve(t, ProjectOptions{}) {
		t.Fatal("an option-free project must resolve luauTypes off")
	}
	if !resolve(t, ProjectOptions{LuauTypes: true}) {
		t.Fatal("ProjectOptions.LuauTypes was ignored")
	}

	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("luauTypes = true\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if !resolve(t, ProjectOptions{}) {
		t.Fatal("rotor.toml luauTypes = true did not reach the project context")
	}
	// The command line beats every file, including when it says false.
	if resolve(t, ProjectOptions{LuauTypes: true, SolutionArgv: luauTypesArgv(false)}) {
		t.Fatal("--luauTypes=false must beat rotor.toml and the fallback")
	}
}
