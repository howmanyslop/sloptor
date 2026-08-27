package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// TestApplyOverlay proves regeneration re-emits the rotor shims without
// running a full re-mirror: applyOverlay into an empty directory must land
// each overlay file at its relative path minus the .tmpl suffix, byte-equal
// to the overlay source.
func TestApplyOverlay(t *testing.T) {
	// applyOverlay resolves overlayDir relative to the repo root (where
	// `go run ./tools/mirror` is invoked); tests run in tools/mirror.
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

	// The checked-in tsgo copy must stay byte-identical to the overlay.
	checkedIn, err := os.ReadFile(filepath.Join(outDir, "checker", "rotor_exports.go"))
	if err != nil {
		t.Fatalf("read checked-in shim: %v", err)
	}
	if string(checkedIn) != string(want) {
		t.Errorf("tsgo/checker/rotor_exports.go is out of sync with %s/checker/rotor_exports.go.tmpl; edit the overlay and re-copy", overlayDir)
	}
}

func TestApplyPatchesReproducesRotorEdits(t *testing.T) {
	t.Chdir(filepath.Join("..", ".."))

	files := []string{
		filepath.Join("compiler", "program.go"),
		filepath.Join("parser", "references.go"),
		filepath.Join("printer", "emitcontext.go"),
	}
	base := t.TempDir()
	for _, rel := range files {
		data, err := os.ReadFile(filepath.Join(outDir, rel))
		if err != nil {
			t.Fatalf("read checked-in %s: %v", rel, err)
		}
		target := filepath.Join(base, outDir, rel)
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(target, data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	patches, err := filepath.Glob(filepath.Join(patchDir, "*.patch"))
	if err != nil {
		t.Fatal(err)
	}
	for i := len(patches) - 1; i >= 0; i-- {
		cmd := exec.Command("git", "apply", "--reverse", "--unsafe-paths", "--directory", base, patches[i])
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("reverse %s: %v\n%s", patches[i], err, out)
		}
	}
	if err := applyPatchesTo(base); err != nil {
		t.Fatalf("applyPatchesTo: %v", err)
	}
	for _, rel := range files {
		want, err := os.ReadFile(filepath.Join(outDir, rel))
		if err != nil {
			t.Fatal(err)
		}
		got, err := os.ReadFile(filepath.Join(base, outDir, rel))
		if err != nil {
			t.Fatal(err)
		}
		if string(got) != string(want) {
			t.Errorf("%s after patches differs from checked-in mirror", rel)
		}
	}
}
