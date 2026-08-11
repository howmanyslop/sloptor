package compile

import (
	"context"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

const task7DIAG002Message = "Flamework cannot be built in a dirty environment, please delete your tsbuildinfo"

func TestTask7DIAG002_DirtyIncrementalStateMatchesPublicUpstream(t *testing.T) {
	// Given: clean and warm incremental builds in independent real compiler lanes.
	fixture, modules := task7DIAG002Paths(t)
	upstream := task7DIAG002Project(t, fixture, modules, false)
	native := task7DIAG002Project(t, fixture, modules, true)
	for _, run := range []struct {
		name string
		root string
		run  func(*testing.T, string) task7DIAG002Result
	}{
		{name: "upstream-clean", root: upstream, run: task7DIAG002Upstream},
		{name: "upstream-warm", root: upstream, run: task7DIAG002Upstream},
		{name: "native-clean", root: native, run: task7DIAG002Native},
		{name: "native-warm", root: native, run: task7DIAG002Native},
	} {
		t.Run(run.name, func(t *testing.T) {
			if result := run.run(t, run.root); result.exit != 0 || result.diagnostic != "" {
				t.Fatalf("%s = exit %d diagnostic %q, want clean success", run.name, result.exit, result.diagnostic)
			}
		})
	}

	// When: flamework.build disappears while each configured incremental state file remains.
	for _, lane := range []struct {
		name, root, state string
		run               func(*testing.T, string) task7DIAG002Result
	}{
		{name: "upstream", root: upstream, state: "out/task7.tsbuildinfo", run: task7DIAG002Upstream},
		{name: "native", root: native, state: "out/task7.rbxtsc.tsbuildinfo", run: task7DIAG002Native},
	} {
		t.Run(lane.name+"-dirty", func(t *testing.T) {
			if _, err := os.Stat(filepath.Join(lane.root, filepath.FromSlash(lane.state))); err != nil {
				t.Fatalf("%s incremental state precondition: %v", lane.name, err)
			}
			if err := os.Remove(filepath.Join(lane.root, "flamework.build")); err != nil {
				t.Fatalf("remove %s flamework.build: %v", lane.name, err)
			}
			before := task7ArtifactHash(t, lane.root)
			first := lane.run(t, lane.root)
			second := lane.run(t, lane.root)
			if first.exit != 1 || first.diagnostic != task7DIAG002Message {
				t.Fatalf("%s dirty result = exit %d diagnostic %q, want exit 1 %q", lane.name, first.exit, first.diagnostic, task7DIAG002Message)
			}
			if second != first {
				t.Fatalf("%s dirty result changed: first=%+v second=%+v", lane.name, first, second)
			}
			if after := task7ArtifactHash(t, lane.root); after != before {
				t.Fatalf("%s dirty state wrote artifacts: before=%s after=%s", lane.name, before, after)
			}
			if _, err := os.Stat(filepath.Join(lane.root, "flamework.build")); !errors.Is(err, os.ErrNotExist) {
				t.Fatalf("%s dirty state recreated flamework.build: %v", lane.name, err)
			}
		})
	}
}

func TestTask7DIAG002_MalformedPersistedStateFailsClosed(t *testing.T) {
	// Given: independent clean incremental projects with each real compiler-state adapter.
	fixture, modules := task7DIAG002Paths(t)
	for _, lane := range []struct {
		name, state string
		native      bool
		run         func(*testing.T, string) task7DIAG002Result
	}{
		{name: "upstream", state: "out/task7.tsbuildinfo", run: task7DIAG002Upstream},
		{name: "native", state: "out/task7.rbxtsc.tsbuildinfo", native: true, run: task7DIAG002Native},
	} {
		t.Run(lane.name, func(t *testing.T) {
			root := task7DIAG002Project(t, fixture, modules, lane.native)
			if result := lane.run(t, root); result.exit != 0 || result.diagnostic != "" {
				t.Fatalf("%s clean result = %+v, want success", lane.name, result)
			}
			if err := os.Remove(filepath.Join(root, "flamework.build")); err != nil {
				t.Fatal(err)
			}

			// When: the remaining persisted compiler state becomes malformed.
			statePath := filepath.Join(root, filepath.FromSlash(lane.state))
			if err := os.WriteFile(statePath, []byte("{\n"), 0o644); err != nil {
				t.Fatal(err)
			}
			before := task7ArtifactHash(t, root)
			result := lane.run(t, root)

			// Then: state content cannot turn the missing-build-info condition into a write or success.
			if result.exit != 1 || result.diagnostic != task7DIAG002Message {
				t.Fatalf("%s malformed result = %+v", lane.name, result)
			}
			if after := task7ArtifactHash(t, root); after != before {
				t.Fatalf("%s malformed state wrote artifacts: before=%s after=%s", lane.name, before, after)
			}
		})
	}
}

type task7DIAG002Result struct {
	exit       int
	diagnostic string
}

func task7DIAG002Paths(t *testing.T) (string, string) {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	repo := filepath.Clean(filepath.Join(filepath.Dir(file), "..", ".."))
	return filepath.Join(filepath.Dir(file), "testdata", "flamework", "task7-diag-dirty"), filepath.Join(repo, "testdata", "transformers", "project", "node_modules")
}

func task7DIAG002Project(t *testing.T, fixture, modules string, native bool) string {
	t.Helper()
	root := t.TempDir()
	copyDir(t, fixture, root)
	if err := os.Symlink(modules, filepath.Join(root, "node_modules")); err != nil {
		t.Fatal(err)
	}
	if !native {
		return root
	}
	configPath := filepath.Join(root, "tsconfig.json")
	config, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatal(err)
	}
	nativeConfig := strings.Replace(string(config), `"plugins": [{ "transform": "rbxts-transformer-flamework", "idGenerationMode": "short", "salt": "task7-diag-dirty" }]`, `"plugins": []`, 1)
	if nativeConfig == string(config) {
		t.Fatal("native fixture did not remove the legacy transformer")
	}
	if err := os.WriteFile(configPath, []byte(nativeConfig), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "rotor.toml"), []byte("[flamework]\nidGenerationMode = \"short\"\nsalt = \"task7-diag-dirty\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	return root
}

func task7DIAG002Upstream(t *testing.T, root string) task7DIAG002Result {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	command := exec.CommandContext(ctx, filepath.Join(root, "node_modules", ".bin", "rbxtsc"), "-p", "tsconfig.json")
	command.Dir = root
	output, err := command.CombinedOutput()
	if ctx.Err() != nil {
		t.Fatalf("upstream timed out: %v", ctx.Err())
	}
	return task7DIAG002CommandResult(t, err, string(output))
}

func task7DIAG002Native(t *testing.T, root string) task7DIAG002Result {
	t.Helper()
	_, diagnostics, err := BuildProjectWithOptions(root, ProjectOptions{})
	if err == nil {
		return task7DIAG002Result{}
	}
	diagnostic := strings.Join(diagnostics, "\n")
	if strings.Contains(diagnostic, task7DIAG002Message) {
		diagnostic = task7DIAG002Message
	}
	return task7DIAG002Result{exit: 1, diagnostic: diagnostic}
}

func task7DIAG002CommandResult(t *testing.T, err error, output string) task7DIAG002Result {
	t.Helper()
	if err == nil {
		return task7DIAG002Result{}
	}
	var exitError *exec.ExitError
	if !errors.As(err, &exitError) {
		t.Fatalf("upstream command: %v", err)
	}
	diagnostic := strings.TrimSpace(output)
	if strings.Contains(diagnostic, task7DIAG002Message) {
		diagnostic = task7DIAG002Message
	}
	return task7DIAG002Result{exit: exitError.ExitCode(), diagnostic: diagnostic}
}
