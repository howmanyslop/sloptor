package migrate

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

const (
	legacyFlameworkPackage = "rbxts-transformer-flamework"
	packageManifest        = "package.json"
)

// PackageCleanup is the precise package-manager operation that removes the legacy transformer.
type PackageCleanup struct {
	Dir            string
	Executable     string
	Args           []string
	DisplayCommand string
	WorkspaceRoot  string
	PackageJSON    string
	Lockfile       string
}

// PackageCleanupRecoveryError reports a cleanup that can safely be rerun after migration completed.
type PackageCleanupRecoveryError struct {
	Cause        error
	RerunCommand string
}

func (e *PackageCleanupRecoveryError) Error() string {
	return fmt.Sprintf("package cleanup failed; migrated configuration remains valid. Rerun: %s: %v", e.RerunCommand, e.Cause)
}

func (e *PackageCleanupRecoveryError) Unwrap() error { return e.Cause }

type packageMetadata struct {
	Name           string          `json:"name"`
	PackageManager string          `json:"packageManager"`
	Workspaces     json.RawMessage `json:"workspaces"`
}

// PlanPackageCleanup finds the package that owns tsconfigPath and plans a scoped legacy-package removal.
func PlanPackageCleanup(tsconfigPath string) (PackageCleanup, error) {
	startDir, err := packageSearchDir(tsconfigPath)
	if err != nil {
		return PackageCleanup{}, err
	}
	ownerDir, owner, err := nearestPackage(startDir)
	if err != nil {
		return PackageCleanup{}, err
	}
	workspaceRoot, workspace, err := workspacePackage(ownerDir)
	if err != nil {
		return PackageCleanup{}, err
	}
	manager, lockfile, err := packageManager(workspaceRoot, owner, workspace)
	if err != nil {
		return PackageCleanup{}, err
	}
	if ownerDir != workspaceRoot && manager != "bun" && owner.Name == "" {
		return PackageCleanup{}, fmt.Errorf("workspace package %s has no name", filepath.Join(ownerDir, packageManifest))
	}

	dir, args := cleanupCommand(manager, ownerDir, workspaceRoot, owner.Name)
	cleanup := PackageCleanup{
		Dir:           dir,
		Executable:    manager,
		Args:          args,
		WorkspaceRoot: workspaceRoot,
		PackageJSON:   filepath.Join(ownerDir, packageManifest),
		Lockfile:      lockfile,
	}
	cleanup.DisplayCommand = displayCommand(cleanup.Dir, cleanup.Executable, cleanup.Args)
	return cleanup, nil
}

// Execute runs the planned command without using a shell.
func (c PackageCleanup) Execute(ctx context.Context) error {
	command := exec.CommandContext(ctx, c.Executable, c.Args...)
	command.Dir = c.Dir
	if err := command.Run(); err != nil {
		return &PackageCleanupRecoveryError{Cause: err, RerunCommand: c.DisplayCommand}
	}
	return nil
}

func packageSearchDir(tsconfigPath string) (string, error) {
	absPath, err := filepath.Abs(tsconfigPath)
	if err != nil {
		return "", fmt.Errorf("resolve tsconfig path %q: %w", tsconfigPath, err)
	}
	info, err := os.Stat(absPath)
	if err != nil {
		return "", fmt.Errorf("stat tsconfig path %q: %w", absPath, err)
	}
	if info.IsDir() {
		return absPath, nil
	}
	return filepath.Dir(absPath), nil
}

func nearestPackage(startDir string) (string, packageMetadata, error) {
	for dir := startDir; ; dir = filepath.Dir(dir) {
		metadata, exists, err := readPackage(filepath.Join(dir, packageManifest))
		if err != nil {
			return "", packageMetadata{}, err
		}
		if exists {
			return dir, metadata, nil
		}
		if parent := filepath.Dir(dir); parent == dir {
			break
		}
	}
	return "", packageMetadata{}, fmt.Errorf("no owning %s found above %s", packageManifest, startDir)
}

