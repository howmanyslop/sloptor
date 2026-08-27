package compile

import (
	"os"
	"path/filepath"
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
