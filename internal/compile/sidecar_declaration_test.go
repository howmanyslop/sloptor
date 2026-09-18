package compile

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"

	"rotor/internal/logservice"
)

const declarationMarkerPlugin = `const ts = require("typescript");

module.exports = function () {
	return (context) => (sourceFile) => {
		const marker = context.factory.createVariableStatement(undefined, context.factory.createVariableDeclarationList([
			context.factory.createVariableDeclaration("__DECLARATION_MARKER__", undefined, undefined, context.factory.createStringLiteral("after-declarations")),
		], ts.NodeFlags.Const));
		return context.factory.updateSourceFile(sourceFile, sourceFile.statements.concat([marker]));
	};
};
`

const insertStatementPlugin = `const ts = require("typescript");

module.exports = function () {
	return (context) => (sourceFile) => {
		const marker = context.factory.createVariableStatement(undefined, context.factory.createVariableDeclarationList([
			context.factory.createVariableDeclaration("__INJECTED__", undefined, undefined, context.factory.createStringLiteral("transformer-was-here")),
		], ts.NodeFlags.Const));
		return context.factory.updateSourceFile(sourceFile, [marker].concat(sourceFile.statements));
	};
};
`

// afterDeclarations transformers have nowhere to run now that declarations
// are emitted natively by tsgo (which has no custom-transformer hook), so the
// build must say so out loud rather than dropping the transform silently.
func TestAfterDeclarationsTransformerWarns(t *testing.T) {
	setRepoSidecarPath(t)
	closeSidecarSessions()
	dir := writeProject(t, "@scope/after-declarations-fixture", "")
	t.Cleanup(closeSidecarSessions)
	writeSidecarPluginFixture(t, dir, "", sidecarDeclarationConfig(`[{ "transform": "./plugins/declaration-marker.js", "afterDeclarations": true }]`))
	if err := os.WriteFile(filepath.Join(dir, "plugins", "declaration-marker.js"), []byte(declarationMarkerPlugin), 0o644); err != nil {
		t.Fatal(err)
	}
	log := captureCompilerLog(t)

	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil {
		t.Fatalf("build: %v (diags: %v)", err, diags)
	}
	if !strings.Contains(log.String(), "afterDeclarations transformers are not supported") {
		t.Fatalf("no afterDeclarations warning in compiler log:\n%s", log)
	}
	declaration, err := os.ReadFile(filepath.Join(dir, "out", "main.d.ts"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(declaration), "__DECLARATION_MARKER__") {
		t.Fatalf("afterDeclarations transformer ran:\n%s", declaration)
	}
	if strings.Contains(result.Outputs["out/main.luau"], "__DECLARATION_MARKER__") {
		t.Fatalf("declaration marker leaked into Luau:\n%s", result.Outputs["out/main.luau"])
	}
}

// resetAfterDeclarationsState clears the once-per-project sentinels so a test
// that expects the warning is not silenced by an earlier test in the package.
func resetAfterDeclarationsState() {
	afterDeclarationsWarned = sync.Map{}
	afterDeclarationsScanned = sync.Map{}
}

// captureCompilerLog redirects logservice output for the test's lifetime.
func captureCompilerLog(t *testing.T) *bytes.Buffer {
	t.Helper()
	buffer := &bytes.Buffer{}
	previous := logservice.Output
	logservice.Output = buffer
	t.Cleanup(func() { logservice.Output = previous })
	resetAfterDeclarationsState()
	t.Cleanup(resetAfterDeclarationsState)
	return buffer
}

func TestTransformerParentFix(t *testing.T) {
	setRepoSidecarPath(t)
	closeSidecarSessions()
	dir := writeProject(t, "@scope/parent-fix-fixture", "")
	t.Cleanup(closeSidecarSessions)
	plugin := `const ts = require("typescript");
function inject(context) {
	return (sourceFile) => ts.visitNode(sourceFile, function visit(node) {
		if (ts.isStringLiteral(node) && node.text === "start") return ts.factory.createStringLiteral("synthetic");
		return ts.visitEachChild(node, visit, context);
	});
}
function requireParent(context) {
	return (sourceFile) => ts.visitNode(sourceFile, function visit(node) {
		if (ts.isStringLiteral(node) && node.text === "synthetic" && !ts.isVariableDeclaration(node.parent)) throw new Error("missing synthetic parent");
		return ts.visitEachChild(node, visit, context);
	});
}
module.exports = function () { return { before: inject, after: requireParent }; };`
	writeSidecarPluginFixture(t, dir, "", sidecarDeclarationConfig(`[{ "transform": "./plugins/parent-fix.js" }]`))
	if err := os.WriteFile(filepath.Join(dir, "plugins", "parent-fix.js"), []byte(plugin), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const value = \"start\";\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil {
		t.Fatalf("build: %v (diags: %v)", err, diags)
	}
	if !strings.Contains(result.Outputs["out/main.luau"], `"synthetic"`) {
		t.Fatalf("parent-fix transform missing:\n%s", result.Outputs["out/main.luau"])
	}
}

func TestDeclarationPathRewrite(t *testing.T) {
	setRepoSidecarPath(t)
	closeSidecarSessions()
	dir := writeProject(t, "@scope/declaration-path-fixture", "")
	t.Cleanup(closeSidecarSessions)
	config := strings.Replace(
		sidecarDeclarationConfig(`[]`),
		`"outDir": "out",`,
		`"outDir": "out", "baseUrl": ".", "paths": { "@alias/*": ["src/*"] },`,
		1,
	)
	writeSidecarPluginFixture(t, dir, "", config)
	if err := os.WriteFile(filepath.Join(dir, "src", "value.ts"), []byte("export interface Value { label: string; }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("import type { Value } from \"@alias/value\";\nexport type Output = Value;\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil {
		t.Fatalf("build: %v (diags: %v)", err, diags)
	}
	declaration, err := os.ReadFile(filepath.Join(dir, "out", "main.d.ts"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(declaration), `from "./value"`) {
		t.Fatalf("declaration alias was not rewritten:\n%s", declaration)
	}
}

func TestTransformerSourceMapOriginalContent(t *testing.T) {
	// Catches batched transformer maps being assigned to the wrong emitted
	// source. Each map must still point to its independently authored text.
	setRepoSidecarPath(t)
	closeSidecarSessions()
	dir := writeProject(t, "@scope/transformer-source-map-fixture", "")
	t.Cleanup(closeSidecarSessions)
	config := strings.Replace(sidecarDeclarationConfig(`[{ "transform": "./plugins/insert.js" }]`), `"declaration": true,`, `"declaration": true, "sourceMap": true,`, 1)
	writeSidecarPluginFixture(t, dir, "", config)
	if err := os.WriteFile(filepath.Join(dir, "plugins", "insert.js"), []byte(insertStatementPlugin), 0o644); err != nil {
		t.Fatal(err)
	}
	originals := map[string]string{
		"main":  "export const value = \"start\";\n",
		"extra": "export const extra = \"second\";\n",
	}
	for name, original := range originals {
		if err := os.WriteFile(filepath.Join(dir, "src", name+".ts"), []byte(original), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	buildDir := dir
	alias := filepath.Join(t.TempDir(), "project")
	if err := os.Symlink(dir, alias); err == nil {
		buildDir = alias
	}
	if _, diags, err := BuildProjectWithOptions(buildDir, ProjectOptions{}); err != nil {
		t.Fatalf("build: %v (diags: %v)", err, diags)
	}
	for name, original := range originals {
		mapBytes, err := os.ReadFile(filepath.Join(dir, "out", name+".luau.map"))
		if err != nil {
			t.Fatal(err)
		}
		var sourceMap struct {
			SourcesContent []string `json:"sourcesContent"`
			Mappings       string   `json:"mappings"`
		}
		if err := json.Unmarshal(mapBytes, &sourceMap); err != nil {
			t.Fatal(err)
		}
		if len(sourceMap.SourcesContent) != 1 || sourceMap.SourcesContent[0] != original {
			t.Fatalf("%s sourcesContent = %q, want original source %q", name, sourceMap.SourcesContent, original)
		}
		if sourceMap.Mappings == "" {
			t.Fatalf("%s transformed source map has no mappings", name)
		}
	}
}

func sidecarDeclarationConfig(plugins string) string {
	return `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"declaration": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out",
		"plugins": ` + plugins + `
	},
	"include": ["src"]
}`
}

// The tsconfig-chain scan re-reads the whole `extends` graph, and the common
// answer is zero, so it has to be claimed once per project: a watch session
// round-trips on every keystroke.
func TestAfterDeclarationsScanIsClaimedOnce(t *testing.T) {
	resetAfterDeclarationsState()
	t.Cleanup(resetAfterDeclarationsState)
	const configPath = "C:/project/tsconfig.json"

	if !takeAfterDeclarationsScan(configPath) {
		t.Fatal("the first caller must own the scan")
	}
	for attempt := 0; attempt < 3; attempt++ {
		if takeAfterDeclarationsScan(configPath) {
			t.Fatalf("attempt %d re-ran the scan", attempt)
		}
	}
	if !takeAfterDeclarationsScan("C:/other/tsconfig.json") {
		t.Fatal("a different project must get its own scan")
	}
}

// Once the warning is out there is nothing left for the scan to discover.
func TestAfterDeclarationsScanIsSkippedAfterTheWarning(t *testing.T) {
	resetAfterDeclarationsState()
	t.Cleanup(resetAfterDeclarationsState)
	log := captureCompilerLog(t)
	const configPath = "C:/project/tsconfig.json"

	warnUnsupportedAfterDeclarations(configPath, 1)
	if !strings.Contains(log.String(), "afterDeclarations transformers are not supported") {
		t.Fatalf("no warning in the compiler log:\n%s", log)
	}
	if takeAfterDeclarationsScan(configPath) {
		t.Fatal("the scan ran after the warning was already out")
	}

	// And the warning itself is one line per project, not one per round trip.
	before := log.Len()
	warnUnsupportedAfterDeclarations(configPath, 2)
	if log.Len() != before {
		t.Fatalf("the warning repeated:\n%s", log)
	}
}
