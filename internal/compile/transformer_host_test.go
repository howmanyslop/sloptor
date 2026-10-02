package compile

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

func TestCallbackTransformerHostRetainsAndRevertsOverlays(t *testing.T) {
	dir := writeProject(t, "@scope/callback-overlay-fixture", "")
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const phase = \"disk\";\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	pluginsDir := filepath.Join(dir, "plugins")
	if err := os.MkdirAll(pluginsDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(pluginsDir, "fixture.js"), []byte("module.exports = () => () => file => file;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	config := `{"compilerOptions":{"allowSyntheticDefaultImports":true,"module":"CommonJS","moduleResolution":"Node","noLib":true,"moduleDetection":"force","strict":true,"target":"ESNext","types":[],"typeRoots":["node_modules/@rbxts"],"rootDir":"src","outDir":"out","plugins":[{"transform":"./plugins/fixture.js"}]},"include":["src"]}`
	if err := os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(config), 0o644); err != nil {
		t.Fatal(err)
	}
	mainPath := filepath.Join(dir, "src", "main.ts")

	var mu sync.Mutex
	var requests []TransformerRequest
	ctx := WithTransformerCallback(context.Background(), func(_ context.Context, request TransformerRequest) (TransformerResponse, error) {
		mu.Lock()
		requests = append(requests, request)
		mu.Unlock()
		return TransformerResponse{Diagnostics: []TransformerDiagnostic{}, Transformed: []TransformerOutputFile{}}, nil
	})

	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{
		Context:  ctx,
		Overlays: map[string]string{mainPath: "export const phase = \"memory\";\n"},
	}); err != nil {
		t.Fatalf("overlay build: %v (%v)", err, diags)
	}
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Context: ctx}); err != nil {
		t.Fatalf("disk build: %v (%v)", err, diags)
	}

	mu.Lock()
	defer mu.Unlock()
	if len(requests) != 2 {
		t.Fatalf("callback requests = %d, want 2", len(requests))
	}
	if len(requests[0].ChangedFiles) != 1 || !strings.Contains(requests[0].ChangedFiles[0].Text, "memory") {
		t.Fatalf("first changedFiles = %#v, want overlay", requests[0].ChangedFiles)
	}
	if len(requests[1].ChangedFiles) != 1 || !strings.Contains(requests[1].ChangedFiles[0].Text, "disk") {
		t.Fatalf("second changedFiles = %#v, want overlay revert", requests[1].ChangedFiles)
	}
}
