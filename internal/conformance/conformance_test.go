package conformance

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/compile"
	"rotor/internal/forkparity"
)

func repoRoot(t *testing.T) string {
	t.Helper()
	dir, err := filepath.Abs(filepath.Join("..", ".."))
	if err != nil {
		t.Fatal(err)
	}
	return dir
}

// goldenPaths walks testdata/conformance/golden and returns every golden's
// path relative to the golden root, slash-separated (the manifest key form).
func goldenPaths(t *testing.T, goldenDir string) []string {
	t.Helper()
	var rels []string
	err := filepath.WalkDir(goldenDir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(path, ".luau") {
			return nil
		}
		rel, err := filepath.Rel(goldenDir, path)
		if err != nil {
			return err
		}
		rels = append(rels, filepath.ToSlash(rel))
		return nil
	})
	if err != nil {
		t.Fatalf("walking goldens: %v", err)
	}
	return rels
}

func TestConformance(t *testing.T) {
	root := repoRoot(t)
	projDir := filepath.Join(root, "testdata", "conformance", "project")
	goldenDir := filepath.Join(root, "testdata", "conformance", "golden")
	ledger, err := forkparity.ReadDivergenceLedger(filepath.Join(root, "testdata", "forkparity", "divergence-ledger.json"))
	if err != nil {
		t.Fatal(err)
	}
	corrections := map[string]string{}
	for _, row := range ledger.Rows {
		if row.Classification == forkparity.DivergenceUpstreamCorrected {
			corrections[row.ID] = row.BehavioralTest
		}
	}
	verifiedCorrections := map[string]bool{}
	verifiedRuntime := false

	goldens := goldenPaths(t, goldenDir)
	if len(goldens) == 0 {
		t.Fatal("no goldens found — run tools/oracle/conformance-oracle.ps1")
	}

	enabled := map[string]bool{}
	for _, name := range EnabledFixtures {
		enabled[name] = true
	}
	for name := range enabled {
		if _, err := os.Stat(filepath.Join(goldenDir, filepath.FromSlash(name))); err != nil {
			t.Errorf("manifest entry %q has no golden: %v", name, err)
		}
	}
	for name, reason := range DisabledFixtures {
		if reason == "" {
			t.Errorf("disabled fixture %q is missing a reason", name)
		}
		if _, err := os.Stat(filepath.Join(goldenDir, filepath.FromSlash(name))); err != nil {
			t.Errorf("disabled manifest entry %q has no golden: %v", name, err)
		}
	}

	// Everything-disabled fast path: do NOT compile the project, so this test
	// stays green regardless of transformer state until Phase 5 enables
	// fixtures.
	if len(enabled) == 0 {
		t.Logf("conformance corpus disabled: all %d goldens skipped (manifest empty)", len(goldens))
		t.Skipf("no fixtures enabled in internal/conformance/manifest.go")
	}

	skipped := []string{}
	for _, rel := range goldens {
		if !enabled[rel] {
			if reason, ok := DisabledFixtures[rel]; ok {
				skipped = append(skipped, fmt.Sprintf("%s (%s)", rel, reason))
				continue
			}
			t.Fatalf("golden %q is neither enabled nor disabled", rel)
			continue
		}
		t.Run(rel, func(t *testing.T) {
			want, err := os.ReadFile(filepath.Join(goldenDir, filepath.FromSlash(rel)))
			if err != nil {
				t.Fatal(err)
			}
			got, err := compileConformanceFixture(root, projDir, rel)
			if err != nil {
				t.Fatal(err)
			}
			if behavioralTest, corrected := corrections["conformance/"+rel]; corrected {
				if !verifiedRuntime {
					if err := runBehavioralSuite(root, requireCompatibilityRuntimeTools(t)); err != nil {
						t.Fatal(err)
					}
					verifiedRuntime = true
				}
				if !verifiedCorrections[behavioralTest] {
					runPinnedCompatibilityFixture(t, strings.TrimPrefix(behavioralTest, "TestPinnedCompatibility/"))
					verifiedCorrections[behavioralTest] = true
				}
				if got != string(want) {
					t.Logf("upstream-corrected output differs from the retained frozen golden; runtime verified by %s", behavioralTest)
				}
				return
			}
			if got != string(want) {
				t.Errorf("output differs from rbxtsc golden")
				reportFirstDiff(t, got, string(want))
			}
		})
	}
	if len(skipped) > 0 {
		t.Logf("skipped %d goldens (explicitly disabled): %s", len(skipped), strings.Join(skipped, ", "))
	}
}

