package flamework

import (
	"crypto/sha256"
	"encoding/csv"
	"encoding/hex"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

func TestTask4OracleInventory_accountsForPinnedUpstreamSource(t *testing.T) {
	// Given: the pinned source tree and its independently recorded inventory.
	corpusRoot := filepath.Join("testdata", "task4")
	referenceRoot := filepath.Join("..", "..", "reference", "rbxts-transformer-flamework")
	inventoryFile, err := os.Open(filepath.Join(corpusRoot, "inventory.tsv"))
	if err != nil {
		t.Fatalf("open inventory: %v", err)
	}
	t.Cleanup(func() {
		if err := inventoryFile.Close(); err != nil {
			t.Errorf("close inventory: %v", err)
		}
	})

	reader := csv.NewReader(inventoryFile)
	reader.Comma = '\t'
	records, err := reader.ReadAll()
	if err != nil {
		t.Fatalf("read inventory: %v", err)
	}
	if len(records) == 0 || strings.Join(records[0], "\t") != "sha256\tpath\tfamily\trole" {
		t.Fatalf("inventory header = %v", records)
	}

	// When: every inventory entry is matched to the vendored source bytes.
	inventoried := make(map[string]bool, len(records)-1)
	families := make(map[string]bool)
	for _, record := range records[1:] {
		if len(record) != 4 {
			t.Fatalf("inventory record = %v, want four columns", record)
		}
		digest, sourcePath, family, role := record[0], record[1], record[2], record[3]
		if inventoried[sourcePath] {
			t.Fatalf("duplicate inventory path %q", sourcePath)
		}
		if family == "" || role == "" {
			t.Fatalf("inventory path %q has empty classification", sourcePath)
		}
		contents, err := os.ReadFile(filepath.Join(referenceRoot, filepath.FromSlash(sourcePath)))
		if err != nil {
			t.Fatalf("read source %q: %v", sourcePath, err)
		}
		actualDigest := sha256.Sum256(contents)
		if got := hex.EncodeToString(actualDigest[:]); got != digest {
			t.Fatalf("source %q sha256 = %s, want %s", sourcePath, got, digest)
		}
		inventoried[sourcePath] = true
		families[family] = true
	}

	var sourcePaths []string
	err = filepath.WalkDir(filepath.Join(referenceRoot, "src"), func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() {
			return nil
		}
		relative, err := filepath.Rel(referenceRoot, path)
		if err != nil {
			return err
		}
		sourcePaths = append(sourcePaths, filepath.ToSlash(relative))
		return nil
	})
	if err != nil {
		t.Fatalf("walk pinned source: %v", err)
	}
	sort.Strings(sourcePaths)

	// Then: all 67 source entries and every required behavior family are explicit.
	if len(sourcePaths) != 67 || len(inventoried) != len(sourcePaths) {
		t.Fatalf("source accounting = %d vendored, %d inventoried, want 67", len(sourcePaths), len(inventoried))
	}
	for _, sourcePath := range sourcePaths {
		if !inventoried[sourcePath] {
			t.Fatalf("vendored source %q is not inventoried", sourcePath)
		}
	}
	for _, family := range []string{
		"class-decorator-di", "component-attributes", "guards", "invalid-diagnostics",
		"macro-groups", "macro-paths", "networking", "package-metadata-options",
	} {
		if !families[family] {
			t.Fatalf("required behavior family %q is not inventoried", family)
		}
	}
}

func TestTask4OracleCorpus_containsDurableBehaviorArtifacts(t *testing.T) {
	// Given: the controlled upstream inputs and the committed expected surfaces.
	root := filepath.Join("testdata", "task4")
	required := []string{
		"oracle/deterministic-random.cjs",
		"oracle/flamework.build",
		"oracle/flamework.json",
		"oracle/package.json",
		"oracle/tsconfig.json",
		"oracle/src/server/class-decorator-di.server.ts",
		"oracle/src/shared/component-attributes.ts",
		"oracle/src/shared/guards.ts",
		"oracle/src/shared/macros.ts",
		"oracle/src/shared/networking.ts",
		"expected/diagnostics.jsonl",
		"expected/transformed.ts",
		"expected/artifacts/config.json",
		"expected/artifacts/flamework.build",
		"expected/artifacts/globs.json",
		"expected/luau/server/class-decorator-di.server.luau",
		"expected/luau/shared/component-attributes.luau",
		"expected/luau/shared/guards.luau",
		"expected/luau/shared/macros.luau",
		"expected/luau/shared/networking.luau",
	}

	// When: every artifact is read from disk.
	artifacts := make(map[string]string, len(required))
	for _, name := range required {
		contents, err := os.ReadFile(filepath.Join(root, filepath.FromSlash(name)))
		if err != nil {
			t.Fatalf("read corpus artifact %q: %v", name, err)
		}
		if len(contents) == 0 {
			t.Fatalf("corpus artifact %q is empty", name)
		}
		artifacts[name] = string(contents)
	}

	// Then: each requested observable family has a machine-consumed oracle marker.
	markers := map[string][]string{
		"expected/transformed.ts": {
			`Reflect["decorate"]`, `Flamework["resolveDependency"]`, `const dedup =`,
			`Networking.createEvent`, `"instanceGuard"`, `"uuid": "00000000-0000-4000-8000-000000000004"`,
		},
		"expected/diagnostics.jsonl": {
			`"exitCode":1`, `Path is invalid`, `template literal which is unsupported`,
		},
		"expected/artifacts/flamework.build": {
			`"flameworkVersion": "1.3.2"`, `"identifierPrefix": "task4"`, `"stringHashes"`,
		},
		"expected/luau/shared/macros.luau": {
			`Flamework._addPaths`, `payloadHash = "00000000-0000-4000-8000-000000000004"`,
		},
	}
	for name, expectedMarkers := range markers {
		for _, marker := range expectedMarkers {
			if !strings.Contains(artifacts[name], marker) {
				t.Fatalf("corpus artifact %q missing marker %q", name, marker)
			}
		}
	}
}
