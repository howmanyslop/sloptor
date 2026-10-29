package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"

	sidecarfs "rotor/tools/sidecar"
)

// resolveSidecarDir locates the Node transformer worker. ROTOR_SIDECAR_PATH
// overrides (repo-dev and tests); otherwise the embedded worker is extracted
// once into a content-addressed cache dir so released binaries work without
// a repo checkout.
func resolveSidecarDir() (string, error) {
	if dir := os.Getenv("ROTOR_SIDECAR_PATH"); dir != "" {
		return dir, nil
	}

	names, hash, err := embeddedSidecarManifest()
	if err != nil {
		return "", err
	}
	cacheRoot, err := os.UserCacheDir()
	if err != nil {
		return "", fmt.Errorf("compile: cannot locate user cache dir for sidecar extraction: %w", err)
	}
	dir := filepath.Join(cacheRoot, "rotor", "sidecar-"+hash)
	marker := filepath.Join(dir, ".complete")
	if _, err := os.Stat(marker); err == nil {
		return dir, nil
	}

	tmp := dir + fmt.Sprintf(".tmp-%d", os.Getpid())
	if err := os.RemoveAll(tmp); err != nil {
		return "", err
	}
	// 0o700/0o600: the extracted worker JS is executed by Node, so keep the
	// cache user-private on Unix (no-op on Windows ACLs).
	for _, name := range names {
		data, err := sidecarfs.FS.ReadFile(name)
		if err != nil {
			return "", err
		}
		target := filepath.Join(tmp, filepath.FromSlash(name))
		if err := os.MkdirAll(filepath.Dir(target), 0o700); err != nil {
			return "", err
		}
		if err := os.WriteFile(target, data, 0o600); err != nil {
			return "", err
		}
	}
	if err := os.WriteFile(filepath.Join(tmp, ".complete"), nil, 0o600); err != nil {
		return "", err
	}
	if err := os.Rename(tmp, dir); err != nil {
		// Lost a race with a concurrent extraction; accept the winner.
		if _, statErr := os.Stat(marker); statErr == nil {
			_ = os.RemoveAll(tmp)
			return dir, nil
		}
		return "", err
	}
	return dir, nil
}

func embeddedSidecarManifest() ([]string, string, error) {
	var names []string
	err := fs.WalkDir(sidecarfs.FS, ".", func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		names = append(names, path)
		return nil
	})
	if err != nil {
		return nil, "", err
	}
	sort.Strings(names)

	hasher := sha256.New()
	for _, name := range names {
		data, err := sidecarfs.FS.ReadFile(name)
		if err != nil {
			return nil, "", err
		}
		hasher.Write([]byte(name))
		hasher.Write([]byte{0})
		hasher.Write(data)
		hasher.Write([]byte{0})
	}
	return names, hex.EncodeToString(hasher.Sum(nil))[:16], nil
}
