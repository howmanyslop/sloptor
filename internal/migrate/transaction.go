// Package migrate plans and commits the filesystem changes made by migration commands.
package migrate

import (
	"errors"
	"fmt"
)

// FileChange is an already-preflighted replacement. Original is used to detect races.
type FileChange struct {
	Path              string
	Original, Updated []byte
	Existed           bool
}

type Receipt struct{ Backups []string }

var ErrTransactionConflict = errors.New("migration transaction conflict")

// TransactionConflictError means a file changed after its migration plan was made.
type TransactionConflictError struct{ Path string }

func (e *TransactionConflictError) Error() string {
	return fmt.Sprintf("%s changed after migration was planned", e.Path)
}
func (e *TransactionConflictError) Is(target error) bool { return target == ErrTransactionConflict }

// CommitError reports the original failure and whether all written files were restored.
type CommitError struct {
	Cause          error
	RollbackErrors []error
	RolledBack     bool
}

func (e *CommitError) Error() string {
	if e.RolledBack {
		return fmt.Sprintf("commit migration: %v; changes rolled back", e.Cause)
	}
	return fmt.Sprintf("commit migration: %v; rollback failed: %v", e.Cause, errors.Join(e.RollbackErrors...))
}
func (e *CommitError) Unwrap() error { return e.Cause }

type rootedRename func(oldName, newName string) error

// Commit writes all planned changes beneath trustedRoot only after every target has been rechecked.
func Commit(trustedRoot string, changes []FileChange) (receipt Receipt, err error) {
	tx, err := openTransactionFS(trustedRoot)
	if err != nil {
		return Receipt{}, err
	}
	defer func() {
		if closeErr := tx.root.Close(); closeErr != nil {
			err = errors.Join(err, fmt.Errorf("close migration root: %w", closeErr))
		}
	}()
	return tx.commitWithRename(changes, tx.root.Rename)
}

func (tx *transactionFS) commitWithRename(changes []FileChange, rename rootedRename) (Receipt, error) {
	prepared, err := tx.preflight(changes)
	if err != nil {
		return Receipt{}, err
	}
	staged := make([]createdFile, len(prepared))
	for i, change := range prepared {
		staged[i], err = tx.stageFile(change, change.Updated)
		if err != nil {
			return Receipt{}, withCleanup(fmt.Errorf("stage %s: %w", change.Path, err), tx.removeCreatedFiles(staged[:i]))
		}
	}
	backups := make([]createdFile, 0, len(prepared))
	for _, change := range prepared {
		if !change.Existed {
			continue
		}
		if err := tx.validateDestination(change); err != nil {
			return Receipt{}, withCleanup(err, append(tx.removeCreatedFiles(backups), tx.removeCreatedFiles(staged)...))
		}
		backup, err := tx.writeBackup(change)
		if err != nil {
			return Receipt{}, withCleanup(fmt.Errorf("backup %s: %w", change.Path, err), append(tx.removeCreatedFiles(backups), tx.removeCreatedFiles(staged)...))
		}
		backups = append(backups, backup)
	}
	for i, change := range prepared {
		if err := tx.validateRename(change, staged[i]); err != nil {
			rollbackErrors := tx.rollback(prepared[:i], staged[:i], rename)
			rollbackErrors = append(rollbackErrors, tx.removeCreatedFiles(backups)...)
			rollbackErrors = append(rollbackErrors, tx.removeCreatedFiles(staged[i:])...)
			return Receipt{}, &CommitError{Cause: err, RollbackErrors: rollbackErrors, RolledBack: len(rollbackErrors) == 0}
		}
		if err := rename(staged[i].name, change.name); err != nil {
			rollbackErrors := tx.rollback(prepared[:i], staged[:i], rename)
			rollbackErrors = append(rollbackErrors, tx.removeCreatedFiles(backups)...)
			rollbackErrors = append(rollbackErrors, tx.removeCreatedFiles(staged[i:])...)
			return Receipt{}, &CommitError{Cause: fmt.Errorf("write %s: %w", change.Path, err), RollbackErrors: rollbackErrors, RolledBack: len(rollbackErrors) == 0}
		}
	}
	paths := make([]string, len(backups))
	for i, backup := range backups {
		paths[i] = backup.path
	}
	return Receipt{Backups: paths}, nil
}

func (tx *transactionFS) rollback(changes []preparedChange, staged []createdFile, rename rootedRename) []error {
	var rollbackErrors []error
	for index := len(changes) - 1; index >= 0; index-- {
		change := changes[index]
		if err := change.parent.revalidateDirectory(); err != nil {
			rollbackErrors = append(rollbackErrors, fmt.Errorf("restore %s: %w", change.Path, conflict(change.Path)))
			continue
		}
		updated := fileIdentity{fs: tx, name: change.name, path: change.Path, info: staged[index].info}
		if err := updated.revalidateRegular(); err != nil {
			rollbackErrors = append(rollbackErrors, fmt.Errorf("restore %s: %w", change.Path, conflict(change.Path)))
			continue
		}
		if !change.Existed {
			if err := tx.root.Remove(change.name); err != nil {
				rollbackErrors = append(rollbackErrors, fmt.Errorf("remove %s: %w", change.Path, err))
			}
			continue
		}
		restored, err := tx.stageFile(change, change.Original)
		if err == nil {
			err = tx.validateRenameReplacement(change, updated, restored)
		}
		if err == nil {
			err = rename(restored.name, change.name)
		}
		if cleanupErrors := tx.removeCreatedFiles([]createdFile{restored}); err == nil && len(cleanupErrors) > 0 {
			err = errors.Join(cleanupErrors...)
		}
		if err != nil {
			rollbackErrors = append(rollbackErrors, fmt.Errorf("restore %s: %w", change.Path, err))
		}
	}
	return rollbackErrors
}

func (tx *transactionFS) validateRenameReplacement(change preparedChange, updated fileIdentity, restored createdFile) error {
	if err := change.parent.revalidateDirectory(); err != nil {
		return conflict(change.Path)
	}
	if err := updated.revalidateRegular(); err != nil {
		return conflict(change.Path)
	}
	if err := restored.revalidateRegular(); err != nil {
		return conflict(change.Path)
	}
	return nil
}
