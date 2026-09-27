package flamework

import (
	"context"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"rotor/internal/config"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
	"rotor/tsgo/printer"
)

func writeAddPathsFixture(t *testing.T, directory, main string) {
	t.Helper()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"include":["src/**/*.ts"]}`)
	writeTransformFixture(t, directory, "default.project.json", `{"name":"fixture","tree":{"$className":"DataModel","ServerScriptService":{"TS":{"$path":"out/server"}},"ReplicatedStorage":{"TS":{"$path":"out/shared"}}}}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/package.json", `{"name":"@flamework/core","types":"index.d.ts"}`)
	writeTransformFixture(t, directory, "node_modules/@flamework/core/index.d.ts", strings.Join([]string{
		`export declare namespace Modding {`,
		` type Intrinsic<N extends string, M extends unknown[], T = symbol> = T & { _flamework_intrinsic: [N, ...M] };`,
		`}`,
		`export declare namespace Flamework {`,
		` function _addPaths(paths: string[][]): void;`,
		` function _addPathsGlob(arg: string): void;`,
		` function ignite(): void;`,
		` /** @metadata macro intrinsic-arg-shift {@link _addPaths intrinsic-flamework-rewrite} */`,
		` function addPaths<T extends string>(path: T, meta?: Modding.Intrinsic<"path", [T]>): void;`,
		` /** @metadata macro intrinsic-arg-shift {@link _addPathsGlob intrinsic-flamework-rewrite} */`,
		` function addPathsGlob<T extends string>(path: T, meta?: Modding.Intrinsic<"pathglob", [T]>): void;`,
		`}`,
	}, "\n"))
	for _, name := range []string{
		"src/server/services/b.ts",
		"src/server/services/a.ts",
		"src/server/services/nested/index.ts",
		"src/server/services/runner.server.ts",
		"src/shared/components/y.ts",
		"src/shared/components/x.ts",
		"src/shared/components/types.d.ts",
		"src/shared/other.ts",
	} {
		writeTransformFixture(t, directory, name, "export {};\n")
	}
	writeTransformFixture(t, directory, "src/server/main.server.ts", main)
}

func transformAddPathsFixture(t *testing.T, directory string) (string, *Project) {
	t.Helper()
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	t.Cleanup(release)
	sourceFile := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/server/main.server.ts")))
	if sourceFile == nil {
		t.Fatal("source file not found")
	}
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	result, err := Transform(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{sourceFile}, Project: project})
	if err != nil {
		t.Fatalf("Transform() error = %v", err)
	}
	if len(result.Diagnostics) != 0 {
		t.Fatalf("diagnostics = %v", result.Diagnostics)
	}
	return printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).EmitSourceFile(result.Files[0]), project
}

// A bundler cannot follow Flamework's runtime requires of Instances, so
// top-level addPaths calls expand into static side-effect imports in place.
func TestTransform_expandsAddPathsToSideEffectImports_whenCallIsTopLevel(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeAddPathsFixture(t, directory, strings.Join([]string{
		`import { Flamework } from "@flamework/core";`,
		`Flamework.addPaths("src/server/services");`,
		`Flamework.addPathsGlob("./../shared/components/*.ts");`,
		`Flamework.ignite();`,
	}, "\n"))

	// When
	printed, project := transformAddPathsFixture(t, directory)

	// Then
	want := strings.Join([]string{
		`import { Flamework } from "@flamework/core";`,
		`import "./services/a";`,
		`import "./services/b";`,
		`import "./services/nested/index";`,
		`import "../shared/components/x";`,
		`import "../shared/components/y";`,
		`Flamework.ignite();`,
		``,
	}, "\n")
	if printed != want {
		t.Fatalf("transformed source =\n%s\nwant:\n%s", printed, want)
	}
	snapshot := project.BuildInfoSnapshot()
	if snapshot.Metadata == nil || snapshot.Metadata.Globs == nil || snapshot.Metadata.Globs.Origins == nil {
		t.Fatalf("glob registration = %#v, want origins for watch invalidation", snapshot.Metadata)
	}
	origins := (*snapshot.Metadata.Globs.Origins)["src/server/main.server.ts"]
	if !slices.Equal(origins, []string{"src/shared/components/*.ts"}) {
		t.Fatalf("origin globs = %#v, want only the addPathsGlob pattern", origins)
	}
}

func TestTransform_expandsAddPathsAtNestedCallSite(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeAddPathsFixture(t, directory, strings.Join([]string{
		`import { Flamework } from "@flamework/core";`,
		`function load() {`,
		`    const before = 1;`,
		`    Flamework.addPathsGlob("src/shared/components/*.ts");`,
		`    Flamework.addPaths("src/server/services");`,
		`    const after = 2;`,
		`}`,
		`load();`,
	}, "\n"))

	// When
	printed, _ := transformAddPathsFixture(t, directory)

	// Then
	if strings.Contains(printed, `_addPaths`) {
		t.Fatalf("transformed source =\n%s\nwant static imports", printed)
	}
	for _, module := range []string{"../shared/components/x", "../shared/components/y", "./services/a", "./services/b", "./services/nested/index"} {
		if !strings.Contains(printed, `import("`+module+`" + "`+transformer.FlameworkStaticImportMarker+`")`) {
			t.Fatalf("transformed source =\n%s\nmissing import %q", printed, module)
		}
	}
	if !strings.Contains(printed, "const before = 1;") || !strings.Contains(printed, "const after = 2;") {
		t.Fatalf("transformed source =\n%s\nlost statements around call", printed)
	}
}

