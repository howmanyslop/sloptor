// Package fsutil holds small filesystem helpers shared by the output writers.
package fsutil

import (
	"bytes"
	"os"
)

// SameContents reports whether path is a regular file holding exactly data.
// It compares sizes before reading, so a changed file usually costs one stat.
func SameContents(path string, data []byte) bool {
	info, err := os.Lstat(path)
	if err != nil || !info.Mode().IsRegular() || info.Size() != int64(len(data)) {
		return false
	}
	existing, err := os.ReadFile(path)
	return err == nil && bytes.Equal(existing, data)
}

// WriteFileIfChanged writes data to path unless path already holds it.
// It reports whether it wrote.
func WriteFileIfChanged(path string, data []byte, perm os.FileMode) (bool, error) {
	if SameContents(path, data) {
		return false, nil
	}
	return true, os.WriteFile(path, data, perm)
}
