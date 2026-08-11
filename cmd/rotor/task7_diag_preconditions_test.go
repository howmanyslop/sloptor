package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"slices"
	"strings"
	"testing"
	"time"
)

type task7CLIRun struct {
	exitCode int
	stdout   string
	stderr   string
}

var (
	task7CLIANSI               = regexp.MustCompile(`\x1b\[[0-9;]*m`)
	task7CLIUpstreamDiagnostic = regexp.MustCompile(`(?m)^src/index\.ts:([0-9]+):([0-9]+) - error TS([0-9]+): (.+)$`)
	task7CLINativeDiagnostic   = regexp.MustCompile(`(?m)^error TS([0-9]+): (.+)\n  --> src/index\.ts:([0-9]+):([0-9]+)$`)
)

type task7CLIDiagnostic struct {
	file    string
	line    string
	column  string
	code    string
	message string
}

func TestTask7DiagnosticPreconditionsMatchPinnedCompilerCLIs(t *testing.T) {
	// Given: the real pinned rbxtsc installation and a freshly built sloptor binary.
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	repo := filepath.Clean(filepath.Join(filepath.Dir(file), "..", ".."))
	modules := filepath.Join(repo, "testdata", "transformers", "project", "node_modules")
	manifestData, err := os.ReadFile(filepath.Join(modules, "rbxts-transformer-flamework", "package.json"))
	if err != nil {
		t.Fatal(err)
	}
	var manifest struct {
		Version string `json:"version"`
	}
	if err := json.Unmarshal(manifestData, &manifest); err != nil {
		t.Fatal(err)
	}
	if manifest.Version != "1.3.2" {
		t.Fatalf("rbxts-transformer-flamework version = %q, want 1.3.2", manifest.Version)
	}
	rbxtsc := filepath.Join(modules, ".bin", "rbxtsc")
	sloptor := filepath.Join(t.TempDir(), "sloptor")
	build := task7CLIRunCommand(t, filepath.Join(repo, "cmd", "rotor"), "go", "build", "-p=1", "-o", sloptor, ".")
	if build.exitCode != 0 {
		t.Fatalf("build sloptor exit = %d\nstdout=%s\nstderr=%s", build.exitCode, build.stdout, build.stderr)
	}

	t.Run("DIAG-005 missing tsconfig", func(t *testing.T) {
		// Given: an isolated directory with no tsconfig in its ancestor chain.
		root := t.TempDir()
		before := task7CLIInventory(t, root)

		// When: both public compiler binaries resolve the same missing project argument.
		upstream := task7CLIRunCommand(t, root, rbxtsc, "-p", "missing-tsconfig.json")
		native := task7CLIRunCommand(t, root, sloptor, "build", "-p", "missing-tsconfig.json")

		// Then: both stop at config resolution with exit 1 and no compiler output.
		if upstream.exitCode != 1 || task7CLINormalize(upstream.stdout) != "error TS roblox-ts: Unable to find tsconfig.json!" || upstream.stderr != "" {
			t.Fatalf("upstream result = %#v", upstream)
		}
		if native.exitCode != 1 || native.stdout != "" || task7CLINormalize(native.stderr) != "error: Unable to find tsconfig.json!" {
			t.Fatalf("native result = %#v", native)
		}
		task7CLIAssertNoLuau(t, root)
		if added := task7CLIAdded(before, task7CLIInventory(t, root)); len(added) != 0 {
			t.Fatalf("missing-tsconfig wrote files: %v", added)
		}
	})

	t.Run("DIAG-007 missing @flamework/core", func(t *testing.T) {
		// Given: equivalent plugin/native projects with the core package deliberately absent.
		upstreamRoot := task7CLIStageProject(t, repo, "upstream")
		nativeRoot := task7CLIStageProject(t, repo, "native")
		upstreamBefore := task7CLIInventory(t, upstreamRoot)
		nativeBefore := task7CLIInventory(t, nativeRoot)

		// When: both public compiler binaries build their isolated project.
		upstream := task7CLIRunCommand(t, upstreamRoot, rbxtsc, "-p", "tsconfig.json", "--noInclude", "--type", "model")
		native := task7CLIRunCommand(t, nativeRoot, sloptor, "build", "-p", "tsconfig.json", "--noInclude", "--type", "model")

		// Then: TS2307 is the earlier public precondition at the same source location.
		upstreamText := task7CLINormalize(upstream.stdout)
		nativeText := task7CLINormalize(native.stderr)
		for compiler, result := range map[string]task7CLIRun{"upstream": upstream, "native": native} {
			if result.exitCode != 1 {
				t.Fatalf("%s exit = %d, want 1\nstdout=%s\nstderr=%s", compiler, result.exitCode, result.stdout, result.stderr)
			}
		}
		want := task7CLIDiagnostic{file: "src/index.ts", line: "1", column: "27", code: "2307", message: "Cannot find module '@flamework/core' or its corresponding type declarations."}
		if got := task7CLIParseUpstreamDiagnostic(t, upstreamText); got != want {
			t.Fatalf("upstream diagnostic = %#v, want %#v", got, want)
		}
		if got := task7CLIParseNativeDiagnostic(t, nativeText); got != want {
			t.Fatalf("native diagnostic = %#v, want %#v", got, want)
		}
		task7CLIAssertNoLuau(t, upstreamRoot)
		task7CLIAssertNoLuau(t, nativeRoot)
		upstreamAdded := task7CLIAdded(upstreamBefore, task7CLIInventory(t, upstreamRoot))
		if !slices.Equal(upstreamAdded, []string{"flamework.build"}) {
			t.Fatalf("upstream ancillary writes = %v, want [flamework.build]", upstreamAdded)
		}
		nativeAdded := task7CLIAdded(nativeBefore, task7CLIInventory(t, nativeRoot))
		if len(nativeAdded) != 1 || !strings.HasPrefix(nativeAdded[0], ".rotor/cache/rojo/") || !strings.HasSuffix(nativeAdded[0], ".rojocache.json") {
			t.Fatalf("native ancillary writes = %v, want one Rojo cache", nativeAdded)
		}
		t.Logf("allowed ancillary writes: upstream=%v native=%v; include=[]", upstreamAdded, nativeAdded)
	})
}

