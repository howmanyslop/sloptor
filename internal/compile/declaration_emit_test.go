package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/tsgo/sourcemap"
)

// buildDeclarationPathsFixture builds testdata/declpaths_model — a project with
// `paths`, `declaration`, and `declarationMap` — into a temp copy and returns
// the copy's directory.
func buildDeclarationPathsFixture(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	copyDir(t, filepath.Join("testdata", "declpaths_model"), dir)
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil {
		t.Fatalf("build: %v (diags: %v)", err, diags)
	}
	return dir
}

func readDeclarationFixtureFile(t *testing.T, dir, name string) string {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(dir, "out", name))
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}

// End-to-end: a project with `paths` publishes declarations whose specifiers
// the Luau runtime can actually resolve, with no Node worker involved.
func TestDeclarationEmitRewritesPathAliases(t *testing.T) {
	dir := buildDeclarationPathsFixture(t)
	declaration := readDeclarationFixtureFile(t, dir, "main.d.ts")

	if !strings.Contains(declaration, `from "./shared/mod"`) {
		t.Fatalf("path alias was not rewritten in an import declaration:\n%s", declaration)
	}
	// The `import("...")` type is synthesized by the declaration emitter for
	// the inferred return type, so its specifier is not in the file's import
	// list. The sidecar left these unresolved; the native route rewrites them.
	if !strings.Contains(declaration, `import("./shared/mod")`) {
		t.Fatalf("synthesized import type specifier was not rewritten:\n%s", declaration)
	}
	if strings.Contains(declaration, "@alias/") {
		t.Fatalf("an alias spelling survived into the declaration:\n%s", declaration)
	}
	// An external-library import resolves inside node_modules and must keep
	// its package spelling: the runtime resolves that one.
	if !strings.Contains(declaration, `"@rbxts/dummy"`) {
		t.Fatalf("external library import was rewritten:\n%s", declaration)
	}
}

// The `.d.ts.map` is written from the text tsgo printed, before the specifier
// splice, so every generated column past a rewritten specifier has to move
// with it. Editors are the only consumer, and a stale column lands the cursor
// mid-token.
func TestDeclarationMapColumnsFollowTheRewrittenSpecifier(t *testing.T) {
	dir := buildDeclarationPathsFixture(t)
	declaration := readDeclarationFixtureFile(t, dir, "main.d.ts")
	mapText := readDeclarationFixtureFile(t, dir, "main.d.ts.map")
	source, err := os.ReadFile(filepath.Join(dir, "src", "main.ts"))
	if err != nil {
		t.Fatal(err)
	}

	generatedLines := strings.Split(strings.ReplaceAll(declaration, "\r\n", "\n"), "\n")
	sourceLines := strings.Split(strings.ReplaceAll(string(source), "\r\n", "\n"), "\n")

	checkedSpecifier := false
	decoder := sourcemap.DecodeMappings(declarationMapMappings(t, mapText))
	for mapping := range decoder.Values() {
		generatedColumn := int(mapping.GeneratedCharacter)
		if mapping.GeneratedLine >= len(generatedLines) {
			t.Fatalf("mapping generated line %d is past the end of the declaration", mapping.GeneratedLine)
		}
		line := generatedLines[mapping.GeneratedLine]
		if generatedColumn > len(line) {
			t.Fatalf("mapping generated column %d is past the end of line %d (%q)", generatedColumn, mapping.GeneratedLine, line)
		}
		if !mapping.IsSourceMapping() {
			continue
		}
		if mapping.SourceLine >= len(sourceLines) || int(mapping.SourceCharacter) > len(sourceLines[mapping.SourceLine]) {
			continue
		}
		// The one position this test can pin exactly: where the source has the
		// aliased specifier, the generated text must have the rewritten one.
		if !strings.HasPrefix(sourceLines[mapping.SourceLine][int(mapping.SourceCharacter):], `"@alias/shared/mod"`) {
			continue
		}
		if !strings.HasPrefix(line[generatedColumn:], `"./shared/mod"`) {
			t.Fatalf("specifier mapping points at %q, want the rewritten specifier", line[generatedColumn:])
		}
		checkedSpecifier = true
	}
	if err := decoder.Error(); err != nil {
		t.Fatalf("decode declaration map mappings: %v", err)
	}
	if !checkedSpecifier {
		t.Fatal("no mapping covered the rewritten specifier")
	}
}

