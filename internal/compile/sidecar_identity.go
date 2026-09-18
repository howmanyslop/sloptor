package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"

	"github.com/zeebo/xxh3"
	"rotor/tsgo/vfs/osvfs"
)

const sidecarNodeProtocolVersion = 3

// sidecarWorkerIdentity names the workspace daemon and the exact Node worker
// that can serve a build. The canonical paths are also the paths the caller
// should send to the daemon, so aliases cannot split one project across
// multiple processes.
type sidecarWorkerIdentity struct {
	WorkspaceKey string
	WorkerKey    string
	ProjectDir   string
	ConfigPath   string
	SidecarDir   string
	NodePath     string
	ChildEnv     []string
}

type sidecarNodeIdentity struct {
	Path        string `json:"path"`
	ContentHash string `json:"contentHash"`
}

type sidecarTypeScriptIdentity struct {
	EntryPath string `json:"entryPath"`
	Version   string `json:"version,omitempty"`
	EntryHash string `json:"entryHash"`
}

// resolveSidecarWorkerIdentity computes the two process-lifetime keys before a
// request crosses into the daemon. WorkspaceKey is stable for one canonical
// project, so every config shares its worker idle budget. WorkerKey changes for
// each config and whenever an input already loaded by a Node worker would make
// reusing that worker unsafe. A solution passes its invoking root directory as
// workspaceDir so referenced projects share the same bounded idle pool.
func resolveSidecarWorkerIdentity(projectDir, configPath string) (sidecarWorkerIdentity, error) {
	return resolveSidecarWorkerIdentityForWorkspace(projectDir, configPath, projectDir)
}

func resolveSidecarWorkerIdentityForWorkspace(projectDir, configPath, workspaceDir string) (sidecarWorkerIdentity, error) {
	canonicalProjectDir, err := canonicalSidecarIdentityPath(projectDir)
	if err != nil {
		return sidecarWorkerIdentity{}, fmt.Errorf("canonicalize sidecar project directory: %w", err)
	}
	canonicalConfigPath, err := canonicalSidecarIdentityPath(configPath)
	if err != nil {
		return sidecarWorkerIdentity{}, fmt.Errorf("canonicalize sidecar config path: %w", err)
	}
	if workspaceDir == "" {
		workspaceDir = projectDir
	}
	canonicalWorkspaceDir, err := canonicalSidecarIdentityPath(workspaceDir)
	if err != nil {
		return sidecarWorkerIdentity{}, fmt.Errorf("canonicalize sidecar workspace directory: %w", err)
	}

	sidecarDir, err := resolveSidecarDir()
	if err != nil {
		return sidecarWorkerIdentity{}, err
	}
	canonicalSidecarDir, err := canonicalSidecarIdentityPath(sidecarDir)
	if err != nil {
		return sidecarWorkerIdentity{}, fmt.Errorf("canonicalize sidecar directory: %w", err)
	}
	sidecarContent, err := sidecarContentIdentity(canonicalSidecarDir)
	if err != nil {
		return sidecarWorkerIdentity{}, err
	}

	nodePath, nodeIdentity, err := resolveSidecarNodeIdentity()
	if err != nil {
		return sidecarWorkerIdentity{}, err
	}
	typeScriptIdentity, err := resolveSidecarTypeScriptIdentity(canonicalProjectDir, canonicalSidecarDir)
	if err != nil {
		return sidecarWorkerIdentity{}, err
	}
	pluginFingerprints, err := transformerPluginFingerprints(canonicalProjectDir, canonicalConfigPath)
	if err != nil {
		return sidecarWorkerIdentity{}, err
	}
	childEnv := sidecarEnv(filepath.FromSlash(canonicalProjectDir), filepath.FromSlash(canonicalSidecarDir))
	environmentHash := sidecarEnvironmentHash(childEnv)

	workspaceSum := sha256.Sum256([]byte(canonicalWorkspaceDir))
	workspaceKey := hex.EncodeToString(workspaceSum[:])

	workerMaterial, err := json.Marshal(struct {
		WorkspaceKey       string                         `json:"workspaceKey"`
		ConfigPath         string                         `json:"configPath"`
		NodeProtocol       int                            `json:"nodeProtocol"`
		SidecarDir         string                         `json:"sidecarDir"`
		SidecarContent     string                         `json:"sidecarContent"`
		Node               sidecarNodeIdentity            `json:"node"`
		EnvironmentHash    string                         `json:"environmentHash"`
		TypeScript         sidecarTypeScriptIdentity      `json:"typeScript"`
		TransformerPlugins []transformerPluginFingerprint `json:"transformerPlugins,omitempty"`
	}{
		WorkspaceKey:       workspaceKey,
		ConfigPath:         canonicalConfigPath,
		NodeProtocol:       sidecarNodeProtocolVersion,
		SidecarDir:         canonicalSidecarDir,
		SidecarContent:     sidecarContent,
		Node:               nodeIdentity,
		EnvironmentHash:    environmentHash,
		TypeScript:         typeScriptIdentity,
		TransformerPlugins: pluginFingerprints,
	})
	if err != nil {
		return sidecarWorkerIdentity{}, err
	}
	workerSum := sha256.Sum256(workerMaterial)

	return sidecarWorkerIdentity{
		WorkspaceKey: workspaceKey,
		WorkerKey:    hex.EncodeToString(workerSum[:]),
		ProjectDir:   canonicalProjectDir,
		ConfigPath:   canonicalConfigPath,
		SidecarDir:   canonicalSidecarDir,
		NodePath:     nodePath,
		ChildEnv:     childEnv,
	}, nil
}

