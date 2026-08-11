package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestMigrateFlameworkUsesDefaultTSConfig(t *testing.T) {
	// Given
	dir := t.TempDir()
	writeFlameworkMigrationFixture(t, dir, `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework"}]}}`, "")
	originalDir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Chdir(dir); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chdir(originalDir) })

	// When
	code, out, errOut := runMigrate(t, []string{"flamework"})

	// Then
	if code != 0 {
		t.Fatalf("migrate flamework exit %d: %s", code, errOut)
	}
	if strings.Contains(mustReadFile(t, filepath.Join(dir, "tsconfig.json")), "rbxts-transformer-flamework") {
		t.Fatal("default tsconfig retained legacy plugin")
	}
	if !strings.Contains(out, "npm uninstall") {
		t.Fatalf("optional cleanup command missing: %s", out)
	}
}

func TestMigrateFlameworkStateMatrixDoesNotWriteOnFailure(t *testing.T) {
	tests := []struct {
		name      string
		tsconfig  string
		rotor     string
		wantCode  int
		wantError string
	}{
		{name: "already migrated is idempotent", tsconfig: `{}`, rotor: "[flamework]\n", wantCode: 0},
		{name: "native and legacy conflict", tsconfig: `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework"}]}}`, rotor: "[flamework]\n", wantCode: 1, wantError: "conflict"},
		{name: "neither configuration exists", tsconfig: `{}`, wantCode: 1, wantError: "nothing to migrate"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// Given
			dir := t.TempDir()
			writeFlameworkMigrationFixture(t, dir, test.tsconfig, test.rotor)
			tsconfigPath := filepath.Join(dir, "tsconfig.json")
			beforeTSConfig := mustReadFile(t, tsconfigPath)
			beforeRotor := test.rotor

			// When
			code, _, errOut := runMigrate(t, []string{"flamework", tsconfigPath})

			// Then
			if code != test.wantCode {
				t.Fatalf("exit %d, want %d: %s", code, test.wantCode, errOut)
			}
			if test.wantError != "" && !strings.Contains(errOut, test.wantError) {
				t.Fatalf("stderr %q does not contain %q", errOut, test.wantError)
			}
			if got := mustReadFile(t, tsconfigPath); got != beforeTSConfig {
				t.Fatalf("tsconfig changed: %q", got)
			}
			rotorPath := filepath.Join(dir, "rotor.toml")
			if beforeRotor == "" {
				if fileExists(rotorPath) {
					t.Fatal("rotor.toml created on failed preflight")
				}
			} else if got := mustReadFile(t, rotorPath); got != beforeRotor {
				t.Fatalf("rotor.toml changed: %q", got)
			}
		})
	}
}

func TestMigrateFlameworkRemovePackageExecutesManagerAfterConfigCommit(t *testing.T) {
	// Given
	dir := t.TempDir()
	writeFlameworkMigrationFixture(t, dir, `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework"}]}}`, "")
	manifest := `{"name":"game","packageManager":"npm@11.0.0","devDependencies":{"rbxts-transformer-flamework":"1.3.2"}}`
	if err := os.WriteFile(filepath.Join(dir, "package.json"), []byte(manifest), 0o644); err != nil {
		t.Fatal(err)
	}
	shimDir := t.TempDir()
	argvPath := filepath.Join(dir, "manager.argv")
	writePlatformShim(t, shimDir, "npm",
		"#!/bin/sh\nprintf '%s\\n' \"$@\" > \""+argvPath+"\"\nprintf '%s\\n' '{\"name\":\"game\",\"packageManager\":\"npm@11.0.0\",\"devDependencies\":{}}' > package.json\n",
		">\""+argvPath+"\" echo uninstall\r\n>>\""+argvPath+"\" echo rbxts-transformer-flamework\r\n>package.json echo {\"name\":\"game\",\"packageManager\":\"npm@11.0.0\",\"devDependencies\":{}}\r\n")
	t.Setenv("PATH", shimDir)

	// When
	code, _, errOut := runMigrate(t, []string{"flamework", filepath.Join(dir, "tsconfig.json"), "--remove-package"})

	// Then
	if code != 0 {
		t.Fatalf("migrate --remove-package exit %d: %s", code, errOut)
	}
	if got := mustReadFile(t, argvPath); got != "uninstall\nrbxts-transformer-flamework\n" {
		t.Fatalf("manager argv = %q", got)
	}
	if got := mustReadFile(t, filepath.Join(dir, "package.json")); strings.Contains(got, "rbxts-transformer-flamework") {
		t.Fatalf("dependency remains in package.json: %s", got)
	}
	if strings.Contains(mustReadFile(t, filepath.Join(dir, "tsconfig.json")), "rbxts-transformer-flamework") {
		t.Fatal("legacy plugin remains after cleanup")
	}
}

func TestMigrateFlameworkManagerFailureKeepsCommittedConfigsAndReportsRecovery(t *testing.T) {
	// Given
	dir := t.TempDir()
	writeFlameworkMigrationFixture(t, dir, `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework"}]}}`, "")
	shimDir := t.TempDir()
	writePlatformShim(t, shimDir, "npm", "#!/bin/sh\nexit 17\n", "exit /b 17\r\n")
	t.Setenv("PATH", shimDir)

	// When
	code, _, errOut := runMigrate(t, []string{"flamework", filepath.Join(dir, "tsconfig.json"), "--remove-package"})

	// Then
	if code != 1 {
		t.Fatalf("migrate --remove-package exit %d, want 1", code)
	}
	for _, want := range []string{"npm uninstall", "backups:", "rbxts-transformer-flamework"} {
		if !strings.Contains(errOut, want) {
			t.Fatalf("recovery stderr missing %q: %s", want, errOut)
		}
	}
	if strings.Contains(mustReadFile(t, filepath.Join(dir, "tsconfig.json")), "rbxts-transformer-flamework") {
		t.Fatal("manager failure rolled back valid config migration")
	}
	if !fileExists(filepath.Join(dir, "tsconfig.json.bak")) {
		t.Fatal("manager failure did not retain config backup")
	}
}

func writeFlameworkMigrationFixture(t *testing.T, dir, tsconfig, rotor string) {
	t.Helper()
	files := map[string]string{
		"tsconfig.json":     tsconfig,
		"package.json":      `{"name":"game","packageManager":"npm@11.0.0"}`,
		"package-lock.json": "{}\n",
	}
	if rotor != "" {
		files["rotor.toml"] = rotor
	}
	for name, contents := range files {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(contents), 0o644); err != nil {
			t.Fatal(err)
		}
	}
}