func compileConformanceFixture(root, baseProjectDir, goldenRel string) (string, error) {
	if err := ensureConformanceProjectDeps(baseProjectDir); err != nil {
		return "", err
	}

	tmpProj, err := os.MkdirTemp(baseProjectDir, ".phase5-fixture-")
	if err != nil {
		return "", err
	}
	defer os.RemoveAll(tmpProj)

	if err := os.WriteFile(filepath.Join(tmpProj, "package.json"), []byte(`{"name":"conformance-fixture"}`), 0o644); err != nil {
		return "", err
	}
	if err := os.WriteFile(filepath.Join(tmpProj, "tsconfig.json"), []byte(fixtureTSConfig()), 0o644); err != nil {
		return "", err
	}
	if err := os.WriteFile(filepath.Join(tmpProj, "default.project.json"), []byte(fixtureRojoConfig()), 0o644); err != nil {
		return "", err
	}
	if err := os.MkdirAll(filepath.Join(tmpProj, "src"), 0o755); err != nil {
		return "", err
	}
	if err := os.MkdirAll(filepath.Join(tmpProj, "include"), 0o755); err != nil {
		return "", err
	}
	if err := ensureFixtureTypeRoots(baseProjectDir, tmpProj); err != nil {
		return "", err
	}
	if err := copyTree(filepath.Join(baseProjectDir, "src", "helpers"), filepath.Join(tmpProj, "src", "helpers")); err != nil {
		return "", err
	}
	if err := copyFile(filepath.Join(baseProjectDir, "src", "services.d.ts"), filepath.Join(tmpProj, "src", "services.d.ts")); err != nil {
		return "", err
	}
	if err := os.WriteFile(filepath.Join(tmpProj, "src", "testez-globals.d.ts"), []byte("/// <reference types=\"@rbxts/testez/globals\" />\n"), 0o644); err != nil {
		return "", err
	}

	sourceRel, err := sourceRelFromGolden(baseProjectDir, goldenRel)
	if err != nil {
		return "", err
	}
	if err := copyFile(filepath.Join(baseProjectDir, "src", filepath.FromSlash(sourceRel)), filepath.Join(tmpProj, "src", filepath.FromSlash(sourceRel))); err != nil {
		return "", err
	}

	text, diags, err := compile.CompileFileWithOptions(tmpProj, filepath.ToSlash(filepath.Join("src", sourceRel)), compile.ProjectOptions{
		AllowCommentDirectives: true,
	})
	if err != nil {
		return "", fmt.Errorf("CompileFile(%s): %w (diagnostics: %v)", goldenRel, err, diags)
	}
	if len(diags) > 0 {
		return "", fmt.Errorf("CompileFile(%s) diagnostics: %v", goldenRel, diags)
	}
	return text, nil
}

func sourceRelFromGolden(baseProjectDir, goldenRel string) (string, error) {
	rel := strings.TrimSuffix(goldenRel, ".luau")
	for _, ext := range []string{".ts", ".tsx"} {
		candidate := rel + ext
		if _, err := os.Stat(filepath.Join(baseProjectDir, "src", filepath.FromSlash(candidate))); err == nil {
			return candidate, nil
		}
	}
	return "", fmt.Errorf("no source file for golden %s", goldenRel)
}

func fixtureTSConfig() string {
	return `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"downlevelIteration": true,
		"jsx": "react",
		"jsxFactory": "Roact.jsx",
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"resolveJsonModule": true,
		"forceConsistentCasingInFileNames": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": ["node_modules/@rbxts"],
		"experimentalDecorators": true,
		"rootDir": "src",
		"outDir": "out",
		"baseUrl": "src"
	},
	"include": ["src"]
}`
}

func fixtureRojoConfig() string {
	return `{"name":"conformance","tree":{"$path":"out","include":{"$path":"include"},"node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}`
}

func copyTree(src, dst string) error {
	return filepath.WalkDir(src, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		target := filepath.Join(dst, rel)
		if d.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		return copyFile(path, target)
	})
}

func copyFile(src, dst string) error {
	data, err := os.ReadFile(src)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	return os.WriteFile(dst, data, 0o644)
}

func ensureFixtureTypeRoots(baseProjectDir, tmpProj string) error {
	src := filepath.Join(baseProjectDir, "node_modules", "@rbxts")
	dst := filepath.Join(tmpProj, "node_modules", "@rbxts")
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	if err := os.Symlink(src, dst); err == nil {
		return nil
	}
	return copyTree(src, dst)
}

func reportFirstDiff(t *testing.T, got, want string) {
	t.Helper()
	gl, wl := strings.Split(got, "\n"), strings.Split(want, "\n")
	for i := 0; i < len(gl) && i < len(wl); i++ {
		if gl[i] != wl[i] {
			t.Errorf("first diff at line %d:\n  got:  %q\n  want: %q", i+1, gl[i], wl[i])
			return
		}
	}
	t.Errorf("length mismatch: got %d lines, want %d lines\n--- got tail ---\n%s\n--- want tail ---\n%s",
		len(gl), len(wl), tail(gl), tail(wl))
}

func tail(lines []string) string {
	n := len(lines)
	if n > 5 {
		lines = lines[n-5:]
	}
	return strings.Join(lines, "\n")
}