func canonicalSidecarIdentityPath(path string) (string, error) {
	if path == "" {
		return "", errors.New("path is empty")
	}
	absolute, err := filepath.Abs(filepath.FromSlash(path))
	if err != nil {
		return "", err
	}
	resolved, err := filepath.EvalSymlinks(absolute)
	if err != nil {
		return "", err
	}
	canonical := filepath.ToSlash(filepath.Clean(resolved))
	if !osvfs.FS().UseCaseSensitiveFileNames() {
		canonical = strings.ToLower(canonical)
	}
	return canonical, nil
}

func sidecarContentIdentity(sidecarDir string) (string, error) {
	names, embeddedHash, err := embeddedSidecarManifest()
	if err != nil {
		return "", err
	}
	if os.Getenv("ROTOR_SIDECAR_PATH") == "" {
		return "embedded:" + embeddedHash, nil
	}

	hasher := sha256.New()
	for _, name := range names {
		contents, readErr := os.ReadFile(filepath.Join(filepath.FromSlash(sidecarDir), filepath.FromSlash(name)))
		if readErr != nil {
			return "", fmt.Errorf("hash sidecar override file %q: %w", name, readErr)
		}
		hasher.Write([]byte(name))
		hasher.Write([]byte{0})
		hasher.Write(contents)
		hasher.Write([]byte{0})
	}
	return "override:" + hex.EncodeToString(hasher.Sum(nil)), nil
}

func resolveSidecarNodeIdentity() (string, sidecarNodeIdentity, error) {
	canonicalNodePath, err := resolveSidecarNodePath()
	if err != nil {
		return "", sidecarNodeIdentity{}, err
	}
	contentHash, err := sidecarNodeContentHash(canonicalNodePath)
	if err != nil {
		return "", sidecarNodeIdentity{}, err
	}
	return canonicalNodePath, sidecarNodeIdentity{
		Path:        canonicalNodePath,
		ContentHash: contentHash,
	}, nil
}

