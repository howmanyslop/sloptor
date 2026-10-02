package includefiles

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestCopyKeepsUnchangedFileMtimes(t *testing.T) {
	dir := t.TempDir()
	if err := Copy(dir); err != nil {
		t.Fatal(err)
	}
	old := time.Unix(100, 0)
	for _, name := range Names() {
		if err := os.Chtimes(filepath.Join(dir, name), old, old); err != nil {
			t.Fatal(err)
		}
	}
	if err := Copy(dir); err != nil {
		t.Fatal(err)
	}
	for _, name := range Names() {
		info, err := os.Stat(filepath.Join(dir, name))
		if err != nil {
			t.Fatal(err)
		}
		if !info.ModTime().Equal(old) {
			t.Errorf("%s rewritten", name)
		}
	}
}
