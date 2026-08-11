package flamework

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
)

type Artifact struct {
	Path string
	Data []byte
}

type artifactTransaction struct {
	root *os.Root
}

type stagedArtifact struct {
	transaction        *artifactTransaction
	artifact           Artifact
	tempPath           string
	prior              []byte
	mode               os.FileMode
	existed            bool
	committed          bool
	createdDirectories []string
	parent             artifactIdentity
	destination        artifactIdentity
	temp               artifactIdentity
	committedIdentity  artifactIdentity
}

var (
	artifactPersistenceMutex sync.Mutex
	renameArtifact           = (*os.Root).Rename
)

func PrepareArtifacts(info *BuildInfo, metadataDirectory string, configJSON, globsJSON []byte) ([]Artifact, error) {
	if info == nil {
		return nil, fmt.Errorf("flamework: prepare artifacts: %w", ErrInvalidBuildInfo)
	}
	for name, data := range map[string][]byte{"config.json": configJSON, "globs.json": globsJSON} {
		if data != nil && !json.Valid(data) {
			return nil, fmt.Errorf("flamework: prepare %s: invalid JSON", name)
		}
	}
	buildInfoJSON, err := info.MarshalOrderedJSON()
	if err != nil {
		return nil, err
	}
	return []Artifact{
		{Path: filepath.Join(metadataDirectory, "config.json"), Data: append([]byte(nil), configJSON...)},
		{Path: filepath.Join(metadataDirectory, "globs.json"), Data: append([]byte(nil), globsJSON...)},
		{Path: info.Path(), Data: buildInfoJSON},
	}, nil
}

func PersistArtifacts(projectRoot string, artifacts []Artifact) error {
	artifactPersistenceMutex.Lock()
	defer artifactPersistenceMutex.Unlock()

	root, err := os.OpenRoot(projectRoot)
	if err != nil {
		return fmt.Errorf("flamework: open artifact root: %w", err)
	}
	defer func() { _ = root.Close() }()
	transaction := &artifactTransaction{root: root}
	staged, err := stageArtifacts(transaction, artifacts)
	if err != nil {
		return err
	}
	defer cleanupStagedArtifacts(staged)

	for index := range staged {
		if err := commitArtifact(&staged[index]); err != nil {
			rollbackErr := rollbackArtifacts(staged[:index])
			if rollbackErr != nil {
				return errors.Join(fmt.Errorf("flamework: commit artifact %q: %w", staged[index].artifact.Path, err), rollbackErr)
			}
			return fmt.Errorf("flamework: commit artifact %q: %w", staged[index].artifact.Path, err)
		}
		staged[index].committed = true
	}
	return nil
}

func stageArtifacts(transaction *artifactTransaction, artifacts []Artifact) ([]stagedArtifact, error) {
	staged := make([]stagedArtifact, 0, len(artifacts))
	seen := make(map[string]struct{}, len(artifacts))
	for _, artifact := range artifacts {
		path, err := localArtifactPath(artifact.Path)
		if err != nil {
			cleanupStagedArtifacts(staged)
			return nil, err
		}
		if _, exists := seen[path]; exists {
			cleanupStagedArtifacts(staged)
			return nil, fmt.Errorf("flamework: duplicate artifact path %q", path)
		}
		seen[path] = struct{}{}
		entry, err := stageArtifact(transaction, Artifact{Path: path, Data: cloneArtifactData(artifact.Data)})
		if err != nil {
			cleanupStagedArtifacts(staged)
			return nil, err
		}
		staged = append(staged, entry)
	}
	return staged, nil
}

func localArtifactPath(path string) (string, error) {
	cleaned := filepath.Clean(path)
	if path == "" || cleaned == "." || !filepath.IsLocal(path) || filepath.IsAbs(path) {
		return "", fmt.Errorf("flamework: artifact path must be local to the project root: %q", path)
	}
	return cleaned, nil
}

func cloneArtifactData(data []byte) []byte {
	if data == nil {
		return nil
	}
	return bytes.Clone(data)
}
