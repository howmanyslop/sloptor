package flamework

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
)

func stageArtifact(transaction *artifactTransaction, artifact Artifact) (stagedArtifact, error) {
	entry := stagedArtifact{transaction: transaction, artifact: artifact, mode: 0o644}
	info, err := transaction.root.Lstat(artifact.Path)
	if err == nil {
		if !info.Mode().IsRegular() {
			return stagedArtifact{}, fmt.Errorf("flamework: artifact destination %q is not a regular file", artifact.Path)
		}
		entry.destination = artifactIdentity{root: transaction.root, path: artifact.Path, info: info}
		entry.prior, err = readArtifactIdentity(entry.destination)
		if err != nil {
			return stagedArtifact{}, fmt.Errorf("flamework: read prior artifact %q: %w", artifact.Path, err)
		}
		entry.existed = true
		entry.mode = info.Mode().Perm()
	} else if !errors.Is(err, os.ErrNotExist) {
		return stagedArtifact{}, fmt.Errorf("flamework: stat artifact %q: %w", artifact.Path, err)
	}
	entry.createdDirectories, err = missingArtifactDirectories(transaction.root, filepath.Dir(artifact.Path))
	if err != nil {
		return stagedArtifact{}, fmt.Errorf("flamework: inspect artifact directory: %w", err)
	}
	if err := transaction.root.MkdirAll(filepath.Dir(artifact.Path), 0o755); err != nil {
		removeEmptyArtifactDirectories(transaction.root, entry.createdDirectories)
		return stagedArtifact{}, fmt.Errorf("flamework: create artifact directory: %w", err)
	}
	entry.parent, err = artifactDirectoryIdentity(transaction.root, filepath.Dir(artifact.Path))
	if err != nil {
		removeEmptyArtifactDirectories(transaction.root, entry.createdDirectories)
		return stagedArtifact{}, fmt.Errorf("flamework: identify artifact parent: %w", err)
	}
	if entry.existed {
		if err := entry.destination.revalidateRegular(); err != nil {
			removeEmptyArtifactDirectories(transaction.root, entry.createdDirectories)
			return stagedArtifact{}, fmt.Errorf("flamework: artifact destination changed: %w", err)
		}
	} else if err := requireArtifactMissing(transaction.root, artifact.Path); err != nil {
		removeEmptyArtifactDirectories(transaction.root, entry.createdDirectories)
		return stagedArtifact{}, fmt.Errorf("flamework: artifact destination changed: %w", err)
	}
	if artifact.Data == nil {
		return entry, nil
	}
	return stageArtifactData(entry)
}

func stageArtifactData(entry stagedArtifact) (stagedArtifact, error) {
	temp, tempPath, err := createArtifactTemp(entry.transaction.root, filepath.Dir(entry.artifact.Path), ".flamework-artifact-")
	if err != nil {
		removeEmptyArtifactDirectories(entry.transaction.root, entry.createdDirectories)
		return stagedArtifact{}, fmt.Errorf("flamework: stage artifact %q: %w", entry.artifact.Path, err)
	}
	entry.tempPath = tempPath
	tempInfo, err := temp.Stat()
	if err != nil {
		_ = temp.Close()
		_ = entry.transaction.root.Remove(tempPath)
		removeEmptyArtifactDirectories(entry.transaction.root, entry.createdDirectories)
		return stagedArtifact{}, fmt.Errorf("flamework: identify staged artifact %q: %w", entry.artifact.Path, err)
	}
	entry.temp = artifactIdentity{root: entry.transaction.root, path: tempPath, info: tempInfo}
	if _, err := temp.Write(entry.artifact.Data); err != nil {
		_ = temp.Close()
		_ = entry.transaction.root.Remove(tempPath)
		removeEmptyArtifactDirectories(entry.transaction.root, entry.createdDirectories)
		return stagedArtifact{}, fmt.Errorf("flamework: stage artifact %q: %w", entry.artifact.Path, err)
	}
	if err := temp.Chmod(entry.mode); err != nil {
		_ = temp.Close()
		_ = entry.transaction.root.Remove(tempPath)
		removeEmptyArtifactDirectories(entry.transaction.root, entry.createdDirectories)
		return stagedArtifact{}, fmt.Errorf("flamework: set artifact mode %q: %w", entry.artifact.Path, err)
	}
	if err := temp.Close(); err != nil {
		_ = entry.transaction.root.Remove(tempPath)
		removeEmptyArtifactDirectories(entry.transaction.root, entry.createdDirectories)
		return stagedArtifact{}, fmt.Errorf("flamework: close staged artifact %q: %w", entry.artifact.Path, err)
	}
	return entry, nil
}

func missingArtifactDirectories(root *os.Root, directory string) ([]string, error) {
	var missing []string
	for current := filepath.Clean(directory); ; current = filepath.Dir(current) {
		_, err := root.Stat(current)
		if err == nil {
			return missing, nil
		}
		if !errors.Is(err, fs.ErrNotExist) {
			return nil, err
		}
		missing = append(missing, current)
		parent := filepath.Dir(current)
		if parent == current {
			return missing, nil
		}
	}
}

func removeEmptyArtifactDirectories(root *os.Root, directories []string) {
	for _, directory := range directories {
		_ = root.Remove(directory)
	}
}
