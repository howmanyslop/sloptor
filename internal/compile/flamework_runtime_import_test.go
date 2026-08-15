package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestNativeFlameworkPipeline_emitsJsxFactoryImport_whenReactIsOnlyUsedByJsx(t *testing.T) {
	// Given: a native Flamework project whose only React use is JSX.
	dir := writeProject(t, "@scope/flamework-jsx-react-import", "")
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("[flamework]\nnoSemanticDiagnostics = true\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(`{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"jsx": "react",
		"jsxFactory": "React.createElement",
		"jsxFragmentFactory": "React.Fragment",
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts", "node_modules/@flamework"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}`), 0o644); err != nil {
		t.Fatal(err)
	}
	globals, err := os.ReadFile(filepath.Join(dir, "src", "globals.d.ts"))
	if err != nil {
		t.Fatal(err)
	}
	globals = append(globals, []byte("\ndeclare namespace JSX {\n\tinterface IntrinsicElements {\n\t\tframe: {};\n\t}\n}\n")...)
	if err := os.WriteFile(filepath.Join(dir, "src", "globals.d.ts"), globals, 0o644); err != nil {
		t.Fatal(err)
	}
	writeRuntimeImportModule(t, dir, filepath.Join("node_modules", "@rbxts", "react"), `{
	"name": "@rbxts/react",
	"main": "src/index.ts",
	"types": "src/index.d.ts"
}`, `export function createElement(..._args: Array<unknown>): unknown;
export const Fragment: unknown;
declare const React: { createElement: typeof createElement; Fragment: unknown };
export default React;
`)
	if err := os.WriteFile(filepath.Join(dir, "src", "box.tsx"), []byte("import React from \"@rbxts/react\";\nexport function Box() {\n\treturn <frame />;\n}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: the full native Flamework print/overlay/reparse path emits Luau.
	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	// Then: the JSX factory import survives as a runtime binding.
	if err != nil || len(diags) != 0 {
		t.Fatalf("BuildProjectWithOptions: %v (%v)", err, diags)
	}
	got := result.Outputs["out/box.luau"]
	t.Logf("box.luau:\n%s", got)
	if !strings.Contains(got, "React.createElement") {
		t.Fatalf("emitted Luau missing JSX factory call:\n%s", got)
	}
	if !strings.Contains(got, "local React") || !strings.Contains(got, "TS.import") {
		t.Fatalf("emitted Luau dropped the React import used only by JSX:\n%s", got)
	}
}

func TestNativeFlameworkPipeline_emitsNamedImport_whenUsedOnlyAsValueAfterOverlay(t *testing.T) {
	// Given: native Flamework reprints every file and skips semantic diagnostics.
	dir := writeProject(t, "@scope/flamework-named-import-elision", "")
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("[flamework]\nnoSemanticDiagnostics = true\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	writeRuntimeImportModule(t, dir, filepath.Join("node_modules", "@rbxts", "t"), `{
	"name": "@rbxts/t",
	"main": "lib/t.d.ts",
	"types": "lib/t.d.ts"
}`, `export const t: { interface(shape: unknown): unknown };
`)
	if err := os.WriteFile(filepath.Join(dir, "src", "guards.ts"), []byte("import { t } from \"@rbxts/t\";\nexport const payloadGuard = t.interface({ label: t.string });\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: the full native Flamework print/overlay/reparse path emits Luau.
	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	// Then: the named import survives as a runtime binding.
	if err != nil || len(diags) != 0 {
		t.Fatalf("BuildProjectWithOptions: %v (%v)", err, diags)
	}
	got := result.Outputs["out/guards.luau"]
	t.Logf("guards.luau:\n%s", got)
	if !strings.Contains(got, "t.interface") && !strings.Contains(got, "t:interface") && !strings.Contains(got, `t["interface"]`) {
		t.Fatalf("emitted Luau missing t.interface use:\n%s", got)
	}
	if !strings.Contains(got, "local t =") || !strings.Contains(got, "TS.import") {
		t.Fatalf("emitted Luau dropped the named t import:\n%s", got)
	}
}

func TestNativeFlameworkPipeline_emitsGuardLibraryImport_whenCreateGuardExpands(t *testing.T) {
	// Given: native Flamework expands createGuard and reprints under noSemanticDiagnostics.
	dir := writeProject(t, "@scope/flamework-create-guard-import", "")
	symlinkTransformerFixtureModules(t, dir)
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("[flamework]\nnoSemanticDiagnostics = true\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	tsconfig, err := os.ReadFile(filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	updated := strings.Replace(string(tsconfig), `"typeRoots": ["node_modules/@rbxts"]`, `"typeRoots": ["node_modules/@rbxts", "node_modules/@flamework"]`, 1)
	if updated == string(tsconfig) {
		t.Fatal("typeRoots not found in writeProject tsconfig")
	}
	if err := os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(updated), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "guards.ts"), []byte("import { Flamework } from \"@flamework/core\";\ninterface Payload { readonly label: string; readonly count: number }\nexport const payloadGuard = Flamework.createGuard<Payload>();\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: the full native Flamework print/overlay/reparse path emits Luau.
	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	// Then: the synthesized guard-library import survives as a runtime binding.
	if err != nil || len(diags) != 0 {
		t.Fatalf("BuildProjectWithOptions: %v (%v)", err, diags)
	}
	got := result.Outputs["out/guards.luau"]
	t.Logf("guards.luau:\n%s", got)
	if !strings.Contains(got, "t.interface") && !strings.Contains(got, "t:interface") && !strings.Contains(got, `t["interface`) {
		t.Fatalf("emitted Luau missing expanded createGuard:\n%s", got)
	}
	if !strings.Contains(got, "local t =") || !strings.Contains(got, "TS.import") {
		t.Fatalf("emitted Luau dropped the synthesized t import:\n%s", got)
	}
}

func writeRuntimeImportModule(t *testing.T, dir, rel, packageJSON, types string) {
	t.Helper()
	root := filepath.Join(dir, rel)
	if err := os.MkdirAll(filepath.Join(root, "src"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "out"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "lib"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "package.json"), []byte(packageJSON), 0o644); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{
		filepath.Join("src", "index.ts"),
		filepath.Join("src", "index.d.ts"),
		filepath.Join("out", "index.ts"),
		filepath.Join("out", "index.d.ts"),
		filepath.Join("lib", "t.d.ts"),
	} {
		if err := os.WriteFile(filepath.Join(root, name), []byte(types), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}