func TestTransform_ignoresTsconfigExcludedMatches(t *testing.T) {
	// Given: test files exist in the Rojo source directory but are not compiled.
	directory := t.TempDir()
	writeAddPathsFixture(t, directory, strings.Join([]string{
		`import { Flamework } from "@flamework/core";`,
		`function load() { Flamework.addPaths("src/server/services"); }`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/server/services/player.test.ts", "export {};\n")
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"include":["src/**/*.ts"],"exclude":["src/**/*.test.ts"]}`)

	// When
	printed, _ := transformAddPathsFixture(t, directory)

	// Then
	if strings.Contains(printed, `_addPaths`) || strings.Contains(printed, "player.test") || !strings.Contains(printed, `import("./services/a" + "`+transformer.FlameworkStaticImportMarker+`")`) {
		t.Fatalf("transformed source =\n%s\nwant only compiled modules as imports", printed)
	}
}

func TestTransform_keepsUnbracedConditionalAroundExpandedCall(t *testing.T) {
	directory := t.TempDir()
	writeAddPathsFixture(t, directory, strings.Join([]string{
		`import { Flamework } from "@flamework/core";`,
		`function load(enabled: boolean) {`,
		`    if (enabled) Flamework.addPaths("src/server/services");`,
		`}`,
	}, "\n"))
	printed, _ := transformAddPathsFixture(t, directory)
	if !strings.Contains(printed, `if (enabled) {`) || !strings.Contains(printed, `import("./services/a" + "`+transformer.FlameworkStaticImportMarker+`")`) || strings.Contains(printed, `_addPaths`) {
		t.Fatalf("transformed source =\n%s\nwant conditional static imports", printed)
	}
}

func TestStaticAddPathsMatches_tracksCompiledFilesWithoutChangingRuntimeGlobs(t *testing.T) {
	directory := t.TempDir()
	writeAddPathsFixture(t, directory, strings.Join([]string{
		`import { Flamework } from "@flamework/core";`,
		`function load() { Flamework.addPaths("src/server/services"); }`,
	}, "\n"))
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"moduleResolution":"node","rootDir":"src","outDir":"out"},"include":["src/**/*.ts"],"exclude":["src/**/*.test.ts"]}`)
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatal(err)
	}

	before, err := StaticAddPathsMatches(newTransformProgram(t, directory), project)
	if err != nil {
		t.Fatal(err)
	}
	writeTransformFixture(t, directory, "src/server/services/new.ts", "export {};\n")
	writeTransformFixture(t, directory, "src/server/services/excluded.test.ts", "export {};\n")
	after, err := StaticAddPathsMatches(newTransformProgram(t, directory), project)
	if err != nil {
		t.Fatal(err)
	}
	pattern := "src/server/services/**"
	if slices.Contains(before[pattern], "./services/new") || !slices.Contains(after[pattern], "./services/new") || slices.Contains(after[pattern], "./services/excluded.test") {
		t.Fatalf("plain addPaths matches before=%v after=%v", before[pattern], after[pattern])
	}
	if project.BuildInfoSnapshot().Metadata.Globs != nil {
		t.Fatal("plain addPaths changed flamework.build runtime glob metadata")
	}
}

func TestTransform_removesAddPaths_whenFirstStatementMatchesNothing(t *testing.T) {
	// Given: the call precedes the (hoisted) import and matches no module.
	directory := t.TempDir()
	writeAddPathsFixture(t, directory, strings.Join([]string{
		`Flamework.addPathsGlob("src/missing/*.ts");`,
		`import { Flamework } from "@flamework/core";`,
	}, "\n"))

	// When
	printed, _ := transformAddPathsFixture(t, directory)

	// Then
	if want := "import { Flamework } from \"@flamework/core\";\n"; printed != want {
		t.Fatalf("transformed source =\n%s\nwant:\n%s", printed, want)
	}
}

// A hand-written Lua module is not a program source, so a static import cannot
// load it; the runtime call must stay.
func TestTransform_keepsRuntimeAddPaths_whenMatchIncludesLuaModule(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeAddPathsFixture(t, directory, strings.Join([]string{
		`import { Flamework } from "@flamework/core";`,
		`Flamework.addPaths("src/server/services");`,
	}, "\n"))
	writeTransformFixture(t, directory, "src/server/services/legacy.lua", "return {}\n")

	// When
	printed, _ := transformAddPathsFixture(t, directory)

	// Then
	if !strings.Contains(printed, `Flamework["_addPaths"](`) || strings.Contains(printed, `import "`) {
		t.Fatalf("transformed source =\n%s\nwant runtime _addPaths call", printed)
	}
}
