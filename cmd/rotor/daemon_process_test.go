package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"

	"rotor/internal/compile"
)

const rotorCLITestHelperEnv = "ROTOR_CLI_TEST_HELPER"
const rotorCLIAbandonProjectEnv = "ROTOR_CLI_TEST_ABANDON_PROJECT"

func TestMain(m *testing.M) {
	if os.Getenv(rotorCLITestHelperEnv) == "1" && len(os.Args) > 1 && os.Args[1] == "__sidecar-daemon" {
		compile.EnablePersistentSidecarDaemon()
		os.Exit(run(os.Args[1:]))
	}
	if projectDir := os.Getenv(rotorCLIAbandonProjectEnv); projectDir != "" {
		compile.EnablePersistentSidecarDaemon()
		request, _ := json.Marshal(map[string]any{
			"protocol": 2, "operation": "transform",
			"projectDir": projectDir, "tsConfigPath": filepath.Join(projectDir, "tsconfig.json"),
			"compileFileNames": []string{filepath.Join(projectDir, "src", "main.ts")},
			"fileNames":        []string{filepath.Join(projectDir, "src", "main.ts")},
		})
		result, err := compile.SidecarDaemonRoundTrip(context.Background(), compile.SidecarDaemonCall{
			WorkspaceKey: projectDir,
			WorkerKey:    "abandoned-client",
			ProjectDir:   projectDir,
			SidecarDir:   os.Getenv("ROTOR_SIDECAR_PATH"),
			LeaseOwner:   fmt.Sprintf("%d-1", os.Getpid()),
			Payload:      request,
			StampFileNames: []string{
				filepath.Join(projectDir, "src", "main.ts"),
			},
		})
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		var response struct {
			ResultHandle string `json:"resultHandle"`
		}
		if json.Unmarshal(result.Payload, &response) != nil || response.ResultHandle == "" {
			fmt.Fprintln(os.Stderr, "transform did not retain a result")
			os.Exit(1)
		}
		os.Exit(0)
	}
	if os.Getenv(rotorCLITestHelperEnv) == "1" {
		compile.EnablePersistentSidecarDaemon()
		os.Exit(run(os.Args[1:]))
	}
	os.Exit(m.Run())
}

type externalRotorResult struct {
	code   int
	stdout string
	stderr string
}

func runExternalRotor(t *testing.T, runtimeDir string, environment map[string]string, stdin string, args ...string) externalRotorResult {
	t.Helper()
	executable, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	command := exec.CommandContext(ctx, executable, args...)
	values := map[string]string{
		rotorCLITestHelperEnv:      "1",
		"ROTOR_DAEMON_RUNTIME_DIR": runtimeDir,
		"ROTOR_SIDECAR_PATH":       repoSidecarPath(t),
	}
	for key, value := range environment {
		values[key] = value
	}
	command.Env = replaceExternalRotorEnvironment(values)
	command.Stdin = strings.NewReader(stdin)
	var stdout bytes.Buffer
	var stderr bytes.Buffer
	command.Stdout = &stdout
	command.Stderr = &stderr
	err = command.Run()
	if ctx.Err() != nil {
		t.Fatalf("external sloptor timed out: %v", ctx.Err())
	}
	code := 0
	if err != nil {
		var exitError *exec.ExitError
		if !errors.As(err, &exitError) {
			t.Fatalf("start external sloptor: %v", err)
		}
		code = exitError.ExitCode()
	}
	return externalRotorResult{code: code, stdout: stdout.String(), stderr: stderr.String()}
}

func replaceExternalRotorEnvironment(replacements map[string]string) []string {
	environment := make([]string, 0, len(os.Environ())+len(replacements))
	for _, entry := range os.Environ() {
		key, _, _ := strings.Cut(entry, "=")
		if _, replaced := replacements[key]; !replaced {
			environment = append(environment, entry)
		}
	}
	for key, value := range replacements {
		environment = append(environment, key+"="+value)
	}
	return environment
}

