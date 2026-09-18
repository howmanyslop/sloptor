package compile

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

type sidecarIdentityFixture struct {
	projectDir     string
	configPath     string
	sidecarDir     string
	nodePath       string
	pluginPath     string
	typeScriptPath string
}

func newSidecarIdentityFixture(t *testing.T) sidecarIdentityFixture {
	t.Helper()
	root := t.TempDir()
	projectDir := filepath.Join(root, "project")
	sidecarDir := filepath.Join(root, "sidecar")
	nodeName := "node"
	if runtime.GOOS == "windows" {
		nodeName = "node.exe"
	}
	nodePath := filepath.Join(root, nodeName)
	pluginPath := filepath.Join(projectDir, "plugin.js")
	typeScriptPath := filepath.Join(projectDir, "node_modules", "typescript", "index.js")
	configPath := filepath.Join(projectDir, "tsconfig.json")

	writeSidecarIdentityFile(t, nodePath, "#!/bin/sh\nexit 0\n", 0o755)
	writeSidecarIdentityFile(t, configPath, `{"compilerOptions":{"plugins":[{"transform":"./plugin.js","flavor":"vanilla"}]}}`, 0o644)
	writeSidecarIdentityFile(t, pluginPath, "module.exports = function plugin() {};\n", 0o644)
	writeSidecarIdentityFile(t, typeScriptPath, "module.exports = { version: 'fixture-v1' };\n", 0o644)
	writeSidecarIdentityFile(t, filepath.Join(projectDir, "node_modules", "typescript", "package.json"), `{"name":"typescript","version":"0.0.0-fixture","main":"index.js"}`, 0o644)

	workerFiles := []string{
		"index.js",
		"lib/diagnostics.js",
		"lib/plugins.js",
		"lib/session.js",
		"main.js",
		"package.json",
	}
	for _, name := range workerFiles {
		writeSidecarIdentityFile(t, filepath.Join(sidecarDir, filepath.FromSlash(name)), "fixture "+name+"\n", 0o644)
	}

	t.Setenv("ROTOR_NODE_PATH", nodePath)
	t.Setenv("ROTOR_SIDECAR_PATH", sidecarDir)
	return sidecarIdentityFixture{
		projectDir:     projectDir,
		configPath:     configPath,
		sidecarDir:     sidecarDir,
		nodePath:       nodePath,
		pluginPath:     pluginPath,
		typeScriptPath: typeScriptPath,
	}
}

func writeSidecarIdentityFile(t *testing.T, path, contents string, mode os.FileMode) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), mode); err != nil {
		t.Fatal(err)
	}
}

