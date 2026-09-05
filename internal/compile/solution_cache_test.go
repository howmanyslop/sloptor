package compile

import (
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"testing/synctest"

	"rotor/tsgo/ast"
)

func TestCacheableParseName(t *testing.T) {
	if cacheableParseName("src/main.ts") || cacheableParseName("src/main.tsx") {
		t.Fatal("ordinary TypeScript sources must not be cached across projects")
	}
	if !cacheableParseName("node_modules/@rbxts/types/index.d.ts") || !cacheableParseName("tsconfig.json") {
		t.Fatal("declaration and JSON files should be cacheable")
	}
}

func TestSolutionCompileCacheSingleflight(t *testing.T) {
	synctest.Test(t, func(t *testing.T) {
		cache := newSolutionCompileCache()
		var parses atomic.Int32
		opts := ast.SourceFileParseOptions{FileName: "shared.d.ts", Path: "shared.d.ts"}
		started := make(chan struct{}, 2)
		release := make(chan struct{})
		sentinel := &ast.SourceFile{}
		parse := func(o ast.SourceFileParseOptions) *ast.SourceFile {
			started <- struct{}{}
			<-release
			parses.Add(1)
			return sentinel
		}
		var wg sync.WaitGroup
		wg.Add(2)
		go func() {
			defer wg.Done()
			cache.getSourceFile(opts, parse)
		}()
		go func() {
			defer wg.Done()
			cache.getSourceFile(opts, parse)
		}()
		synctest.Wait()
		if len(started) != 1 {
			t.Fatalf("in-flight parses = %d, want 1", len(started))
		}
		close(release)
		wg.Wait()
		if parses.Load() != 1 {
			t.Fatalf("parses = %d, want 1", parses.Load())
		}
		if cache.misses.Load() != 1 || cache.hits.Load() < 1 {
			t.Fatalf("hits=%d misses=%d, want a miss then a hit", cache.hits.Load(), cache.misses.Load())
		}
	})
}

func TestSolutionParseCacheHitsSharedDeclarations(t *testing.T) {
	root := t.TempDir()
	sharedDir := filepath.Join(root, "shared")
	if err := os.MkdirAll(sharedDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(sharedDir, "index.d.ts"), []byte("export declare const shared: number;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./left", "./right"}, true)
	writeSharedDeclSolutionProject(t, filepath.Join(root, "left"), "../shared/index.d.ts")
	writeSharedDeclSolutionProject(t, filepath.Join(root, "right"), "../shared/index.d.ts")

	timings := NewBuildTimings()
	builders := 1
	if _, messages, err := BuildSolutionWithOptions(filepath.Join(root, "tsconfig.json"), ProjectOptions{Builders: &builders, Timings: timings}); err != nil {
		t.Fatalf("BuildSolutionWithOptions: %v (%v)", err, messages)
	}
	if timings.Counts.ParseCacheMisses == 0 {
		t.Fatal("parse cache misses = 0, want the shared declaration parsed at least once")
	}
	if timings.Counts.ParseCacheHits == 0 {
		t.Fatal("parse cache hits = 0, want the second project to reuse the shared declaration")
	}
}

// Catches sibling projects resolving the same package specifier through the
// other project's package.json when a solution shares package metadata.
func TestSolutionPackageJSONCacheKeepsSiblingPackagesDistinct(t *testing.T) {
	root := t.TempDir()
	writeSolutionConfig(t, root, "tsconfig.json", []string{"./left", "./right"}, true)
	writePackageJSONCacheProject(t, filepath.Join(root, "left"), "left")
	writePackageJSONCacheProject(t, filepath.Join(root, "right"), "right")

	builders := 1
	if _, messages, err := BuildSolutionWithOptions(filepath.Join(root, "tsconfig.json"), ProjectOptions{Builders: &builders}); err != nil {
		t.Fatalf("BuildSolutionWithOptions: %v (%v)", err, messages)
	}
}

