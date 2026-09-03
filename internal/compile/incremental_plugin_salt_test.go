package compile

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// writeFakeTransformerPackage installs a transformer under the project's
// node_modules with the given package.json fields and entry text.
func writeFakeTransformerPackage(t *testing.T, projectDir, name string, manifest map[string]any, entryRelative, entryText string) {
	t.Helper()
	packageDir := filepath.Join(projectDir, "node_modules", filepath.FromSlash(name))
	entryPath := filepath.Join(packageDir, filepath.FromSlash(entryRelative))
	if err := os.MkdirAll(filepath.Dir(entryPath), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(entryPath, []byte(entryText), 0o644); err != nil {
		t.Fatal(err)
	}
	encoded, err := json.Marshal(manifest)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(packageDir, "package.json"), encoded, 0o644); err != nil {
		t.Fatal(err)
	}
}

func writePluginTsconfig(t *testing.T, projectDir string, entries ...map[string]any) {
	t.Helper()
	path := filepath.Join(projectDir, "tsconfig.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var config map[string]any
	if err := json.Unmarshal(data, &config); err != nil {
		t.Fatal(err)
	}
	options := config["compilerOptions"].(map[string]any)
	plugins := make([]any, 0, len(entries))
	for _, entry := range entries {
		plugins = append(plugins, entry)
	}
	options["plugins"] = plugins
	encoded, err := json.Marshal(config)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, encoded, 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestTransformerPluginFingerprintsPinVersionAndEntryContents(t *testing.T) {
	dir := writeProject(t, "@scope/plugin-salt", "")
	writeFakeTransformerPackage(t, dir, "rotor-fake-transformer",
		map[string]any{"name": "rotor-fake-transformer", "version": "1.2.3", "main": "dist/index.js"},
		"dist/index.js", "module.exports = () => (sf) => sf;\n")
	writePluginTsconfig(t, dir, map[string]any{"transform": "rotor-fake-transformer", "after": true})

	fingerprints, err := transformerPluginFingerprints(dir, filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	if len(fingerprints) != 1 {
		t.Fatalf("fingerprinted %d plugins, want 1", len(fingerprints))
	}
	first := fingerprints[0]
	if first.Unresolved != "" {
		t.Fatalf("plugin unresolved: %s", first.Unresolved)
	}
	if first.Version != "1.2.3" || first.EntryHash == "" {
		t.Fatalf("fingerprint = %+v, want the declared version and an entry hash", first)
	}
	if !strings.Contains(string(first.Config), `"after":true`) {
		t.Fatalf("fingerprint config = %s, want the whole tsconfig entry", first.Config)
	}

	// An edit to the plugin's own code is what a version number cannot see.
	entry := filepath.Join(dir, "node_modules", "rotor-fake-transformer", "dist", "index.js")
	if err := os.WriteFile(entry, []byte("module.exports = () => (sf) => sf; // changed\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	edited, err := transformerPluginFingerprints(dir, filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	if edited[0].EntryHash == first.EntryHash {
		t.Fatal("editing the plugin entry left the fingerprint unchanged")
	}
}

func TestTransformerPluginFingerprintsFollowPackageExports(t *testing.T) {
	dir := writeProject(t, "@scope/plugin-salt-exports", "")
	writeFakeTransformerPackage(t, dir, "@scope/exported-transformer",
		map[string]any{
			"name":    "@scope/exported-transformer",
			"version": "0.0.9",
			"exports": map[string]any{".": "./dist/index.cjs", "./package.json": "./package.json"},
		},
		"dist/index.cjs", "module.exports = () => (sf) => sf;\n")
	writePluginTsconfig(t, dir, map[string]any{"transform": "@scope/exported-transformer"})

	fingerprints, err := transformerPluginFingerprints(dir, filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	if fingerprints[0].Unresolved != "" || fingerprints[0].Version != "0.0.9" || fingerprints[0].EntryHash == "" {
		t.Fatalf("fingerprint = %+v, want the exports entry resolved", fingerprints[0])
	}
}

// A plugin that is not installed still has to be described, or two projects
// with different broken plugin lists would share a salt.
func TestTransformerPluginFingerprintsRecordUnresolvedPackages(t *testing.T) {
	dir := writeProject(t, "@scope/plugin-salt-missing", "")
	writePluginTsconfig(t, dir, map[string]any{"transform": "rotor-transformer-that-is-not-installed"})

	fingerprints, err := transformerPluginFingerprints(dir, filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	if fingerprints[0].Unresolved != "package not installed" {
		t.Fatalf("fingerprint = %+v, want it marked unresolved", fingerprints[0])
	}
}

// The salt is what an incremental build compares before it trusts a previous
// manifest, so the plugin identity has to reach it.
func TestIncrementalSaltMovesWithTheTransformerPluginEntry(t *testing.T) {
	dir := writeProject(t, "@scope/plugin-salt-build", "")
	writeFakeTransformerPackage(t, dir, "rotor-fake-transformer",
		map[string]any{"name": "rotor-fake-transformer", "version": "1.0.0", "main": "index.js"},
		"index.js", "module.exports = () => (sf) => sf;\n")
	writePluginTsconfig(t, dir, map[string]any{"transform": "rotor-fake-transformer"})

	projectDir, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (diags: %v)", err, diags)
	}
	before, err := incrementalSaltWithFlamework(projectDir, program, ProjectOptions{}, "", nil)
	if err != nil {
		t.Fatal(err)
	}

	entry := filepath.Join(dir, "node_modules", "rotor-fake-transformer", "index.js")
	if err := os.WriteFile(entry, []byte("module.exports = () => (sf) => sf; // v2\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	after, err := incrementalSaltWithFlamework(projectDir, program, ProjectOptions{}, "", nil)
	if err != nil {
		t.Fatal(err)
	}
	if before == after {
		t.Fatal("the incremental salt survived a transformer plugin change, so the previous outputs would be reused")
	}
}