func task7CLIRunCommand(t *testing.T, directory, name string, args ...string) task7CLIRun {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()
	command := exec.CommandContext(ctx, name, args...)
	command.Dir = directory
	command.Env = append(os.Environ(), "NO_COLOR=1")
	var stdout bytes.Buffer
	var stderr bytes.Buffer
	command.Stdout = &stdout
	command.Stderr = &stderr
	err := command.Run()
	if ctx.Err() != nil {
		t.Fatalf("%s timed out: %v", filepath.Base(name), ctx.Err())
	}
	exitCode := 0
	if err != nil {
		var exitError *exec.ExitError
		if !errors.As(err, &exitError) {
			t.Fatalf("run %s: %v", filepath.Base(name), err)
		}
		exitCode = exitError.ExitCode()
	}
	return task7CLIRun{exitCode: exitCode, stdout: stdout.String(), stderr: stderr.String()}
}

func task7CLIStageProject(t *testing.T, repo, kind string) string {
	t.Helper()
	root := t.TempDir()
	source := filepath.Join(repo, "cmd", "rotor", "testdata", "task7_diag_preconditions", kind)
	modules := filepath.Join(repo, "testdata", "transformers", "project", "node_modules")
	if err := os.CopyFS(root, os.DirFS(source)); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "node_modules", "@flamework"), 0o755); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"@rbxts", "roblox-ts", "typescript", "rbxts-transformer-flamework"} {
		if err := os.Symlink(filepath.Join(modules, name), filepath.Join(root, "node_modules", name)); err != nil {
			t.Fatal(err)
		}
	}
	return root
}

func task7CLIInventory(t *testing.T, root string) []string {
	t.Helper()
	var paths []string
	if err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		relative, err := filepath.Rel(root, path)
		if err != nil || relative == "." {
			return err
		}
		if relative == "node_modules" {
			return filepath.SkipDir
		}
		if !entry.IsDir() {
			paths = append(paths, filepath.ToSlash(relative))
		}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	return paths
}

func task7CLIAdded(before, after []string) []string {
	var added []string
	for _, path := range after {
		if !slices.Contains(before, path) {
			added = append(added, path)
		}
	}
	return added
}

func task7CLINormalize(value string) string {
	value = task7CLIANSI.ReplaceAllString(value, "")
	value = strings.ReplaceAll(value, "\r\n", "\n")
	return strings.TrimRight(value, "\n")
}

func task7CLIParseUpstreamDiagnostic(t *testing.T, output string) task7CLIDiagnostic {
	t.Helper()
	matches := task7CLIUpstreamDiagnostic.FindAllStringSubmatch(output, -1)
	if len(matches) != 1 {
		t.Fatalf("upstream diagnostic count = %d, want 1:\n%s", len(matches), output)
	}
	return task7CLIDiagnostic{file: "src/index.ts", line: matches[0][1], column: matches[0][2], code: matches[0][3], message: matches[0][4]}
}

func task7CLIParseNativeDiagnostic(t *testing.T, output string) task7CLIDiagnostic {
	t.Helper()
	matches := task7CLINativeDiagnostic.FindAllStringSubmatch(output, -1)
	if len(matches) != 1 {
		t.Fatalf("native diagnostic count = %d, want 1:\n%s", len(matches), output)
	}
	return task7CLIDiagnostic{file: "src/index.ts", line: matches[0][3], column: matches[0][4], code: matches[0][1], message: matches[0][2]}
}

func task7CLIAssertNoLuau(t *testing.T, root string) {
	t.Helper()
	count := 0
	if err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if !entry.IsDir() && (filepath.Ext(path) == ".luau" || filepath.Ext(path) == ".lua") {
			count++
		}
		return nil
	}); err != nil {
		t.Fatal(err)
	}
	if count != 0 {
		t.Fatalf("failure wrote %d compiler outputs", count)
	}
}
