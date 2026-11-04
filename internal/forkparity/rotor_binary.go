package forkparity

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sync"
)

// RotorBinEnv names a prebuilt sloptor executable for the parity suites to run.
// When it is set, the suites skip building one. CI builds the release binary
// once and points every suite at it; locally, `mise run rust:forkparity` does
// the same.
const RotorBinEnv = "ROTOR_BIN"

var rustBuild struct {
	once sync.Once
	path string
	err  error
}

// RotorBinary returns the sloptor executable the parity suites compare against
// the pinned fork. It is the file named by ROTOR_BIN when that is set. Otherwise
// it is the release build of crates/rotor_cli, built once per process with cargo
// from repoRoot.
func RotorBinary(repoRoot string) (string, error) {
	if prebuilt := os.Getenv(RotorBinEnv); prebuilt != "" {
		path, err := filepath.Abs(prebuilt)
		if err != nil {
			return "", fmt.Errorf("resolve %s: %w", RotorBinEnv, err)
		}
		info, err := os.Stat(path)
		if err != nil {
			return "", fmt.Errorf("%s: %w", RotorBinEnv, err)
		}
		if info.IsDir() {
			return "", fmt.Errorf("%s names a directory: %s", RotorBinEnv, path)
		}
		return path, nil
	}
	rustBuild.once.Do(func() {
		rustBuild.path, rustBuild.err = buildRustRotor(repoRoot)
	})
	return rustBuild.path, rustBuild.err
}

func buildRustRotor(repoRoot string) (string, error) {
	build := exec.Command("cargo", "build", "--locked", "--release", "-p", "rotor_cli")
	build.Dir = repoRoot
	if output, err := build.CombinedOutput(); err != nil {
		return "", fmt.Errorf("build sloptor with cargo (set %s to skip the build): %w\n%s", RotorBinEnv, err, output)
	}

	metadata := exec.Command("cargo", "metadata", "--format-version", "1", "--no-deps", "--locked")
	metadata.Dir = repoRoot
	output, err := metadata.Output()
	if err != nil {
		return "", fmt.Errorf("locate the cargo target directory: %w", err)
	}
	var parsed struct {
		TargetDirectory string `json:"target_directory"`
	}
	if err := json.Unmarshal(output, &parsed); err != nil {
		return "", fmt.Errorf("parse cargo metadata: %w", err)
	}

	name := "sloptor"
	if runtime.GOOS == "windows" {
		name += ".exe"
	}
	path := filepath.Join(parsed.TargetDirectory, "release", name)
	if _, err := os.Stat(path); err != nil {
		return "", fmt.Errorf("find the built sloptor binary: %w", err)
	}
	return path, nil
}
