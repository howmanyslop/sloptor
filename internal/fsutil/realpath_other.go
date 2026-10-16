//go:build !windows

package fsutil

import "path/filepath"

// EvalSymlinks is filepath.EvalSymlinks. Only Windows has path aliases
// (junctions, subst drives, 8.3 short names) that filepath.EvalSymlinks
// cannot see.
func EvalSymlinks(path string) (string, error) {
	return filepath.EvalSymlinks(path)
}
