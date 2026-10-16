package fsutil

import (
	"os"
	"path/filepath"

	"rotor/tsgo/vfs/osvfs"
)

// EvalSymlinks is filepath.EvalSymlinks that also resolves directory
// junctions, subst drives and 8.3 short names, as Node's fs.realpathSync.native
// does. Since Go 1.23 filepath.EvalSymlinks fails on any path through a
// junction, so Go and the Node sidecar spelled the same file differently.
// Like filepath.EvalSymlinks, it fails when the path does not exist.
func EvalSymlinks(path string) (string, error) {
	absolute, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	if _, err := os.Stat(absolute); err != nil {
		return "", err
	}
	return filepath.FromSlash(osvfs.FS().Realpath(filepath.ToSlash(absolute))), nil
}
