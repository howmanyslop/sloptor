package main

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestApplyOverlay(t *testing.T) {
	t.Chdir(filepath.Join("..", ".."))

	dst := t.TempDir()
	if err := applyOverlay(dst); err != nil {
		t.Fatalf("applyOverlay: %v", err)
	}

	want, err := os.ReadFile(filepath.Join(overlayDir, "checker", "rotor_exports.go.tmpl"))
	if err != nil {
		t.Fatalf("read overlay source: %v", err)
	}
	got, err := os.ReadFile(filepath.Join(dst, "checker", "rotor_exports.go"))
	if err != nil {
		t.Fatalf("shim not applied: %v", err)
	}
	if string(got) != string(want) {
		t.Errorf("applied shim differs from overlay source\ngot:\n%s\nwant:\n%s", got, want)
	}
	checkedIn, err := os.ReadFile(filepath.Join(outDir, "checker", "rotor_exports.go"))
	if err != nil {
		t.Fatalf("read checked-in shim: %v", err)
	}
	if string(checkedIn) != string(want) {
		t.Errorf("tsgo/checker/rotor_exports.go is out of sync with %s/checker/rotor_exports.go.tmpl; edit the overlay and re-copy", overlayDir)
	}
}

func TestMirrorMergesHarmlessUpstreamContextDrift(t *testing.T) {
	// Catches a source-only upstream comment change making regeneration fail.
	// The expected Rotor edit comes from the checked-in patch series in the
	// issue requirement: it must survive without replacing the changed comment.
	t.Chdir(filepath.Join("..", ".."))
	destination := mirrorDestination(t)
	source := mirrorSource(t, func(path string, data []byte) []byte {
		if filepath.ToSlash(path) != "internal/compiler/program.go" {
			return data
		}
		return []byte(strings.Replace(string(data),
			"// createCheckerPool, if non-nil, overrides", "// Upstream clarified that createCheckerPool, if non-nil, overrides", 1))
	})

	if _, _, err := mirror(source, "HEAD", destination); err != nil {
		t.Fatalf("mirror context drift: %v", err)
	}
	got, err := os.ReadFile(filepath.Join(destination, "compiler", "program.go"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(got), "Upstream clarified that createCheckerPool") {
		t.Error("context-only upstream change was lost")
	}
	if !strings.Contains(string(got), "func (p *Program) UpdateProgramFiles") {
		t.Error("Rotor program patch was not applied")
	}
	parser, err := os.ReadFile(filepath.Join(destination, "parser", "references.go"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(parser), "func RefreshExternalModuleReferences") {
		t.Error("module-reference refresh API was not restored")
	}
	printer, err := os.ReadFile(filepath.Join(destination, "printer", "emitcontext.go"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(printer), "e.leadingComments = slices.Clone(source.leadingComments)") ||
		!strings.Contains(string(printer), "e.trailingComments = slices.Clone(source.trailingComments)") {
		t.Error("emit context no longer retains copied comments")
	}
}

func TestMirrorPreservesDestinationAndConflictWorkspace(t *testing.T) {
	// Catches a conflicting upstream edit destroying the existing mirror.
	// The unchanged sentinel is the requirement-derived expected result until a
	// person resolves the preserved Git conflict workspace.
	t.Chdir(filepath.Join("..", ".."))
	destination := mirrorDestination(t)
	sentinel := filepath.Join(destination, "keep-until-candidate-succeeds")
	if err := os.WriteFile(sentinel, []byte("current mirror"), 0o644); err != nil {
		t.Fatal(err)
	}
	source := mirrorSource(t, func(path string, data []byte) []byte {
		if filepath.ToSlash(path) != "internal/compiler/program.go" {
			return data
		}
		return []byte(strings.Replace(string(data), "oldFile := p.filesByPath[changedFilePath]", "oldFile := changedProgramFile", 1))
	})

	_, _, err := mirror(source, "HEAD", destination)
	if err == nil {
		t.Fatal("mirror unexpectedly accepted an overlapping source edit")
	}
	var candidate *candidateError
	if !errors.As(err, &candidate) {
		t.Fatalf("mirror error did not preserve a candidate: %v", err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(candidate.dir) })
	if got, readErr := os.ReadFile(sentinel); readErr != nil || string(got) != "current mirror" {
		t.Fatalf("destination changed after conflict: %q, %v", got, readErr)
	}
	conflict, readErr := os.ReadFile(filepath.Join(candidate.dir, "tsgo", "compiler", "program.go"))
	if readErr != nil {
		t.Fatalf("read preserved conflict: %v", readErr)
	}
	if !strings.Contains(string(conflict), "<<<<<<<") {
		t.Error("candidate does not retain conflict markers")
	}
}

func TestReplaceOutputRestoresOriginalWhenCandidateMoveFails(t *testing.T) {
	// Catches a failed final move leaving the established mirror unavailable.
	// The expected old content is authored here because the recovery contract is
	// that a failed replacement restores exactly the tree that preceded it.
	root := t.TempDir()
	destination := filepath.Join(root, outDir)
	if err := os.MkdirAll(destination, 0o755); err != nil {
		t.Fatal(err)
	}
	oldFile := filepath.Join(destination, "old.go")
	if err := os.WriteFile(oldFile, []byte("old mirror"), 0o644); err != nil {
		t.Fatal(err)
	}
	candidate := filepath.Join(root, "missing-candidate")

	err := replaceOutput(candidate, destination)
	if err == nil {
		t.Fatal("replace unexpectedly accepted a missing candidate")
	}
	var replacement *replacementError
	if !errors.As(err, &replacement) {
		t.Fatalf("replace error did not report recovery paths: %v", err)
	}
	if replacement.original != destination {
		t.Fatalf("original recovery path = %q, want %q", replacement.original, destination)
	}
	got, readErr := os.ReadFile(oldFile)
	if readErr != nil || string(got) != "old mirror" {
		t.Fatalf("original mirror was not restored: %q, %v", got, readErr)
	}
}

func mirrorDestination(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	destination := filepath.Join(root, outDir)
	patches, err := patchNames()
	if err != nil {
		t.Fatal(err)
	}
	targets, err := patchTargets(patches)
	if err != nil {
		t.Fatal(err)
	}
	for _, target := range targets {
		data, err := os.ReadFile(target)
		if err != nil {
			t.Fatal(err)
		}
		path := filepath.Join(root, target)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	return destination
}

func mirrorSource(t *testing.T, change func(path string, data []byte) []byte) string {
	t.Helper()
	patches, err := patchNames()
	if err != nil {
		t.Fatal(err)
	}
	targets, err := patchTargets(patches)
	if err != nil {
		t.Fatal(err)
	}
	bases := t.TempDir()
	for _, target := range targets {
		data, err := os.ReadFile(target)
		if err != nil {
			t.Fatal(err)
		}
		path := filepath.Join(bases, target)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	resolvedBases, err := filepath.EvalSymlinks(bases)
	if err != nil {
		t.Fatal(err)
	}
	for i := len(patches) - 1; i >= 0; i-- {
		if err := gitApply("", "--reverse", "--unsafe-paths", "--directory", resolvedBases, patches[i]); err != nil {
			t.Fatalf("recover %s: %v", patches[i], err)
		}
	}

	source := t.TempDir()
	for _, target := range targets {
		data, err := os.ReadFile(filepath.Join(bases, target))
		if err != nil {
			t.Fatal(err)
		}
		rel := strings.TrimPrefix(filepath.ToSlash(target), "tsgo/")
		path := filepath.Join(source, "internal", rel)
		data = change(filepath.ToSlash(filepath.Join("internal", rel)), data)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(source, "LICENSE"), []byte("test license\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := run(source, "git", "init", "-q"); err != nil {
		t.Fatal(err)
	}
	if err := run(source, "git", "add", "."); err != nil {
		t.Fatal(err)
	}
	if err := run(source, "git", "-c", "user.name=mirror-test", "-c", "user.email=mirror-test@example.invalid", "commit", "-qm", "mirror source"); err != nil {
		t.Fatal(err)
	}
	return source
}