func writeExternalTransformerProject(t *testing.T, plugin, source string) string {
	t.Helper()
	dir := writeBuildableProject(t, source)
	mustWrite(t, filepath.Join(dir, "plugin.js"), plugin)
	mustWrite(t, filepath.Join(dir, "tsconfig.json"), `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out",
		"plugins": [{ "transform": "./plugin.js" }]
	},
	"include": ["src"]
}`)
	return dir
}

func TestPersistentSidecarAcrossCLIProcesses(t *testing.T) {
	runtimeDir := t.TempDir()
	projectDir := writeExternalTransformerProject(t, `const captured = process.env.ROTOR_PLUGIN_VALUE;
let transforms = 0;
module.exports = (program, config, helpers) => (context) => {
  const visit = (node) => helpers.ts.isStringLiteral(node)
    ? helpers.ts.factory.createStringLiteral(captured + "-" + transforms)
    : helpers.ts.visitEachChild(node, visit, context);
  return (source) => {
    transforms += 1;
    return helpers.ts.visitNode(source, visit);
  };
};
`, "export const value = \"source\";\n")
	t.Cleanup(func() { _ = runExternalRotor(t, runtimeDir, nil, "", "daemon", "stop") })

	first := runExternalRotor(t, runtimeDir, map[string]string{"ROTOR_PLUGIN_VALUE": "first"}, "", "build", "--noInclude", projectDir)
	if first.code != 0 {
		t.Fatalf("first build failed: stdout=%q stderr=%q", first.stdout, first.stderr)
	}
	outputPath := filepath.Join(projectDir, "out", "main.luau")
	firstOutput, err := os.ReadFile(outputPath)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(firstOutput), `"first-1"`) {
		t.Fatalf("first build output did not use first environment: %s", firstOutput)
	}
	firstStatus := runExternalRotor(t, runtimeDir, nil, "", "daemon", "status")
	if firstStatus.code != 0 {
		t.Fatalf("first daemon status failed: %s", firstStatus.stderr)
	}
	statusPattern := regexp.MustCompile(`pid ([0-9]+), ([0-9]+) workers`)
	firstMatch := statusPattern.FindStringSubmatch(firstStatus.stdout)
	if len(firstMatch) != 3 || firstMatch[2] != "1" {
		t.Fatalf("first daemon status = %q after stdout=%q stderr=%q", firstStatus.stdout, first.stdout, first.stderr)
	}

	second := runExternalRotor(t, runtimeDir, map[string]string{"ROTOR_PLUGIN_VALUE": "first"}, "", "build", "--noInclude", projectDir)
	if second.code != 0 {
		t.Fatalf("second build failed: stdout=%q stderr=%q", second.stdout, second.stderr)
	}
	secondOutput, err := os.ReadFile(outputPath)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(secondOutput), `"first-2"`) {
		t.Fatalf("second build did not reuse plugin state: %s", secondOutput)
	}
	secondStatus := runExternalRotor(t, runtimeDir, nil, "", "daemon", "status")
	secondMatch := statusPattern.FindStringSubmatch(secondStatus.stdout)
	if len(secondMatch) != 3 || secondMatch[1] != firstMatch[1] || secondMatch[2] != "1" {
		t.Fatalf("same environment did not reuse daemon and worker: first=%q second=%q", firstStatus.stdout, secondStatus.stdout)
	}

	third := runExternalRotor(t, runtimeDir, map[string]string{"ROTOR_PLUGIN_VALUE": "second"}, "", "build", "--noInclude", projectDir)
	if third.code != 0 {
		t.Fatalf("third build failed: stdout=%q stderr=%q", third.stdout, third.stderr)
	}
	thirdOutput, err := os.ReadFile(outputPath)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(thirdOutput), `"second-1"`) {
		t.Fatalf("changed environment reused stale plugin state: %s", thirdOutput)
	}
	thirdStatus := runExternalRotor(t, runtimeDir, nil, "", "daemon", "status")
	thirdMatch := statusPattern.FindStringSubmatch(thirdStatus.stdout)
	if len(thirdMatch) != 3 || thirdMatch[1] != firstMatch[1] || thirdMatch[2] != "2" {
		t.Fatalf("changed environment worker status = %q", thirdStatus.stdout)
	}
}

