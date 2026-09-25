package conformance

import (
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

var pinnedDiagnosticMessages = map[string]string{
	"expectedFunctionGotMethod": "Attempted to assign method where non-method was expected.",
	"expectedMethodGotFunction": "Attempted to assign non-method where method was expected.",
	"noUnstableThisType":        "The generic this type can become void, changing whether a receiver is passed.",
}

func TestPinnedCompatibilityDiagnostics(t *testing.T) {
	environment := compatibilityEnvironment{root: repoRoot(t), upstreamCLI: requirePinnedCompiler(t)}
	dir := filepath.Join(environment.root, "testdata", "compatibility", "diagnostics")
	fixtures, err := loadDiagnosticFixtures(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(fixtures) == 0 {
		t.Fatal("no pinned compatibility diagnostic fixtures")
	}
	for _, fixture := range fixtures {
		t.Run(fixture.Name, func(t *testing.T) {
			if len(fixture.ExpectedIDs) != 1 {
				t.Fatalf("fixture expected IDs = %v, want one", fixture.ExpectedIDs)
			}
			expectedID := fixture.ExpectedIDs[0]
			got, err := compileDiagnosticFixture(environment.root, fixture.Path)
			if err != nil {
				t.Fatal(err)
			}
			if !slices.Equal(got, []string{expectedID}) {
				t.Fatalf("Rotor diagnostics = %v, want [%s]", got, expectedID)
			}
			message, ok := pinnedDiagnosticMessages[expectedID]
			if !ok {
				t.Fatalf("no pinned message for diagnostic %s", expectedID)
			}
			environment.assertPinnedDiagnostic(t, fixture.Path, message)
		})
	}
}

func (environment compatibilityEnvironment) assertPinnedDiagnostic(t *testing.T, fixturePath, message string) {
	t.Helper()
	baseProjectDir := filepath.Join(environment.root, "testdata", "conformance", "project")
	projectDir := t.TempDir()
	if err := copyFile(filepath.Join(baseProjectDir, "package.json"), filepath.Join(projectDir, "package.json")); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projectDir, "tsconfig.json"), []byte(compatibilityRuntimeTSConfig(false)), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projectDir, "default.project.json"), []byte(fixtureRojoConfig()), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := stageRuntimeTypeRoots(baseProjectDir, projectDir); err != nil {
		t.Fatal(err)
	}
	pinnedNodeModules := filepath.Dir(filepath.Dir(filepath.Dir(filepath.Dir(environment.upstreamCLI))))
	if err := copyTree(
		filepath.Join(pinnedNodeModules, "@rbxts", "types"),
		filepath.Join(projectDir, "node_modules", "@rbxts", "types"),
	); err != nil {
		t.Fatal(err)
	}
	if err := copyFile(fixturePath, filepath.Join(projectDir, "src", filepath.Base(fixturePath))); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("node", environment.upstreamCLI, "build", "--project", projectDir, "--type", "game")
	output, err := cmd.CombinedOutput()
	if err == nil {
		t.Fatalf("pinned upstream compiled diagnostic fixture successfully\n%s", output)
	}
	if !strings.Contains(string(output), message) {
		t.Fatalf("pinned upstream output missing %q:\n%s", message, output)
	}
}