// Replaces the sidecar-era JS test "declaration path resolution observes
// filesystem mutations on the next request". The rewrite must resolve against
// the text the build is actually compiling, not against what is on disk: an
// import that exists only in a caller overlay still has to be rewritten.
func TestDeclarationEmitRewritesOverlaidImports(t *testing.T) {
	dir := t.TempDir()
	copyDir(t, filepath.Join("testdata", "declpaths_model"), dir)
	overlaid := filepath.Join(dir, "src", "main.ts")
	overlay := "import type { Shared } from \"@alias/shared/mod\";\n" +
		"export type Overlaid = Shared;\n"

	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Overlays: map[string]string{overlaid: overlay}}); err != nil {
		t.Fatalf("build: %v (diags: %v)", err, diags)
	}

	declaration := readDeclarationFixtureFile(t, dir, "main.d.ts")
	if !strings.Contains(declaration, `from "./shared/mod"`) {
		t.Fatalf("overlaid import was not rewritten:\n%s", declaration)
	}
	if strings.Contains(declaration, "@alias/") {
		t.Fatalf("an alias spelling survived into the declaration:\n%s", declaration)
	}
	if !strings.Contains(declaration, "Overlaid") {
		t.Fatalf("declaration was emitted from disk rather than from the overlay:\n%s", declaration)
	}
}

// Native declaration output is a downstream type boundary: consumers must be
// able to resolve and use it. Member and union order, synthesized-import
// spelling, exported alias choice, and tuple line layout are deliberately
// outside that contract because TypeScript treats each pair as equivalent.
func TestNativeDeclarationsTypeCheckInASeparateConsumer(t *testing.T) {
	root := t.TempDir()
	copyDir(t, filepath.Join("testdata", "declaration_consumer_contracts"), root)

	producer := filepath.Join(root, "producer")
	for _, project := range []string{producer, filepath.Join(root, "consumer")} {
		if err := os.MkdirAll(filepath.Join(project, "node_modules", "@rbxts"), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if _, diags, err := BuildProjectWithOptions(producer, ProjectOptions{EmitDeclarationOnly: true}); err != nil {
		t.Fatalf("emit producer declarations: %v (diags: %v)", err, diags)
	}

	consumer := filepath.Join(root, "consumer")
	if _, diags, err := BuildProjectWithOptions(consumer, ProjectOptions{EmitDeclarationOnly: true}); err != nil {
		t.Fatalf("type check declaration consumer: %v (diags: %v)", err, diags)
	}
}

// --emitDeclarationOnly still runs the transformer round trip even though it
// throws the transformed sources away, so that a plugin which fails to load
// still fails the build rather than silently publishing types.
func TestEmitDeclarationOnlyStillReportsAFailingPlugin(t *testing.T) {
	setRepoSidecarPath(t)
	closeSidecarSessions()
	dir := writeProject(t, "@scope/emit-declaration-only-plugin", "")
	t.Cleanup(closeSidecarSessions)
	writeSidecarPluginFixture(t, dir, "", sidecarDeclarationConfig(`[{ "transform": "./plugins/does-not-exist.js" }]`))

	_, diags, err := BuildProjectWithOptions(dir, ProjectOptions{EmitDeclarationOnly: true})
	if err == nil {
		t.Fatal("build succeeded with an unloadable transformer plugin")
	}
	joined := strings.Join(diags, "\n")
	if !strings.Contains(joined, "does-not-exist.js") {
		t.Fatalf("diagnostics do not name the failing plugin: %v", diags)
	}
}
