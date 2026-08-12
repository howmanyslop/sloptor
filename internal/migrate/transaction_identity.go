package migrate

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
)

type transactionFS struct {
	root     *os.Root
	rootPath string
}

type fileIdentity struct {
	fs   *transactionFS
	name string
	path string
	info fs.FileInfo
}

type preparedChange struct {
	FileChange
	name   string
	mode   fs.FileMode
	target fileIdentity
	parent fileIdentity
}

type createdFile struct{ fileIdentity }

func openTransactionFS(trustedRoot string) (*transactionFS, error) {
	rootPath, err := filepath.Abs(trustedRoot)
	if err != nil {
		return nil, fmt.Errorf("resolve migration root: %w", err)
	}
	root, err := os.OpenRoot(rootPath)
	if err != nil {
		return nil, fmt.Errorf("open migration root: %w", err)
	}
	return &transactionFS{root: root, rootPath: rootPath}, nil
}

func (tx *transactionFS) localName(path string) (string, error) {
	absolute, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	name, err := filepath.Rel(tx.rootPath, absolute)
	if err != nil || name == "." || !filepath.IsLocal(name) {
		return "", conflict(path)
	}
	return name, nil
}

func (tx *transactionFS) displayPath(name string) string {
	return filepath.Join(tx.rootPath, name)
}

func (tx *transactionFS) preflight(changes []FileChange) ([]preparedChange, error) {
	seen := make(map[string]struct{}, len(changes))
	prepared := make([]preparedChange, 0, len(changes))
	for _, change := range changes {
		if change.Path == "" {
			return nil, errors.New("migration change has no path")
		}
		name, err := tx.localName(change.Path)
		if err != nil {
			return nil, conflict(change.Path)
		}
		if _, duplicate := seen[name]; duplicate {
			return nil, fmt.Errorf("migration has duplicate path %s", change.Path)
		}
		seen[name] = struct{}{}
		if bytes.Equal(change.Original, change.Updated) {
			continue
		}
		parent, err := tx.directoryIdentity(filepath.Dir(name))
		if err != nil {
			return nil, conflict(change.Path)
		}
		preparedChange := preparedChange{FileChange: change, name: name, mode: 0o644, parent: parent}
		if change.Existed {
			target, err := tx.regularIdentity(name)
			if err != nil {
				return nil, conflict(change.Path)
			}
			current, err := tx.readUnchanged(target)
			if err != nil || !bytes.Equal(current, change.Original) {
				return nil, conflict(change.Path)
			}
			preparedChange.target = target
			preparedChange.mode = target.info.Mode().Perm()
		} else if err := tx.requireMissing(name, change.Path); err != nil {
			return nil, err
		}
		prepared = append(prepared, preparedChange)
	}
	return prepared, nil
}

func (tx *transactionFS) validateDestination(change preparedChange) error {
	if err := change.parent.revalidateDirectory(); err != nil {
		return conflict(change.Path)
	}
	if !change.Existed {
		return tx.requireMissing(change.name, change.Path)
	}
	if err := change.target.revalidateRegular(); err != nil {
		return conflict(change.Path)
	}
	current, err := tx.readUnchanged(change.target)
	if err != nil || !bytes.Equal(current, change.Original) {
		return conflict(change.Path)
	}
	return nil
}

func (tx *transactionFS) validateRename(change preparedChange, staged createdFile) error {
	if err := tx.validateDestination(change); err != nil {
		return err
	}
	if err := staged.revalidateRegular(); err != nil {
		return conflict(change.Path)
	}
	return nil
}

func (tx *transactionFS) regularIdentity(name string) (fileIdentity, error) {
	info, err := tx.root.Lstat(name)
	if err != nil {
		return fileIdentity{}, err
	}
	if !info.Mode().IsRegular() {
		return fileIdentity{}, errors.New("not a regular file")
	}
	return fileIdentity{fs: tx, name: name, path: tx.displayPath(name), info: info}, nil
}

func (tx *transactionFS) directoryIdentity(name string) (fileIdentity, error) {
	info, err := tx.root.Lstat(name)
	if err != nil {
		return fileIdentity{}, err
	}
	if !info.IsDir() {
		return fileIdentity{}, errors.New("not a directory")
	}
	return fileIdentity{fs: tx, name: name, path: tx.displayPath(name), info: info}, nil
}

func (identity fileIdentity) revalidateRegular() error {
	current, err := identity.fs.regularIdentity(identity.name)
	if err != nil || !os.SameFile(identity.info, current.info) {
		return conflict(identity.path)
	}
	return nil
}

func (identity fileIdentity) revalidateDirectory() error {
	current, err := identity.fs.directoryIdentity(identity.name)
	if err != nil || !os.SameFile(identity.info, current.info) {
		return conflict(identity.path)
	}
	return nil
}

func (tx *transactionFS) readUnchanged(identity fileIdentity) ([]byte, error) {
	if err := identity.revalidateRegular(); err != nil {
		return nil, err
	}
	file, err := tx.root.Open(identity.name)
	if err != nil {
		return nil, err
	}
	info, err := file.Stat()
	if err != nil || !info.Mode().IsRegular() || !os.SameFile(identity.info, info) {
		if closeErr := file.Close(); closeErr != nil {
			return nil, errors.Join(conflict(identity.path), closeErr)
		}
		return nil, conflict(identity.path)
	}
	contents, err := io.ReadAll(file)
	closeErr := file.Close()
	if err != nil {
		return nil, err
	}
	if closeErr != nil {
		return nil, closeErr
	}
	return contents, nil
}

func (tx *transactionFS) requireMissing(name, path string) error {
	_, err := tx.root.Lstat(name)
	if errors.Is(err, fs.ErrNotExist) {
		return nil
	}
	return conflict(path)
}

func conflict(path string) error { return &TransactionConflictError{Path: path} }
