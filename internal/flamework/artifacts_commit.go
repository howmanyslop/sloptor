package flamework

import (
	"crypto/sha256"
	"errors"
	"fmt"
	"os"
	"path/filepath"
)

func commitArtifact(entry *stagedArtifact) error {
	if err := entry.parent.revalidateDirectory(); err != nil {
		return err
	}
	if entry.existed {
		if err := entry.destination.revalidateRegular(); err != nil {
			return err
		}
	} else if err := requireArtifactMissing(entry.transaction.root, entry.artifact.Path); err != nil {
		return err
	}
	if entry.artifact.Data == nil {
		if !entry.existed {
			return requireArtifactMissing(entry.transaction.root, entry.artifact.Path)
		}
		return removeOwnedArtifact(entry.destination)
	}
	if err := entry.temp.revalidateRegular(); err != nil {
		return err
	}
	if err := renameArtifact(entry.transaction.root, entry.tempPath, entry.artifact.Path); err != nil {
		return err
	}
	committed, err := artifactRegularIdentity(entry.transaction.root, entry.artifact.Path)
	if err != nil || !os.SameFile(entry.temp.info, committed.info) {
		return errors.New("committed artifact identity changed")
	}
	committed.digest = sha256.Sum256(entry.artifact.Data)
	committed.hasDigest = true
	if err := committed.revalidateRegular(); err != nil {
		return fmt.Errorf("committed artifact contents changed: %w", err)
	}
	entry.committedIdentity = committed
	entry.tempPath = ""
	return nil
}

func rollbackArtifacts(staged []stagedArtifact) error {
	var rollbackErrors []error
	for index := len(staged) - 1; index >= 0; index-- {
		entry := staged[index]
		if !entry.committed {
			continue
		}
		if !entry.existed {
			if err := removeOwnedArtifact(entry.committedIdentity); err != nil {
				rollbackErrors = append(rollbackErrors, err)
			}
			continue
		}
		if entry.artifact.Data == nil {
			if err := requireArtifactMissing(entry.transaction.root, entry.artifact.Path); err != nil {
				rollbackErrors = append(rollbackErrors, err)
				continue
			}
		} else if err := entry.committedIdentity.revalidateRegular(); err != nil {
			rollbackErrors = append(rollbackErrors, err)
			continue
		}
		if err := restoreArtifact(entry); err != nil {
			rollbackErrors = append(rollbackErrors, err)
		}
	}
	if len(rollbackErrors) > 0 {
		return fmt.Errorf("flamework: rollback artifacts: %w", errors.Join(rollbackErrors...))
	}
	return nil
}

func restoreArtifact(entry stagedArtifact) error {
	if err := entry.parent.revalidateDirectory(); err != nil {
		return err
	}
	temp, tempPath, err := createArtifactTemp(entry.transaction.root, filepath.Dir(entry.artifact.Path), ".flamework-rollback-")
	if err != nil {
		return err
	}
	tempInfo, err := temp.Stat()
	if err != nil {
		_ = temp.Close()
		return err
	}
	identity := artifactIdentity{root: entry.transaction.root, path: tempPath, info: tempInfo, digest: sha256.Sum256(entry.prior), hasDigest: true}
	defer func() { _ = removeOwnedArtifact(identity) }()
	if _, err := temp.Write(entry.prior); err != nil {
		_ = temp.Close()
		return err
	}
	if err := temp.Chmod(entry.mode); err != nil {
		_ = temp.Close()
		return err
	}
	if err := temp.Close(); err != nil {
		return err
	}
	if err := entry.parent.revalidateDirectory(); err != nil {
		return err
	}
	if err := identity.revalidateRegular(); err != nil {
		return err
	}
	return renameArtifact(entry.transaction.root, tempPath, entry.artifact.Path)
}

func cleanupStagedArtifacts(staged []stagedArtifact) {
	for index := len(staged) - 1; index >= 0; index-- {
		entry := staged[index]
		if entry.tempPath != "" {
			_ = removeOwnedArtifact(entry.temp)
		}
		removeEmptyArtifactDirectories(entry.transaction.root, entry.createdDirectories)
	}
}