// This catches one project being assigned multiple daemons when callers use
// different symlink spellings. The worker contract defines those spellings as
// one canonical workspace with one reusable worker identity.
func TestSidecarWorkerIdentityCanonicalizesWorkspaceAliases(t *testing.T) {
	fixture := newSidecarIdentityFixture(t)
	alias := filepath.Join(filepath.Dir(fixture.projectDir), "project-alias")
	if err := os.Symlink(fixture.projectDir, alias); err != nil {
		t.Skipf("symlinks unavailable: %v", err)
	}

	direct, err := resolveSidecarWorkerIdentity(fixture.projectDir, fixture.configPath)
	if err != nil {
		t.Fatal(err)
	}
	throughAlias, err := resolveSidecarWorkerIdentity(alias, filepath.Join(alias, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}

	if throughAlias.WorkspaceKey != direct.WorkspaceKey {
		t.Fatal("a symlink alias produced a second workspace key")
	}
	if throughAlias.WorkerKey != direct.WorkerKey {
		t.Fatal("a symlink alias produced a second worker key")
	}
	if throughAlias.ProjectDir != direct.ProjectDir || throughAlias.ConfigPath != direct.ConfigPath {
		t.Fatalf("canonical paths differ: alias = (%q, %q), direct = (%q, %q)", throughAlias.ProjectDir, throughAlias.ConfigPath, direct.ProjectDir, direct.ConfigPath)
	}
}

// All configs in one project must share the workspace daemon's bounded idle
// worker pool, while retaining separate workers because their loaded compiler
// and transformer configuration can differ.
func TestSidecarWorkerIdentityGroupsConfigsUnderOneWorkspace(t *testing.T) {
	fixture := newSidecarIdentityFixture(t)
	alternateConfig := filepath.Join(fixture.projectDir, "tsconfig.release.json")
	writeSidecarIdentityFile(t, alternateConfig, `{"compilerOptions":{"plugins":[{"transform":"./plugin.js","flavor":"vanilla"}]}}`, 0o644)

	primary, err := resolveSidecarWorkerIdentity(fixture.projectDir, fixture.configPath)
	if err != nil {
		t.Fatal(err)
	}
	alternate, err := resolveSidecarWorkerIdentity(fixture.projectDir, alternateConfig)
	if err != nil {
		t.Fatal(err)
	}

	if alternate.WorkspaceKey != primary.WorkspaceKey {
		t.Fatal("two configs in one project produced separate workspace daemons")
	}
	if alternate.WorkerKey == primary.WorkerKey {
		t.Fatal("two config paths in one project reused the same worker")
	}
}

// A solution's referenced projects share one daemon so the workspace-level
// idle-worker budget applies to the whole build graph.
func TestSidecarWorkerIdentityGroupsSolutionProjectsUnderOneWorkspace(t *testing.T) {
	fixture := newSidecarIdentityFixture(t)
	secondProjectDir := filepath.Join(filepath.Dir(fixture.projectDir), "second-project")
	secondConfigPath := filepath.Join(secondProjectDir, "tsconfig.json")
	writeSidecarIdentityFile(t, secondConfigPath, `{"compilerOptions":{"plugins":[{"transform":"../project/plugin.js"}]}}`, 0o644)
	writeSidecarIdentityFile(t, filepath.Join(secondProjectDir, "node_modules", "typescript", "index.js"), "module.exports = { version: 'fixture-v1' };\n", 0o644)
	writeSidecarIdentityFile(t, filepath.Join(secondProjectDir, "node_modules", "typescript", "package.json"), `{"name":"typescript","version":"0.0.0-fixture","main":"index.js"}`, 0o644)

	first, err := resolveSidecarWorkerIdentityForWorkspace(fixture.projectDir, fixture.configPath, filepath.Dir(fixture.projectDir))
	if err != nil {
		t.Fatal(err)
	}
	second, err := resolveSidecarWorkerIdentityForWorkspace(secondProjectDir, secondConfigPath, filepath.Dir(fixture.projectDir))
	if err != nil {
		t.Fatal(err)
	}
	if second.WorkspaceKey != first.WorkspaceKey {
		t.Fatal("solution projects produced separate workspace daemons")
	}
	if second.WorkerKey == first.WorkerKey {
		t.Fatal("solution projects reused one project worker")
	}
}

// A persistent Node process snapshots its runtime code, plugin configuration,
// and environment. Reusing its key after any of those inputs changes would run
// a later build with stale process state.
func TestSidecarWorkerIdentityChangesWithLoadedRuntimeInputs(t *testing.T) {
	tests := []struct {
		name   string
		mutate func(*testing.T, sidecarIdentityFixture)
	}{
		{
			name: "sidecar override",
			mutate: func(t *testing.T, fixture sidecarIdentityFixture) {
				writeSidecarIdentityFile(t, filepath.Join(fixture.sidecarDir, "index.js"), "changed sidecar index\n", 0o644)
			},
		},
		{
			name: "plugin entry",
			mutate: func(t *testing.T, fixture sidecarIdentityFixture) {
				writeSidecarIdentityFile(t, fixture.pluginPath, "module.exports = function changedPlugin() {};\n", 0o644)
			},
		},
		{
			name: "TypeScript runtime entry",
			mutate: func(t *testing.T, fixture sidecarIdentityFixture) {
				writeSidecarIdentityFile(t, fixture.typeScriptPath, "module.exports = { version: 'fixture-v2' };\n", 0o644)
			},
		},
		{
			name: "plugin config",
			mutate: func(t *testing.T, fixture sidecarIdentityFixture) {
				writeSidecarIdentityFile(t, fixture.configPath, `{"compilerOptions":{"plugins":[{"transform":"./plugin.js","flavor":"chocolate"}]}}`, 0o644)
			},
		},
		{
			name: "Node executable contents at the same path",
			mutate: func(t *testing.T, fixture sidecarIdentityFixture) {
				before, err := os.Stat(fixture.nodePath)
				if err != nil {
					t.Fatal(err)
				}
				writeSidecarIdentityFile(t, fixture.nodePath, "#!/bin/sh\nexit 1\n", 0o755)
				if err := os.Chtimes(fixture.nodePath, before.ModTime(), before.ModTime()); err != nil {
					t.Fatal(err)
				}
			},
		},
		{
			name: "otherwise harmless environment variable",
			mutate: func(t *testing.T, _ sidecarIdentityFixture) {
				t.Setenv("ROTOR_IDENTITY_FIXTURE_ENV", "changed")
			},
		},
	}

	for index, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			fixture := newSidecarIdentityFixture(t)
			t.Setenv("ROTOR_IDENTITY_FIXTURE_ENV", fmt.Sprintf("baseline-%d", index))
			before, err := resolveSidecarWorkerIdentity(fixture.projectDir, fixture.configPath)
			if err != nil {
				t.Fatal(err)
			}

			test.mutate(t, fixture)
			after, err := resolveSidecarWorkerIdentity(fixture.projectDir, fixture.configPath)
			if err != nil {
				t.Fatal(err)
			}

			if after.WorkspaceKey != before.WorkspaceKey {
				t.Fatal("a runtime input change moved the canonical workspace")
			}
			if after.WorkerKey == before.WorkerKey {
				t.Fatal("a loaded runtime input change reused the old worker key")
			}
		})
	}
}