func workspacePackage(ownerDir string) (string, packageMetadata, error) {
	for dir := ownerDir; ; dir = filepath.Dir(dir) {
		metadata, exists, err := readPackage(filepath.Join(dir, packageManifest))
		if err != nil {
			return "", packageMetadata{}, err
		}
		_, pnpmWorkspaceErr := os.Stat(filepath.Join(dir, "pnpm-workspace.yaml"))
		if pnpmWorkspaceErr != nil && !errors.Is(pnpmWorkspaceErr, os.ErrNotExist) {
			return "", packageMetadata{}, fmt.Errorf("stat pnpm workspace file in %s: %w", dir, pnpmWorkspaceErr)
		}
		if exists && (len(metadata.Workspaces) != 0 || pnpmWorkspaceErr == nil) {
			return dir, metadata, nil
		}
		if parent := filepath.Dir(dir); parent == dir {
			break
		}
	}
	owner, _, err := readPackage(filepath.Join(ownerDir, packageManifest))
	if err != nil {
		return "", packageMetadata{}, err
	}
	return ownerDir, owner, nil
}

func readPackage(path string) (packageMetadata, bool, error) {
	data, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return packageMetadata{}, false, nil
	}
	if err != nil {
		return packageMetadata{}, false, fmt.Errorf("read %s: %w", path, err)
	}
	var metadata packageMetadata
	if err := json.Unmarshal(data, &metadata); err != nil {
		return packageMetadata{}, false, fmt.Errorf("parse %s: %w", path, err)
	}
	return metadata, true, nil
}

func packageManager(root string, owner, workspace packageMetadata) (string, string, error) {
	declared := workspace.PackageManager
	if declared == "" {
		declared = owner.PackageManager
	}
	manager := strings.SplitN(declared, "@", 2)[0]
	lockfiles := map[string][]string{
		"pnpm": {"pnpm-lock.yaml"},
		"npm":  {"package-lock.json", "npm-shrinkwrap.json"},
		"yarn": {"yarn.lock"},
		"bun":  {"bun.lock", "bun.lockb"},
	}
	if manager != "" {
		if _, ok := lockfiles[manager]; !ok {
			return "", "", fmt.Errorf("unsupported packageManager %q in %s", declared, root)
		}
		for _, name := range lockfiles[manager] {
			path := filepath.Join(root, name)
			if _, err := os.Stat(path); err == nil {
				return manager, path, nil
			} else if !errors.Is(err, os.ErrNotExist) {
				return "", "", fmt.Errorf("stat lockfile %s: %w", path, err)
			}
		}
		return "", "", fmt.Errorf("packageManager %q has no lockfile in %s", declared, root)
	}

	var detected []string
	for name, candidates := range lockfiles {
		for _, candidate := range candidates {
			if _, err := os.Stat(filepath.Join(root, candidate)); err == nil {
				detected = append(detected, name)
				break
			} else if !errors.Is(err, os.ErrNotExist) {
				return "", "", fmt.Errorf("stat lockfile %s: %w", candidate, err)
			}
		}
	}
	if len(detected) != 1 {
		return "", "", fmt.Errorf("cannot determine package manager from lockfiles in %s", root)
	}
	for _, candidate := range lockfiles[detected[0]] {
		path := filepath.Join(root, candidate)
		if _, err := os.Stat(path); err == nil {
			return detected[0], path, nil
		}
	}
	return "", "", fmt.Errorf("cannot find lockfile in %s", root)
}

func cleanupCommand(manager, ownerDir, workspaceRoot, workspaceName string) (string, []string) {
	if ownerDir == workspaceRoot || manager == "bun" {
		if manager == "npm" {
			return ownerDir, []string{"uninstall", legacyFlameworkPackage}
		}
		return ownerDir, []string{"remove", legacyFlameworkPackage}
	}
	switch manager {
	case "pnpm":
		return workspaceRoot, []string{"--filter", workspaceName, "remove", legacyFlameworkPackage}
	case "npm":
		return workspaceRoot, []string{"uninstall", "--workspace", workspaceName, legacyFlameworkPackage}
	case "yarn":
		return workspaceRoot, []string{"workspace", workspaceName, "remove", legacyFlameworkPackage}
	default:
		return ownerDir, []string{"remove", legacyFlameworkPackage}
	}
}

func displayCommand(dir, executable string, args []string) string {
	parts := []string{"cd", shellQuote(dir), "&&", shellQuote(executable)}
	for _, arg := range args {
		parts = append(parts, shellQuote(arg))
	}
	return strings.Join(parts, " ")
}

func shellQuote(value string) string {
	if value != "" && strings.IndexFunc(value, func(r rune) bool {
		return (r < 'a' || r > 'z') &&
			(r < 'A' || r > 'Z') &&
			(r < '0' || r > '9') &&
			!strings.ContainsRune("_@%+=:,./-", r)
	}) == -1 {
		return value
	}
	return "'" + strings.ReplaceAll(value, "'", "'\\''") + "'"
}
