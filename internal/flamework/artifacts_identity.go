package flamework

import (
	"crypto/sha256"
	"errors"
	"io"
	"io/fs"
	"os"
)

type artifactIdentity struct {
	root      *os.Root
	path      string
	info      fs.FileInfo
	digest    [sha256.Size]byte
	hasDigest bool
}

func artifactRegularIdentity(root *os.Root, path string) (artifactIdentity, error) {
	info, err := root.Lstat(path)
	if err != nil {
		return artifactIdentity{}, err
	}
	if !info.Mode().IsRegular() {
		return artifactIdentity{}, errors.New("not a regular file")
	}
	return artifactIdentity{root: root, path: path, info: info}, nil
}

func artifactDirectoryIdentity(root *os.Root, path string) (artifactIdentity, error) {
	info, err := root.Lstat(path)
	if err != nil {
		return artifactIdentity{}, err
	}
	if !info.IsDir() {
		return artifactIdentity{}, errors.New("not a directory")
	}
	return artifactIdentity{root: root, path: path, info: info}, nil
}

func (identity artifactIdentity) revalidateRegular() error {
	current, err := artifactRegularIdentity(identity.root, identity.path)
	if err != nil || !os.SameFile(identity.info, current.info) {
		return errors.New("artifact identity changed")
	}
	if !identity.hasDigest {
		return nil
	}
	contents, err := readArtifactBytes(current)
	if err != nil {
		return err
	}
	if sha256.Sum256(contents) != identity.digest {
		return errors.New("artifact contents changed")
	}
	return nil
}

func (identity artifactIdentity) revalidateDirectory() error {
	current, err := artifactDirectoryIdentity(identity.root, identity.path)
	if err != nil || !os.SameFile(identity.info, current.info) {
		return errors.New("artifact parent identity changed")
	}
	return nil
}

func readArtifactIdentity(identity artifactIdentity) ([]byte, error) {
	return readArtifactBytes(identity)
}

func readArtifactBytes(identity artifactIdentity) (contents []byte, err error) {
	file, err := identity.root.Open(identity.path)
	if err != nil {
		return nil, err
	}
	defer func() { err = errors.Join(err, file.Close()) }()
	info, statErr := file.Stat()
	if statErr != nil || !info.Mode().IsRegular() || !os.SameFile(identity.info, info) {
		return nil, errors.New("opened artifact identity changed")
	}
	return io.ReadAll(file)
}

func requireArtifactMissing(root *os.Root, path string) error {
	_, err := root.Lstat(path)
	if errors.Is(err, fs.ErrNotExist) {
		return nil
	}
	if err != nil {
		return err
	}
	return errors.New("artifact destination appeared")
}

func removeOwnedArtifact(identity artifactIdentity) error {
	if err := identity.revalidateRegular(); err != nil {
		return err
	}
	return identity.root.Remove(identity.path)
}
