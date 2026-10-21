package compile

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func jecsTestPlugin(t *testing.T, raw string) transformerPluginConfig {
	t.Helper()
	var fields struct {
		Transform string `json:"transform"`
	}
	if err := json.Unmarshal([]byte(raw), &fields); err != nil {
		t.Fatal(err)
	}
	return transformerPluginConfig{Transform: fields.Transform, raw: json.RawMessage(raw)}
}

// installFakeJecs puts a transformer package under dir/node_modules and
// registers its hash as a ported build for the duration of the test.
func installFakeJecs(t *testing.T, dir string) string {
	t.Helper()
	sourceDir := filepath.Join(dir, "node_modules", nativeJecsTransform, "out", "src")
	if err := os.MkdirAll(sourceDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(sourceDir, "index.js"), []byte("export default () => () => (file) => file;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	hash, err := hashJecsTransformerSources(sourceDir)
	if err != nil {
		t.Fatal(err)
	}
	nativeJecsSourceHashes[hash] = true
	t.Cleanup(func() { delete(nativeJecsSourceHashes, hash) })
	return sourceDir
}

// Catches jecs being run natively ahead of another transformer that the
// sidecar would have run before it, which changes the input both of them see.
// The sidecar runs every `before` transformer in list order and then every
// `after` one, so jecs is first exactly when no `before` entry precedes it.
func TestNativeJecsStageKeepsTransformerOrder(t *testing.T) {
	dir := t.TempDir()
	installFakeJecs(t, dir)
	jecsEntry := `{"transform":"rbxts-transformer-jecs","silent":true}`
	afterEntry := `{"transform":"applier","after":true}`
	beforeEntry := `{"transform":"other"}`

	cases := []struct {
		name      string
		plugins   []string
		native    bool
		remaining []string
	}{
		{"alone", []string{jecsEntry}, true, nil},
		{"before an after transformer", []string{jecsEntry, afterEntry}, true, []string{"applier"}},
		{"listed after an after transformer", []string{afterEntry, jecsEntry}, true, []string{"applier"}},
		{"behind another before transformer", []string{beforeEntry, jecsEntry}, false, nil},
		{"with an option the port does not know", []string{`{"transform":"rbxts-transformer-jecs","resolutionHost":{}}`}, false, nil},
	}
	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			var plugins []transformerPluginConfig
			for _, raw := range testCase.plugins {
				plugins = append(plugins, jecsTestPlugin(t, raw))
			}
			config, rest := nativeJecsStage(dir, plugins)
			if (config != nil) != testCase.native {
				t.Fatalf("native = %v, want %v", config != nil, testCase.native)
			}
			if !testCase.native {
				if len(rest) != len(plugins) {
					t.Fatalf("declined stage dropped plugins: %d of %d left", len(rest), len(plugins))
				}
				return
			}
			var names []string
			for _, plugin := range rest {
				names = append(names, plugin.Transform)
			}
			if len(names) != len(testCase.remaining) || (len(names) > 0 && names[0] != testCase.remaining[0]) {
				t.Fatalf("remaining = %v, want %v", names, testCase.remaining)
			}
		})
	}
}

// Catches a re-patched or upgraded transformer running through a port written
// for a different build of it.
func TestNativeJecsStageDeclinesAnUnportedBuild(t *testing.T) {
	dir := t.TempDir()
	sourceDir := installFakeJecs(t, dir)
	if err := os.WriteFile(filepath.Join(sourceDir, "util.js"), []byte("export const changed = true;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	jecsSourceHashCache.Clear()
	t.Cleanup(jecsSourceHashCache.Clear)
	if config, _ := nativeJecsStage(dir, []transformerPluginConfig{jecsTestPlugin(t, `{"transform":"rbxts-transformer-jecs"}`)}); config != nil {
		t.Fatal("ran natively against a transformer build the port does not match")
	}
}
