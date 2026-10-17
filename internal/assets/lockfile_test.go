package assets

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestLockfileMissingIsEmpty(t *testing.T) {
	l, err := LoadLockfile(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if l.Version != 1 || len(l.Assets) != 0 {
		t.Fatalf("got %+v, want empty version-1 lockfile", l)
	}
}

func TestLockfileCorruptIsAnError(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, LockfileName), []byte("{nope"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := LoadLockfile(dir); err == nil {
		t.Fatal("corrupt lockfile should error, not silently reset asset ids")
	}
}

func TestLockfileRoundTripAndDeterminism(t *testing.T) {
	dir := t.TempDir()
	l := NewLockfile()
	l.Assets["assets/zebra.png"] = LockEntry{Hash: "sha256:zz", AssetID: 3}
	l.Assets["assets/alpha.png"] = LockEntry{Hash: "sha256:aa", AssetID: 1}
	l.Assets["assets/mid/beta.ogg"] = LockEntry{Hash: "sha256:bb", AssetID: 2}
	if err := l.Save(dir); err != nil {
		t.Fatal(err)
	}

	got, err := LoadLockfile(dir)
	if err != nil {
		t.Fatal(err)
	}
	if got.Version != 1 || len(got.Assets) != 3 {
		t.Fatalf("round-trip lost data: %+v", got)
	}
	if got.Assets["assets/alpha.png"] != (LockEntry{Hash: "sha256:aa", AssetID: 1}) {
		t.Fatalf("entry mismatch: %+v", got.Assets["assets/alpha.png"])
	}

	raw, err := os.ReadFile(filepath.Join(dir, LockfileName))
	if err != nil {
		t.Fatal(err)
	}
	s := string(raw)
	ia := strings.Index(s, "assets/alpha.png")
	ib := strings.Index(s, "assets/mid/beta.ogg")
	iz := strings.Index(s, "assets/zebra.png")
	if ia < 0 || ia >= ib || ib >= iz {
		t.Fatalf("keys not in sorted order: alpha@%d beta@%d zebra@%d\n%s", ia, ib, iz, s)
	}

	// Saving twice is byte-identical (deterministic) and leaves no temp files.
	if err := got.Save(dir); err != nil {
		t.Fatal(err)
	}
	raw2, err := os.ReadFile(filepath.Join(dir, LockfileName))
	if err != nil {
		t.Fatal(err)
	}
	if string(raw2) != s {
		t.Fatal("re-save is not deterministic")
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range entries {
		if strings.HasPrefix(e.Name(), ".rotor-tmp-") {
			t.Fatalf("leftover temp file %s", e.Name())
		}
	}
	if len(entries) != 1 {
		t.Fatalf("expected only %s in dir, got %d entries", LockfileName, len(entries))
	}
}
