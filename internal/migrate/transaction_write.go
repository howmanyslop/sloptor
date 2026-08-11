package migrate

import (
	"crypto/rand"
	"encoding/hex"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"strconv"
)

func (tx *transactionFS) stageFile(change preparedChange, contents []byte) (createdFile, error) {
	if err := change.parent.revalidateDirectory(); err != nil {
		return createdFile{}, conflict(change.Path)
	}
	file, name, err := tx.createTemp(filepath.Dir(change.name))
	if err != nil {
		return createdFile{}, err
	}
	info, err := file.Stat()
	if err != nil {
		_ = file.Close()
		return createdFile{}, err
	}
	staged := createdFile{fileIdentity{fs: tx, name: name, path: tx.displayPath(name), info: info}}
	if err = file.Chmod(change.mode); err == nil {
		_, err = file.Write(contents)
	}
	if err == nil {
		err = file.Sync()
	}
	if closeErr := file.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		return createdFile{}, withCleanup(err, tx.removeCreatedFiles([]createdFile{staged}))
	}
	if err := staged.revalidateRegular(); err != nil {
		return createdFile{}, err
	}
	return staged, nil
}

func (tx *transactionFS) createTemp(parent string) (*os.File, string, error) {
	for range 100 {
		var random [16]byte
		if _, err := rand.Read(random[:]); err != nil {
			return nil, "", err
		}
		name := filepath.Join(parent, ".rotor-migrate-"+hex.EncodeToString(random[:]))
		file, err := tx.root.OpenFile(name, os.O_RDWR|os.O_CREATE|os.O_EXCL, 0o600)
		if errors.Is(err, fs.ErrExist) {
			continue
		}
		return file, name, err
	}
	return nil, "", errors.New("could not create unique migration stage file")
}

func (tx *transactionFS) writeBackup(change preparedChange) (createdFile, error) {
	if err := tx.validateDestination(change); err != nil {
		return createdFile{}, err
	}
	for index := 0; ; index++ {
		if err := change.parent.revalidateDirectory(); err != nil {
			return createdFile{}, conflict(change.Path)
		}
		name := change.name + ".bak"
		if index > 0 {
			name += "." + strconv.Itoa(index)
		}
		if info, err := tx.root.Lstat(name); err == nil {
			if !info.Mode().IsRegular() {
				return createdFile{}, conflict(tx.displayPath(name))
			}
			continue
		} else if !errors.Is(err, fs.ErrNotExist) {
			return createdFile{}, err
		}
		if err := change.parent.revalidateDirectory(); err != nil {
			return createdFile{}, conflict(change.Path)
		}
		file, err := tx.root.OpenFile(name, os.O_WRONLY|os.O_CREATE|os.O_EXCL, change.mode)
		if errors.Is(err, fs.ErrExist) {
			continue
		}
		if err != nil {
			return createdFile{}, err
		}
		info, err := file.Stat()
		if err != nil {
			_ = file.Close()
			return createdFile{}, err
		}
		backup := createdFile{fileIdentity{fs: tx, name: name, path: tx.displayPath(name), info: info}}
		if _, err = file.Write(change.Original); err == nil {
			err = file.Sync()
		}
		if closeErr := file.Close(); err == nil {
			err = closeErr
		}
		if err != nil {
			return createdFile{}, withCleanup(err, tx.removeCreatedFiles([]createdFile{backup}))
		}
		if err := backup.revalidateRegular(); err != nil {
			return createdFile{}, err
		}
		return backup, nil
	}
}

func (tx *transactionFS) removeCreatedFiles(files []createdFile) []error {
	var errs []error
	for _, file := range files {
		if file.name == "" {
			continue
		}
		info, err := tx.root.Lstat(file.name)
		if errors.Is(err, fs.ErrNotExist) {
			continue
		}
		if err != nil {
			errs = append(errs, err)
			continue
		}
		if !info.Mode().IsRegular() || !os.SameFile(file.info, info) {
			errs = append(errs, conflict(file.path))
			continue
		}
		if err := tx.root.Remove(file.name); err != nil && !errors.Is(err, fs.ErrNotExist) {
			errs = append(errs, err)
		}
	}
	return errs
}

func withCleanup(cause error, cleanup []error) error {
	if len(cleanup) == 0 {
		return cause
	}
	return errors.Join(cause, errors.Join(cleanup...))
}
