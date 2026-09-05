package compile

import (
	"context"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"
)

const workerPIDPlugin = `const ts = require("typescript");

module.exports = function () {
	return (context) => (sourceFile) => {
		const marker = context.factory.createVariableStatement(undefined, context.factory.createVariableDeclarationList([
			context.factory.createVariableDeclaration("__WORKER_PID__", undefined, undefined, context.factory.createNumericLiteral(process.pid)),
		], ts.NodeFlags.Const));
		return context.factory.updateSourceFile(sourceFile, [marker].concat(sourceFile.statements));
	};
};
`

var workerPIDPattern = regexp.MustCompile(`__WORKER_PID__ = ([0-9]+)`)

// Catches a no-change project evicting a useful transformer worker from the
// workspace. The later repaired output must come from the same Node process.
func TestNoChangeBuildPreservesUsefulSidecarWorkers(t *testing.T) {
	setRepoSidecarPath(t)
	closeSidecarSessions()
	previousDaemonSetting := persistentSidecarDaemonEnabled.Load()
	persistentSidecarDaemonEnabled.Store(false)
	t.Cleanup(func() {
		persistentSidecarDaemonEnabled.Store(previousDaemonSetting)
		closeSidecarSessions()
	})

	workspaceDir := t.TempDir()
	projects := make([]string, 3)
	for index := range projects {
		projects[index] = writeWorkerResidencyProject(t, index)
		if _, diagnostics, err := BuildProjectWithOptions(projects[index], ProjectOptions{}); err != nil {
			t.Fatalf("seed project %d: %v (%v)", index, err, diagnostics)
		}
	}
	defaultBuildInfoPath := filepath.Join(projects[2], "tsconfig.rbxtsc.tsbuildinfo")
	if info, err := os.Stat(defaultBuildInfoPath); err != nil || !info.Mode().IsRegular() {
		t.Fatalf("seeded default build info %s: %v", defaultBuildInfoPath, err)
	}
	closeSidecarSessions()

	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	persistentSidecarDaemonEnabled.Store(true)
	firstIdentity, err := resolveSidecarWorkerIdentityForWorkspace(
		projects[0], filepath.Join(projects[0], "tsconfig.json"), workspaceDir,
	)
	if err != nil {
		t.Fatal(err)
	}
	daemonID, err := sidecarDaemonID(firstIdentity.WorkspaceKey)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, daemonID, 30*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, daemonID, done)
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_, _ = StopSidecarDaemons(ctx)
		select {
		case <-done:
		case <-ctx.Done():
		}
	})

	build := func(projectDir string) string {
		t.Helper()
		if err := os.Remove(filepath.Join(projectDir, "out", "main.luau")); err != nil {
			t.Fatal(err)
		}
		result, diagnostics, err := BuildProjectWithOptions(projectDir, ProjectOptions{sidecarWorkspaceDir: workspaceDir})
		if err != nil {
			t.Fatalf("build %s: %v (%v)", filepath.Base(projectDir), err, diagnostics)
		}
		output := result.Outputs["out/main.luau"]
		match := workerPIDPattern.FindStringSubmatch(output)
		if len(match) != 2 {
			t.Fatalf("build %s has no worker PID marker:\n%s", filepath.Base(projectDir), output)
		}
		return match[1]
	}

	firstPID := build(projects[0])
	_ = build(projects[1])
	infos, err := SidecarDaemonStatus(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(infos) != 1 || infos[0].WorkerCount != 2 {
		t.Fatalf("seeded daemon status = %+v, want two useful workers", infos)
	}

	noChangeTimings := NewBuildTimings()
	if _, diagnostics, err := BuildProjectWithOptions(projects[2], ProjectOptions{
		Timings:             noChangeTimings,
		sidecarWorkspaceDir: workspaceDir,
	}); err != nil {
		t.Fatalf("no-change build: %v (%v)", err, diagnostics)
	}
	if noChangeTimings.Counts.SelectedSources != 0 {
		t.Fatalf("no-change build selected %d sources", noChangeTimings.Counts.SelectedSources)
	}
	// Await the same presence decision used by BuildProject so an incorrect
	// hint has time to create a third worker and evict the least-recently-used
	// worker. The build above creates the manifest through the real emit path.
	if warmup := startPersistentSidecarWarmupIfCold(projects[2], ProjectOptions{sidecarWorkspaceDir: workspaceDir}); warmup != nil {
		warmup.wait()
	}
	if secondPID := build(projects[0]); secondPID != firstPID {
		t.Fatalf("useful worker changed from PID %s to %s after a no-change build", firstPID, secondPID)
	}
}

func writeWorkerResidencyProject(t *testing.T, index int) string {
	t.Helper()
	dir := writeProject(t, "@scope/worker-residency", "")
	config := sidecarDeclarationConfig(`[{ "transform": "./plugins/worker-pid.js" }]`)
	if index == 2 {
		config = strings.Replace(config, "{", "{\n\t\"extends\": [\"./tsconfig.incremental.json\"],", 1)
		if err := os.WriteFile(filepath.Join(dir, "tsconfig.incremental.json"), []byte(`{"compilerOptions":{"composite":true}}`), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	writeSidecarPluginFixture(t, dir, "", config)
	if index != 2 {
		enableIncrementalBuilds(t, dir)
	}
	if err := os.WriteFile(filepath.Join(dir, "plugins", "worker-pid.js"), []byte(workerPIDPlugin), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const project = "+string(rune('0'+index))+";\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	return dir
}
