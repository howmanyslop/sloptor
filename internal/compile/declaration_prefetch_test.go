package compile

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

const prefetchFixtureSource = "export const value: number = 1;\nexport function add(left: number, right: number) {\n\treturn left + right;\n}\n"

// writeDeclarationPrefetchProject writes a declaration-emitting project, with
// a no-op transformer plugin when withPlugin is set.
func writeDeclarationPrefetchProject(t *testing.T, withPlugin bool) string {
	t.Helper()
	dir := writeProject(t, "@scope/declaration-prefetch-fixture", "")
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte(prefetchFixtureSource), 0o644); err != nil {
		t.Fatal(err)
	}
	plugins := ""
	if withPlugin {
		pluginsDir := filepath.Join(dir, "plugins")
		if err := os.MkdirAll(pluginsDir, 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(pluginsDir, "fixture.js"), []byte("module.exports = () => () => file => file;\n"), 0o644); err != nil {
			t.Fatal(err)
		}
		plugins = `,"plugins":[{"transform":"./plugins/fixture.js"}]`
	}
	config := `{"compilerOptions":{"allowSyntheticDefaultImports":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out","declaration":true` + plugins + `},"include":["src"]}`
	if err := os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(config), 0o644); err != nil {
		t.Fatal(err)
	}
	return dir
}

type stubTransformerHost struct {
	transformerHost
	calls int
}

func (host *stubTransformerHost) transform(context.Context, transformerCall) (*sidecarResponse, sidecarCallStats, error) {
	host.calls++
	return &sidecarResponse{}, sidecarCallStats{}, nil
}

func TestPrefetchingTransformerHostFinishesEmitBeforeReturning(t *testing.T) {
	dir := writeDeclarationPrefetchProject(t, false)
	_, program, diags, err := newProjectProgramWithOptions(dir, "", ProjectOptions{})
	if err != nil {
		t.Fatalf("program: %v (%v)", err, diags)
	}
	prefetch := newDeclarationPrefetch(program, nil, nil)
	if prefetch == nil {
		t.Fatal("newDeclarationPrefetch returned nil for a declaration-emitting project")
	}
	if _, ran, _ := prefetch.result(); ran {
		t.Fatal("prefetch reported a result before any round trip")
	}

	inner := &stubTransformerHost{}
	host := prefetchingTransformerHost{transformerHost: inner, prefetch: prefetch}
	for range 2 {
		if _, _, err := host.transform(context.Background(), transformerCall{}); err != nil {
			t.Fatalf("transform: %v", err)
		}
	}
	if inner.calls != 2 {
		t.Fatalf("inner transform calls = %d, want 2", inner.calls)
	}

	emitted, ran, err := prefetch.result()
	if err != nil {
		t.Fatalf("prefetch: %v", err)
	}
	if !ran {
		t.Fatal("prefetch did not run during the round trip")
	}
	if len(emitted) != 1 || filepath.Base(emitted[0].FileName) != "main.d.ts" {
		t.Fatalf("prefetched files = %#v, want main.d.ts only", emitted)
	}
}

func TestAbandonedDeclarationPrefetchLeavesNoResult(t *testing.T) {
	dir := writeDeclarationPrefetchProject(t, false)
	_, program, diags, err := newProjectProgramWithOptions(dir, "", ProjectOptions{})
	if err != nil {
		t.Fatalf("program: %v (%v)", err, diags)
	}
	prefetch := newDeclarationPrefetch(program, nil, nil)
	if prefetch == nil {
		t.Fatal("newDeclarationPrefetch returned nil for a declaration-emitting project")
	}
	abandon := make(chan struct{})
	close(abandon)
	prefetch.run(abandon)
	if emitted, ran, err := prefetch.result(); ran || err != nil || emitted != nil {
		t.Fatalf("abandoned prefetch result = (%#v, %v, %v), want none", emitted, ran, err)
	}
}

func TestNewDeclarationPrefetchSkipsProjectsWithoutDeclarations(t *testing.T) {
	dir := writeProject(t, "@scope/declaration-prefetch-none", "")
	_, program, diags, err := newProjectProgramWithOptions(dir, "", ProjectOptions{})
	if err != nil {
		t.Fatalf("program: %v (%v)", err, diags)
	}
	if prefetch := newDeclarationPrefetch(program, nil, nil); prefetch != nil {
		t.Fatal("newDeclarationPrefetch returned a prefetch for a project without declaration emit")
	}
}

// The plugin build emits its declarations during the transformer round trip;
// the plain build emits them after the Luau writes. Both must write the same
// text.
func TestDeclarationPrefetchMatchesPostTransformEmit(t *testing.T) {
	plain := writeDeclarationPrefetchProject(t, false)
	if _, diags, err := BuildProjectWithOptions(plain, ProjectOptions{}); err != nil {
		t.Fatalf("plain build: %v (%v)", err, diags)
	}

	prefetched := writeDeclarationPrefetchProject(t, true)
	calls := 0
	ctx := WithTransformerCallback(context.Background(), func(context.Context, TransformerRequest) (TransformerResponse, error) {
		calls++
		return TransformerResponse{Diagnostics: []TransformerDiagnostic{}, Transformed: []TransformerOutputFile{}}, nil
	})
	if _, diags, err := BuildProjectWithOptions(prefetched, ProjectOptions{Context: ctx}); err != nil {
		t.Fatalf("plugin build: %v (%v)", err, diags)
	}
	if calls != 1 {
		t.Fatalf("transformer callback calls = %d, want 1", calls)
	}

	want, err := os.ReadFile(filepath.Join(plain, "out", "main.d.ts"))
	if err != nil {
		t.Fatal(err)
	}
	got, err := os.ReadFile(filepath.Join(prefetched, "out", "main.d.ts"))
	if err != nil {
		t.Fatal(err)
	}
	if string(got) != string(want) {
		t.Fatalf("prefetched main.d.ts =\n%s\nwant\n%s", got, want)
	}
}
