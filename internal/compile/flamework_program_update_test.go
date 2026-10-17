package compile

import (
	"os"
	"path/filepath"
	"testing"

	"rotor/internal/fsutil"
	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/osvfs"
	"rotor/tsgo/vfs/vfstest"
	"rotor/tsgo/vfs/wrapvfs"
)

func TestNativeOverlayHostPreservesSharedParses(t *testing.T) {
	for fileName, text := range map[string]string{
		"/project/shared.d.ts": "export declare const shared: number;",
		"/project/data.json":   `{"shared": 1}`,
	} {
		t.Run(filepath.Base(fileName), func(t *testing.T) {
			reads := 0
			baseFS := vfstest.FromMap(map[string]string{fileName: text}, true)
			fs := wrapvfs.Wrap(baseFS, wrapvfs.Replacements{ReadFile: func(path string) (string, bool) {
				reads++
				return baseFS.ReadFile(path)
			}})
			cache := newSolutionCompileCache()
			base := cache.wrap(compiler.NewCompilerHost("/project", fs, "", nil, nil), nil)
			opts := ast.SourceFileParseOptions{FileName: fileName, Path: tspath.Path(fileName)}
			original := base.GetSourceFile(opts)
			if original == nil {
				t.Fatal("base host did not parse the shared file")
			}
			host := newNativeOverlayHost(base, map[string]string{"/project/other.ts": "export const other = 2;"})
			for range 2 {
				if got := host.GetSourceFile(opts); got != original {
					t.Fatal("unrelated overlay reparsed a shared declaration/JSON file")
				}
			}
			if reads != 1 || cache.hits.Load() != 2 || cache.misses.Load() != 1 {
				t.Fatalf("reads=%d hits=%d misses=%d, want 1 read, 2 hits, 1 miss", reads, cache.hits.Load(), cache.misses.Load())
			}
			for _, variant := range []ast.SourceFileParseOptions{
				{FileName: fileName, Path: tspath.Path("/another" + fileName)},
				{FileName: fileName, Path: opts.Path, ExternalModuleIndicatorOptions: ast.ExternalModuleIndicatorOptions{Force: true}},
			} {
				parsed := host.GetSourceFile(variant)
				if parsed == nil || parsed == original || parsed.ParseOptions() != variant {
					t.Fatal("overlay host did not preserve distinct source parse options")
				}
				if host.GetSourceFile(variant) != parsed {
					t.Fatal("parse-options variant was not shared through the underlying cache")
				}
			}
		})
	}
}

func TestNativeOverlayHostKeepsTransformedFilesOutOfSharedCache(t *testing.T) {
	for _, fixture := range []struct {
		fileName, original, transformed string
	}{
		{"/project/shared.d.ts", "export declare const original: number;", "export declare const transformed: string;"},
		{"/project/data.json", `{"original": 1}`, `{"transformed": 2}`},
	} {
		t.Run(filepath.Base(fixture.fileName), func(t *testing.T) {
			cache := newSolutionCompileCache()
			fs := vfstest.FromMap(map[string]string{fixture.fileName: fixture.original}, true)
			base := cache.wrap(compiler.NewCompilerHost("/project", fs, "", nil, nil), nil)
			opts := ast.SourceFileParseOptions{FileName: fixture.fileName, Path: tspath.Path(fixture.fileName)}
			original := base.GetSourceFile(opts)
			if original == nil {
				t.Fatal("base host did not parse the original file")
			}
			host := newNativeOverlayHost(base, map[string]string{fixture.fileName: fixture.transformed})
			first := host.GetSourceFile(opts)
			second := host.GetSourceFile(opts)
			if first == nil || second == nil || first == original || first == second || first.Text() != fixture.transformed || second.Text() != fixture.transformed || first.ParseOptions() != opts {
				t.Fatal("transformed declaration/JSON was not freshly parsed from its overlay")
			}
			if text, ok := host.FS().ReadFile(fixture.fileName); !ok || text != first.Text() {
				t.Fatal("overlay filesystem and parsed source disagree")
			}
			if got := base.GetSourceFile(opts); got != original || got.Text() != fixture.original {
				t.Fatal("transformed file contaminated the underlying shared parse cache")
			}
			if cache.misses.Load() != 1 || cache.hits.Load() != 1 {
				t.Fatalf("overlay used shared cache: hits=%d misses=%d", cache.hits.Load(), cache.misses.Load())
			}
		})
	}
}