// Catches a dependent project keeping the pre-build Rojo tree after its
// predecessor adds a nested output project, while the dependent loses its own
// persistent cache. The shared fixture tree declares that nested output under
// Generated, which is the required import path after the predecessor emits it.
func TestSolutionSharedRojoCacheRevalidatesPredecessorOutputAndPersistsPerProject(t *testing.T) {
	root, libDir, gameDir := writeCrossProjectSolution(t)
	for _, dir := range []string{filepath.Join(libDir, "out"), filepath.Join(gameDir, "out")} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatal(err)
		}
	}

	writeSolutionFile(t, root, "shared.project.json", `{"name":"shared","tree":{"$className":"DataModel","ReplicatedStorage":{"include":{"$path":"game/include"},"lib":{"$path":"lib/out"},"game":{"$path":"game/out"}}}}`)
	writeSolutionFile(t, libDir, "tsconfig.json", crossProjectCompilerOptions(true)+`,"rbxts":{"rojo":"../shared.project.json"},"include":["src"]}`)
	writeSolutionFile(t, gameDir, "tsconfig.json", crossProjectCompilerOptions(true)+`,"rbxts":{"rojo":"../shared.project.json","type":"game"},"include":["src"],"references":[{"path":"../lib"}]}`)
	writeSolutionFile(t, libDir, "src/default.project.json", `{"name":"generated","tree":{"Generated":{"regular":{"$path":"regular.luau"}}}}`)
	writeSolutionFile(t, gameDir, "src/index.ts", "import { regular } from \"../../lib/src/regular\";\nexport const value = regular();\n")

	builders := 1
	if _, messages, err := BuildSolutionWithOptions(filepath.Join(root, "tsconfig.json"), ProjectOptions{Builders: &builders}); err != nil {
		t.Fatalf("BuildSolutionWithOptions: %v (%v)", err, messages)
	}

	if _, err := os.Stat(filepath.Join(libDir, "out", "default.project.json")); err != nil {
		t.Fatalf("predecessor did not emit the nested Rojo project: %v", err)
	}
	gameOutput := string(mustReadFile(t, filepath.Join(gameDir, "out", "init.luau")))
	if !strings.Contains(gameOutput, `"lib", "Generated", "regular"`) {
		t.Fatalf("dependent output did not use the predecessor's nested Rojo mapping:\n%s", gameOutput)
	}

	for _, projectDir := range []string{libDir, gameDir} {
		cachePath := onlyCacheFile(t, filepath.Join(projectDir, ".rotor", "cache", "rojo"))
		info, err := os.Stat(cachePath)
		if err != nil {
			t.Fatal(err)
		}
		if !info.Mode().IsRegular() || info.Size() == 0 {
			t.Fatalf("Rojo cache for %s was not persisted as a non-empty regular file", projectDir)
		}
	}
}

func writeSharedDeclSolutionProject(t *testing.T, dir, sharedDecl string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Join(dir, "src"), 0o755); err != nil {
		t.Fatal(err)
	}
	config := `{"compilerOptions":{"allowSyntheticDefaultImports":true,"composite":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out"},"files":["src/main.ts","src/globals.d.ts","` + sharedDecl + `"]}`
	files := map[string]string{
		"tsconfig.json":    config,
		"package.json":     `{"name":"@scope/solution-child"}`,
		"src/globals.d.ts": noLibGlobalStubs,
		"src/main.ts":      "export const value = 1;\n",
	}
	for path, content := range files {
		if err := os.WriteFile(filepath.Join(dir, path), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}

func writePackageJSONCacheProject(t *testing.T, dir, value string) {
	t.Helper()
	files := map[string]string{
		"tsconfig.json":    `{"compilerOptions":{"allowSyntheticDefaultImports":true,"composite":true,"declaration":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out"},"include":["src"]}`,
		"package.json":     `{"name":"@scope/` + value + `"}`,
		"src/globals.d.ts": noLibGlobalStubs,
		"src/main.ts": `import { marker } from "@rbxts/fixture";
const observed: "` + value + `" = marker;
export { observed };
`,
		"node_modules/@rbxts/fixture/package.json": `{"name":"@rbxts/fixture","types":"index.d.ts","main":"init.luau"}`,
		"node_modules/@rbxts/fixture/index.d.ts": `export declare const marker: "` + value + `";
`,
		"node_modules/@rbxts/fixture/init.luau": "return {}\n",
	}
	for path, contents := range files {
		fullPath := filepath.Join(dir, filepath.FromSlash(path))
		if err := os.MkdirAll(filepath.Dir(fullPath), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(fullPath, []byte(contents), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}
