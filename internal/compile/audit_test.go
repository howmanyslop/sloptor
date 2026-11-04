package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ----------------------------------------------------------------------------
// Macro registration audit (Phase 3b Task 1, digest §6).
//
// Upstream's MacroManager constructor throws ProjectError the moment any
// registration name fails to resolve ("You may need to update your
// @rbxts/compiler-types!"). Rotor's MacroManager skips unresolvable names so
// checker-light test projects keep working, but records each skip; Compile*
// must fail hard with the upstream texts when registrations are missing
// WHILE the declaring packages are present (sentinel gating: LuaTuple for
// compiler-types, CFrame for @rbxts/types). Without this, a failed
// ResolveName silently regresses macros to plain method calls — the
// damage-numbers.ts bug class (`v.add(w)` -> wrong `v:add(w)` output, no
// diagnostic).
// ----------------------------------------------------------------------------

// buildAuditProject assembles a minimal Package-type project (scoped name =>
// no Rojo config or runtime lib needed) whose node_modules carry the REAL
// @rbxts/compiler-types and @rbxts/types from the differential fixture, so
// the gating sentinels resolve and the audit is live.
func buildAuditProject(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()

	fixtureRbxts, err := filepath.Abs(filepath.Join("..", "..", "testdata", "diff", "project", "node_modules", "@rbxts"))
	if err != nil {
		t.Fatal(err)
	}
	for _, pkg := range []string{"compiler-types", "types"} {
		copyDir(t, filepath.Join(fixtureRbxts, pkg), filepath.Join(dir, "node_modules", "@rbxts", pkg))
	}

	write := func(name, contents string) {
		t.Helper()
		path := filepath.Join(dir, name)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write("package.json", `{"name":"@audit/fixture"}`)
	write("tsconfig.json", `{
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
		"outDir": "out"
	},
	"include": ["src"]
}`)
	write(filepath.Join("src", "main.ts"), "export {};\n")
	return dir
}

func copyDir(t *testing.T, src, dst string) {
	t.Helper()
	err := filepath.WalkDir(src, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		target := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		return os.WriteFile(target, data, 0o644)
	})
	if err != nil {
		t.Fatal(err)
	}
}

// TestMacroRegistrationAudit: with intact packages the project compiles; with
// a renamed macro declaration (a "broken compiler-types" — the update-lag
// scenario the audit exists for) the compile fails loudly with the verbatim
// upstream ProjectError text instead of silently demacroizing call sites.
func TestMacroRegistrationAudit(t *testing.T) {
	dir := buildAuditProject(t)

	// Positive control: nothing missing, the compile succeeds.
	if _, diags, err := CompileFile(dir, "src/main.ts"); err != nil {
		t.Fatalf("intact project failed: %v (diags: %v)", err, diags)
	}

	// Break compiler-types: rename the classIs declaration, as a renamed or
	// outdated package would.
	callMacrosPath := filepath.Join(dir, "node_modules", "@rbxts", "compiler-types", "types", "callMacros.d.ts")
	data, err := os.ReadFile(callMacrosPath)
	if err != nil {
		t.Fatal(err)
	}
	broken := strings.Replace(string(data), "declare function classIs", "declare function classIsRenamed", 1)
	if broken == string(data) {
		t.Fatal("fixture drift: callMacros.d.ts has no classIs declaration")
	}
	if err := os.WriteFile(callMacrosPath, []byte(broken), 0o644); err != nil {
		t.Fatal(err)
	}

	const wantMsg = "MacroManager could not find symbol for classIs\nYou may need to update your @rbxts/compiler-types!"
	const wantErr = "compile: macro registration failure"

	_, diags, err := CompileFile(dir, "src/main.ts")
	if err == nil || err.Error() != wantErr {
		t.Fatalf("CompileFile error = %v, want %q (diags: %v)", err, wantErr, diags)
	}
	if len(diags) != 1 || diags[0] != wantMsg {
		t.Errorf("CompileFile diags = %q, want [%q]", diags, wantMsg)
	}

	_, diags, err = CompileProject(dir)
	if err == nil || err.Error() != wantErr {
		t.Fatalf("CompileProject error = %v, want %q (diags: %v)", err, wantErr, diags)
	}
	if len(diags) != 1 || diags[0] != wantMsg {
		t.Errorf("CompileProject diags = %q, want [%q]", diags, wantMsg)
	}
}

// TestMacroRegistrationAuditGating: checker-light projects without the types
// packages (the transformer unit-test setup — here imports_model, which has
// neither compiler-types nor @rbxts/types) must NOT fail the audit: every
// registration misses, but both sentinels are absent so Missing() gates to
// nil. The existing imports/runtimelib project tests cover this implicitly;
// this pins the gating explicitly.
func TestMacroRegistrationAuditGating(t *testing.T) {
	if _, diags, err := CompileFile(filepath.Join("..", "..", "testdata", "compile-unit", "imports_model"), "src/_scratch_once.ts"); err != nil {
		t.Fatalf("checker-light project failed the audit gate: %v (diags: %v)", err, diags)
	}
}

// TestMacroOnlyClassAssert: a compiler-types method on a MACRO-ONLY class
// (ReadonlyArray/Array/ReadonlyMap/WeakMap/Map/ReadonlySet/WeakSet/Set/
// String) with NO registered macro is upstream getPropertyCallMacro's
// `assert(false, "Macro X.y() is not implemented!")` — the
// compiler-types-newer-than-the-compiler scenario, simulated here by adding
// a `union` method to the copied compiler-types' OWN ReadonlySet declaration
// (it must be the single global declaration: upstream's identity check
// `symbols.get(parent.name) === parent` fails for a user-side augmentation,
// which merges into a transient clone and falls back to a plain method
// call). Rotor panics with the exact upstream text and CompileFile's recover
// boundary surfaces it as an internal-compiler-error — NEVER silence or a
// wrong `a:union(b)` method call. (Methods of NON-macro-only compiler-types
// classes, e.g. Promise.cancel, emit plain method calls — oracle-pinned in
// the 28_collectionmacros diff fixture.)
func TestMacroOnlyClassAssert(t *testing.T) {
	dir := buildAuditProject(t)

	setDtsPath := filepath.Join(dir, "node_modules", "@rbxts", "compiler-types", "types", "Set.d.ts")
	data, err := os.ReadFile(setDtsPath)
	if err != nil {
		t.Fatal(err)
	}
	const anchor = "interface ReadonlySet<T> extends Iterable<T> {"
	patched := strings.Replace(string(data), anchor,
		anchor+"\n\tunion(this: ReadonlySet<T>, other: ReadonlySet<T>): Set<T>;", 1)
	if patched == string(data) {
		t.Fatal("fixture drift: Set.d.ts has no ReadonlySet declaration")
	}
	if err := os.WriteFile(setDtsPath, []byte(patched), 0o644); err != nil {
		t.Fatal(err)
	}

	mainPath := filepath.Join(dir, "src", "main.ts")
	main := "const a = new Set<number>([1]);\nconst b = new Set<number>([2]);\nconst c = a.union(b);\nprint(c);\nexport {};\n"
	if err := os.WriteFile(mainPath, []byte(main), 0o644); err != nil {
		t.Fatal(err)
	}

	const wantErr = "internal compiler error: Macro ReadonlySet.union() is not implemented!"
	_, diags, err := CompileFile(dir, "src/main.ts")
	if err == nil || err.Error() != wantErr {
		t.Fatalf("CompileFile error = %v, want %q (diags: %v)", err, wantErr, diags)
	}
}