func TestNativeOverlayHostPreservesCustomHostAndParseOptions(t *testing.T) {
	opts := ast.SourceFileParseOptions{
		FileName: "/project/custom.tsx", Path: "/canonical/custom.tsx",
		ExternalModuleIndicatorOptions: ast.ExternalModuleIndicatorOptions{JSX: true, Force: true},
	}
	original := parser.ParseSourceFile(opts, "const custom = <Frame />;", core.ScriptKindTSX)
	baseCalls := 0
	base := nativeOverlayTestHost{
		CompilerHost: compiler.NewCompilerHost("/project", vfstest.FromMap(map[string]string{}, true), "", nil, nil),
		getSourceFile: func(got ast.SourceFileParseOptions) *ast.SourceFile {
			baseCalls++
			if got != opts {
				t.Fatalf("underlying host received parse options %+v, want %+v", got, opts)
			}
			return original
		},
	}
	const transformedPath = "/project/transformed.tsx"
	host := newNativeOverlayHost(base, map[string]string{transformedPath: "const transformed = <Frame />;"})
	if host.FS().FileExists(opts.FileName) || host.GetSourceFile(opts) != original {
		t.Fatal("overlay did not preserve the custom host's source absent from its filesystem")
	}
	transformedOpts := opts
	transformedOpts.FileName = transformedPath
	transformedOpts.Path = "/canonical/transformed.tsx"
	parsed := host.GetSourceFile(transformedOpts)
	if parsed == nil || parsed.ParseOptions() != transformedOpts || parsed.ScriptKind != core.ScriptKindTSX || parsed.ExternalModuleIndicator == nil {
		t.Fatal("transformed source lost its filename, canonical path, module indicators or script kind")
	}
	if baseCalls != 1 {
		t.Fatal("overlay hit unexpectedly invoked the custom base host")
	}
}

type nativeOverlayTestHost struct {
	compiler.CompilerHost
	getSourceFile func(ast.SourceFileParseOptions) *ast.SourceFile
}

func (host nativeOverlayTestHost) GetSourceFile(opts ast.SourceFileParseOptions) *ast.SourceFile {
	return host.getSourceFile(opts)
}

func TestNativeOverlayHostPreservesProjectCacheExclusions(t *testing.T) {
	dir := writeProject(t, "@scope/overlay-exclusions", "")
	configPath := filepath.ToSlash(filepath.Join(dir, "tsconfig.json"))
	callerPath := filepath.Join(dir, "src", "caller.d.ts")
	if err := os.WriteFile(callerPath, []byte("export declare const disk: number;"), 0o644); err != nil {
		t.Fatal(err)
	}
	const callerText = "export declare const caller: string;"
	cache := newSolutionCompileCache()
	_, program, diagnostics, err := newProjectProgramWithOptions(dir, "", ProjectOptions{
		compileCache: cache,
		Overlays:     map[string]string{callerPath: callerText},
	})
	if err != nil {
		t.Fatalf("newProjectProgramWithOptions: %v (%v)", err, diagnostics)
	}
	host := newNativeOverlayHost(program.Host(), map[string]string{"/project/unrelated.ts": "export {};"})
	hits, misses := cache.hits.Load(), cache.misses.Load()
	for fileName, text := range map[string]string{
		envDeclPath(configPath): envDeclText, assetDeclPath(configPath): assetDeclText,
		macroDeclPath(configPath): macroDeclText, callerPath: callerText,
	} {
		source := program.GetSourceFile(fileName)
		if source == nil {
			t.Fatalf("project declaration not loaded: %s", fileName)
		}
		opts := source.ParseOptions()
		first, second := host.GetSourceFile(opts), host.GetSourceFile(opts)
		if first == nil || second == nil || first == second || first.Text() != text || second.Text() != text {
			t.Fatalf("synthetic/caller declaration no longer isolated: %s", fileName)
		}
	}
	if cache.hits.Load() != hits || cache.misses.Load() != misses {
		t.Fatal("synthetic/caller declaration was admitted to the shared parse cache")
	}
}

func TestNativeOverlayHostPreservesCaseInsensitiveCallerExclusion(t *testing.T) {
	const fileName = "/project/Caller.d.ts"
	const transformed = "export declare const caller: string;"
	opts := ast.SourceFileParseOptions{FileName: fileName, Path: "/project/caller.d.ts"}
	fs := vfstest.FromMap(map[string]string{fileName: "export declare const disk: number;"}, false)
	cache := newSolutionCompileCache()
	base := compiler.NewCompilerHost("/project", fs, "", nil, nil)
	if cache.wrap(base, nil).GetSourceFile(opts) == nil {
		t.Fatal("initial disk declaration was not cached")
	}
	callerFS := newOverlayFS(fs, "", map[string]string{"/PROJECT/cALLER.d.ts": transformed})
	callerHost := cache.wrap(compiler.NewCompilerHost("/project", callerFS, "", nil, nil), map[string]struct{}{
		normalizeSourceFilePath("/PROJECT/cALLER.d.ts"): {},
	})
	host := newNativeOverlayHost(callerHost, map[string]string{"/project/unrelated.ts": "export {};"})
	first, second := host.GetSourceFile(opts), host.GetSourceFile(opts)
	if first == nil || second == nil || first == second || first.Text() != transformed || second.Text() != transformed {
		t.Fatal("case-insensitive caller declaration used the primed disk parse cache")
	}
	if cache.hits.Load() != 0 || cache.misses.Load() != 1 {
		t.Fatal("excluded caller declaration touched the shared parse cache")
	}
}