func TestPersistentSidecarDropsOverlayAcrossCLIProcesses(t *testing.T) {
	runtimeDir := t.TempDir()
	projectDir := writeExternalTransformerProject(t, `module.exports = () => () => (source) => {
  if (source.text.includes("overlay-only")) throw new Error("overlay-only marker reached plugin");
  return source;
};
`, "export const value = \"disk\";\n")
	t.Cleanup(func() { _ = runExternalRotor(t, runtimeDir, nil, "", "daemon", "stop") })
	sourcePath := filepath.Join(projectDir, "src", "main.ts")
	overlay, err := json.Marshal(map[string]any{"overlays": map[string]string{sourcePath: "export const value = \"overlay-only\";\n"}})
	if err != nil {
		t.Fatal(err)
	}
	first := runExternalRotor(t, runtimeDir, nil, string(overlay), "diagnostics", "--json", projectDir)
	if !strings.Contains(first.stdout, "overlay-only marker reached plugin") {
		t.Fatalf("overlay did not reach plugin: code=%d stdout=%q stderr=%q", first.code, first.stdout, first.stderr)
	}
	second := runExternalRotor(t, runtimeDir, nil, `{}`, "diagnostics", "--json", projectDir)
	if second.code != 0 || strings.Contains(second.stdout, "overlay-only marker reached plugin") || !strings.Contains(second.stdout, `"ok":true`) {
		t.Fatalf("removed overlay remained in worker: code=%d stdout=%q stderr=%q", second.code, second.stdout, second.stderr)
	}
}

func TestPersistentSidecarDoesNotReplayAcceptedTransform(t *testing.T) {
	runtimeDir := t.TempDir()
	markerPath := filepath.Join(t.TempDir(), "factory-runs")
	projectDir := writeExternalTransformerProject(t, `const fs = require("node:fs");
module.exports = () => {
  fs.appendFileSync(process.env.ROTOR_CRASH_MARKER, "run\n");
  process.exit(17);
};
`, "export const value = \"source\";\n")
	t.Cleanup(func() { _ = runExternalRotor(t, runtimeDir, nil, "", "daemon", "stop") })
	result := runExternalRotor(t, runtimeDir, map[string]string{"ROTOR_CRASH_MARKER": markerPath}, "", "build", "--noInclude", projectDir)
	if result.code == 0 {
		t.Fatalf("crashing transform build succeeded: stdout=%q stderr=%q", result.stdout, result.stderr)
	}
	marker, err := os.ReadFile(markerPath)
	if err != nil {
		t.Fatal(err)
	}
	if got := strings.Count(string(marker), "run\n"); got != 1 {
		t.Fatalf("accepted transform ran %d times, want once: stdout=%q stderr=%q", got, result.stdout, result.stderr)
	}
}

func TestPersistentSidecarReleasesAbandonedCLIResult(t *testing.T) {
	runtimeDir := t.TempDir()
	projectDir := writeExternalTransformerProject(t, `module.exports = (program, config, helpers) => (context) => {
  const visit = (node) => helpers.ts.isStringLiteral(node)
    ? helpers.ts.factory.createStringLiteral("retained")
    : helpers.ts.visitEachChild(node, visit, context);
  return (source) => helpers.ts.visitNode(source, visit);
};
`, "export const value = \"source\";\n")
	t.Cleanup(func() { _ = runExternalRotor(t, runtimeDir, nil, "", "daemon", "stop") })

	abandoned := runExternalRotor(t, runtimeDir, map[string]string{rotorCLIAbandonProjectEnv: projectDir}, "")
	if abandoned.code != 0 {
		t.Fatalf("abandoned client did not retain a result: stdout=%q stderr=%q", abandoned.stdout, abandoned.stderr)
	}
	deadline := time.Now().Add(3 * time.Second)
	for {
		status := runExternalRotor(t, runtimeDir, nil, "", "daemon", "status")
		if status.code == 0 && strings.Contains(status.stdout, ", 0 workers)") {
			return
		}
		if time.Now().After(deadline) {
			t.Fatalf("abandoned result kept its worker alive: stdout=%q stderr=%q", status.stdout, status.stderr)
		}
		time.Sleep(25 * time.Millisecond)
	}
}