// resolveSidecarNodePath resolves a Node override before a sidecar changes into
// the project directory. A relative ROTOR_NODE_PATH is relative to the caller,
// just like exec.LookPath validates it, rather than to a later child cwd.
func resolveSidecarNodePath() (string, error) {
	nodeCommand := os.Getenv("ROTOR_NODE_PATH")
	if nodeCommand != "" {
		if _, err := os.Stat(nodeCommand); err != nil {
			return "", errors.New("node executable not found; rotor transformer plugins require Node.js on PATH")
		}
	} else {
		nodeCommand = "node"
	}
	nodePath, err := exec.LookPath(nodeCommand)
	if err != nil {
		return "", errors.New("node executable not found; rotor transformer plugins require Node.js on PATH")
	}
	canonicalNodePath, err := canonicalSidecarIdentityPath(nodePath)
	if err != nil {
		return "", fmt.Errorf("canonicalize Node executable: %w", err)
	}
	return canonicalNodePath, nil
}

func sidecarNodeContentHash(path string) (string, error) {
	for range 2 {
		file, err := os.Open(filepath.FromSlash(path))
		if err != nil {
			return "", fmt.Errorf("read Node executable: %w", err)
		}
		openedInfo, statErr := file.Stat()
		hasher := xxh3.New()
		_, copyErr := io.Copy(hasher, file)
		closeErr := file.Close()
		if statErr != nil {
			return "", fmt.Errorf("inspect open Node executable: %w", statErr)
		}
		if copyErr != nil {
			return "", fmt.Errorf("hash Node executable: %w", copyErr)
		}
		if closeErr != nil {
			return "", fmt.Errorf("close Node executable: %w", closeErr)
		}

		currentInfo, err := os.Stat(filepath.FromSlash(path))
		if err != nil {
			return "", fmt.Errorf("inspect Node executable after hashing: %w", err)
		}
		if !os.SameFile(openedInfo, currentInfo) || openedInfo.Size() != currentInfo.Size() || openedInfo.ModTime() != currentInfo.ModTime() {
			continue
		}

		sum := hasher.Sum128().Bytes()
		return hex.EncodeToString(sum[:]), nil
	}
	return "", errors.New("Node executable changed while its identity was being computed")
}

// sidecarEnvironmentHash fingerprints exactly what exec.Cmd.Env receives. It
// deliberately keeps neither names nor values after hashing: transformer
// plugins can depend on arbitrary environment variables, including secrets.
func sidecarEnvironmentHash(childEnv []string) string {
	environment := append([]string(nil), childEnv...)
	sort.Strings(environment)
	hasher := sha256.New()
	for _, entry := range environment {
		hasher.Write([]byte(entry))
		hasher.Write([]byte{0})
	}
	return hex.EncodeToString(hasher.Sum(nil))
}

func resolveSidecarTypeScriptIdentity(projectDir, sidecarDir string) (sidecarTypeScriptIdentity, error) {
	entryPath, version, unresolved := resolveTransformerPluginEntry(filepath.FromSlash(projectDir), "typescript")
	if unresolved != "" {
		entryPath, version, unresolved = resolveTransformerPluginEntry(filepath.FromSlash(sidecarDir), "typescript")
	}
	if unresolved != "" {
		return sidecarTypeScriptIdentity{}, fmt.Errorf("resolve TypeScript runtime entry: %s", unresolved)
	}
	canonicalEntryPath, err := canonicalSidecarIdentityPath(entryPath)
	if err != nil {
		return sidecarTypeScriptIdentity{}, fmt.Errorf("canonicalize TypeScript runtime entry: %w", err)
	}
	contents, err := os.ReadFile(filepath.FromSlash(canonicalEntryPath))
	if err != nil {
		return sidecarTypeScriptIdentity{}, fmt.Errorf("read TypeScript runtime entry: %w", err)
	}
	sum := sha256.Sum256(contents)
	return sidecarTypeScriptIdentity{
		EntryPath: canonicalEntryPath,
		Version:   version,
		EntryHash: hex.EncodeToString(sum[:]),
	}, nil
}
