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

// A `transform` that names a subpath inside the package resolves to that file,
// not to the package entry.
func TestTransformerPluginFingerprintsResolveSubpathSpecifiers(t *testing.T) {
	dir := writeProject(t, "@scope/plugin-salt-subpath", "")
	writeFakeTransformerPackage(t, dir, "rotor-multi-transformer",
		map[string]any{"name": "rotor-multi-transformer", "version": "2.0.0", "main": "out/index.js"},
		"out/index.js", "module.exports = () => (sf) => sf; // entry\n")
	other := filepath.Join(dir, "node_modules", "rotor-multi-transformer", "out", "other.js")
	if err := os.WriteFile(other, []byte("module.exports = () => (sf) => sf; // other\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	writePluginTsconfig(t, dir, map[string]any{"transform": "rotor-multi-transformer/out/other"})

	fingerprints, err := transformerPluginFingerprints(dir, filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	if fingerprints[0].Unresolved != "" || fingerprints[0].Version != "2.0.0" {
		t.Fatalf("fingerprint = %+v, want the subpath resolved", fingerprints[0])
	}
	contents, err := os.ReadFile(other)
	if err != nil {
		t.Fatal(err)
	}
	if fingerprints[0].EntryHash != contentHash(string(contents)) {
		t.Fatalf("fingerprint hashed the package entry, not the subpath the tsconfig named")
	}
}

// A package with `exports` and no `main` still has to resolve, and a package
// whose `main` points at a directory has to follow Node's index lookup.
func TestTransformerPluginFingerprintsResolveAwkwardPackageShapes(t *testing.T) {
	tests := []struct {
		name     string
		pkg      string
		manifest map[string]any
		entry    string
	}{
		{
			name:     "exports only",
			pkg:      "rotor-exports-only",
			manifest: map[string]any{"name": "rotor-exports-only", "version": "3.1.0", "exports": map[string]any{".": map[string]any{"require": "./dist/main.cjs", "import": "./dist/main.mjs"}}},
			entry:    "dist/main.cjs",
		},
		{
			name:     "main names a directory",
			pkg:      "rotor-main-directory",
			manifest: map[string]any{"name": "rotor-main-directory", "version": "4.2.0", "main": "lib"},
			entry:    "lib/index.js",
		},
		{
			name:     "no main at all",
			pkg:      "rotor-implicit-index",
			manifest: map[string]any{"name": "rotor-implicit-index", "version": "5.0.0"},
			entry:    "index.js",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			dir := writeProject(t, "@scope/plugin-salt-"+test.pkg, "")
			writeFakeTransformerPackage(t, dir, test.pkg, test.manifest, test.entry, "module.exports = () => (sf) => sf;\n")
			writePluginTsconfig(t, dir, map[string]any{"transform": test.pkg})

			fingerprints, err := transformerPluginFingerprints(dir, filepath.Join(dir, "tsconfig.json"))
			if err != nil {
				t.Fatal(err)
			}
			if fingerprints[0].Unresolved != "" || fingerprints[0].EntryHash == "" {
				t.Fatalf("fingerprint = %+v, want %s resolved", fingerprints[0], test.entry)
			}
			if fingerprints[0].Version != test.manifest["version"] {
				t.Fatalf("fingerprint version = %q, want %q", fingerprints[0].Version, test.manifest["version"])
			}
		})
	}
}

// An unresolvable plugin has no version and no entry to hash, so its tsconfig
// entry is the only thing left that can tell two of them apart.
func TestUnresolvedTransformerPluginFingerprintsStillFollowTheTsconfig(t *testing.T) {
	dir := writeProject(t, "@scope/plugin-salt-unresolved-config", "")
	writePluginTsconfig(t, dir, map[string]any{"transform": "rotor-missing-transformer", "mode": "first"})
	before, err := transformerPluginFingerprints(dir, filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}

	writePluginTsconfig(t, dir, map[string]any{"transform": "rotor-missing-transformer", "mode": "second"})
	after, err := transformerPluginFingerprints(dir, filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}

	if before[0].Unresolved == "" || after[0].Unresolved == "" {
		t.Fatalf("expected both fingerprints unresolved, got %+v and %+v", before[0], after[0])
	}
	if string(before[0].Config) == string(after[0].Config) {
		t.Fatalf("an unresolved plugin's option change left the fingerprint identical: %s", before[0].Config)
	}
}
