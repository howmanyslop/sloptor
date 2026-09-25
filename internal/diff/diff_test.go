package diff

import (
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

func TestDifferential(t *testing.T) {
	root := repoRoot(t)
	projDir := filepath.Join(root, "testdata", "diff", "project")
	goldenDir := filepath.Join(root, "testdata", "diff", "golden")
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

	goldens, err := filepath.Glob(filepath.Join(goldenDir, "*.luau"))
	if err != nil || len(goldens) == 0 {
		t.Fatalf("no goldens found (%v) — run tools/oracle/oracle.ps1", err)
	}

	enabled := map[string]bool{}
	for _, name := range EnabledFixtures {
		enabled[name] = true
	}

	// ONE project-wide compile (multi-file fixtures import each other, so a
	// per-file CompileFile can no longer stand alone); each enabled manifest
	// entry then diffs its out-file against the rbxtsc golden.
	out, diags, err := compile.CompileProject(projDir)
	if err != nil {
		t.Fatalf("CompileProject error: %v (diagnostics: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("CompileProject diagnostics: %v", diags)
	}

	skipped := []string{}
	for _, goldenPath := range goldens {
		name := strings.TrimSuffix(filepath.Base(goldenPath), ".luau")
		if !enabled[name] {
			skipped = append(skipped, name)
			continue
		}
		t.Run(name, func(t *testing.T) {
			want, err := os.ReadFile(goldenPath)
			if err != nil {
				t.Fatal(err)
			}
			got, ok := out["out/"+name+".luau"]
			if !ok {
				t.Fatalf("out/%s.luau missing from CompileProject output", name)
			}
			if behavioralTest, corrected := corrections["differential/"+name]; corrected {
				if !verifiedCorrections[behavioralTest] {
					if err := forkparity.VerifyBehavioralTest(t.Context(), root, behavioralTest); err != nil {
						t.Fatal(err)
					}
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
		t.Logf("skipped (not yet enabled): %s", strings.Join(skipped, ", "))
	}
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