func TestNativeOverlayHostStacksOverlaysAndAliases(t *testing.T) {
	cache := newSolutionCompileCache()
	base := cache.wrap(compiler.NewCompilerHost("/project", vfstest.FromMap(map[string]string{
		"/project/shared.d.ts": "export declare const shared: number;",
	}, false), "", nil, nil), nil)
	sharedOpts := ast.SourceFileParseOptions{FileName: "/project/shared.d.ts", Path: "/project/shared.d.ts"}
	shared := base.GetSourceFile(sharedOpts)
	first := newNativeOverlayHost(base, map[string]string{
		"/Project/First.ts": "export const first = 1;",
		"/Project/Same.ts":  "export const same = 1;",
	})
	second := newNativeOverlayHost(first, map[string]string{"/project/same.ts": "export const same = 2;"})
	for path, text := range map[string]string{
		"/PROJECT/FIRST.ts": "export const first = 1;",
		"/PROJECT/SAME.ts":  "export const same = 2;",
	} {
		parsed := second.GetSourceFile(ast.SourceFileParseOptions{FileName: path, Path: tspath.Path(path)})
		if parsed == nil || parsed.Text() != text {
			t.Fatalf("stacked overlay lost source %s", path)
		}
		if got, ok := second.FS().ReadFile(filepath.FromSlash(path)); !ok || got != text || !second.FS().FileExists(filepath.FromSlash(path)) {
			t.Fatalf("stacked filesystem disagrees with parsed source %s", path)
		}
	}
	if second.GetSourceFile(sharedOpts) != shared {
		t.Fatal("stacked overlays bypassed the underlying shared declaration")
	}
}

func TestNativeOverlayHostUsesPhysicalAliases(t *testing.T) {
	dir := t.TempDir()
	physicalPath := filepath.Join(dir, "source.ts")
	if err := os.WriteFile(physicalPath, []byte("export const disk = 1;"), 0o644); err != nil {
		t.Fatal(err)
	}
	// TempDir can contain Windows short names. Physical aliases use the resolved
	// name; lookups do not resolve every possible spelling of the same file.
	physicalPath, err := fsutil.EvalSymlinks(physicalPath)
	if err != nil {
		t.Fatal(err)
	}
	aliasDir := filepath.Join(t.TempDir(), "project")
	if err := os.Symlink(dir, aliasDir); err != nil {
		t.Skipf("cannot create parent symlink: %v", err)
	}
	const text = "export const transformed = 2;"
	base := compiler.NewCompilerHost(filepath.ToSlash(dir), osvfs.FS(), "", nil, nil)
	aliasPath := filepath.Join(aliasDir, "source.ts")
	host := newNativeOverlayHost(base, normalizeOverlays(map[string]string{aliasPath: text}))
	for _, path := range []string{aliasPath, physicalPath} {
		parsed := host.GetSourceFile(declarationSourceFileParseOptions(path))
		if parsed == nil || parsed.Text() != text {
			t.Fatalf("source path %q did not receive its symlinked overlay", path)
		}
		if got, ok := host.FS().ReadFile(path); !ok || got != text {
			t.Fatalf("source read %q disagrees with its parsed overlay", path)
		}
	}
}

func TestTextOverlayFullFallbackPreservesSharedDeclarations(t *testing.T) {
	dir, original, _ := sparseNativeProgram(t, map[string]string{
		"a.ts":        "export const a = 1;\n",
		"b.ts":        "export const b = 1;\n",
		"shared.d.ts": "export declare const shared: number;\n",
	})
	cache := newSolutionCompileCache()
	program := compiler.NewProgram(compiler.ProgramOptions{
		Host:   cache.wrap(original.Host(), nil),
		Config: original.CommandLine(),
	})
	shared := program.GetSourceFile(filepath.Join(dir, "src", "shared.d.ts"))
	if shared == nil {
		t.Fatal("shared declaration missing from original Program")
	}
	extraPath := filepath.Join(dir, "src", "extra.ts")
	if err := os.WriteFile(extraPath, []byte("export const extra = 1;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if program.GetSourceFile(extraPath) != nil {
		t.Fatal("new module was already loaded before the overlay")
	}
	aPath := filepath.Join(dir, "src", "a.ts")
	bPath := filepath.Join(dir, "src", "b.ts")
	aText := "import { extra } from \"./extra\"; export const a = extra;\n"
	bText := "export const b = 2;\n"
	updated, reused, err := updateProgramWithTextOverlays(program, map[string]string{aPath: aText, bPath: bText})
	if err != nil {
		t.Fatal(err)
	}
	if reused {
		t.Fatal("new import did not force the full Program rebuild")
	}
	if updated.GetSourceFile(extraPath) == nil {
		t.Fatal("full fallback did not load the new imported module")
	}
	for path, text := range map[string]string{aPath: aText, bPath: bText} {
		if got := updated.GetSourceFile(path); got == nil || got.Text() != text || got == program.GetSourceFile(path) {
			t.Fatalf("full fallback lost transformed text or reused the changed source: %s", path)
		}
	}
	if updated.GetSourceFile(shared.FileName()) != shared {
		t.Fatal("full fallback reparsed the unchanged shared declaration")
	}
}
