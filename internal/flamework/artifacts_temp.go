package flamework

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
)

func createArtifactTemp(root *os.Root, directory, prefix string) (*os.File, string, error) {
	var random [8]byte
	for range 100 {
		if _, err := rand.Read(random[:]); err != nil {
			return nil, "", err
		}
		path := filepath.Join(directory, prefix+hex.EncodeToString(random[:])+".tmp")
		file, err := root.OpenFile(path, os.O_RDWR|os.O_CREATE|os.O_EXCL, 0o600)
		if err == nil {
			return file, path, nil
		}
		if !errors.Is(err, fs.ErrExist) {
			return nil, "", err
		}
	}
	return nil, "", errors.New("artifact temporary name collision limit reached")
}
