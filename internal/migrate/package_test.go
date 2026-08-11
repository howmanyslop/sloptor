package migrate

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"testing"
)

func init() {
	if os.Getenv("ROTOR_PACKAGE_MANAGER_HELPER") == "" {
		return
	}
	if capture := os.Getenv("ROTOR_PACKAGE_MANAGER_CAPTURE"); capture != "" {
		if err := os.WriteFile(capture, []byte(strings.Join(os.Args[1:], "\n")), 0o644); err != nil {
			os.Exit(18)
		}
		os.Exit(0)
	}
	if os.Getenv("ROTOR_PACKAGE_MANAGER_SIGNAL") != "" {
		process, err := os.FindProcess(os.Getpid())
		if err != nil || process.Kill() != nil {
			os.Exit(19)
		}
	}
	os.Exit(17)
}

func TestPlanPackageCleanup_plans_workspace_manager_commands(t *testing.T) {
	tests := []struct {
		name           string
		packageManager string
		lockfile       string
		wantExecutable string
		wantDir        string
		wantArgs       []string
	}{
		{
			name:           "pnpm filters the owning workspace from the root",
			packageManager: "pnpm@10.0.0",
			lockfile:       "pnpm-lock.yaml",
			wantExecutable: "pnpm",
			wantDir:        "workspace",
			wantArgs:       []string{"--filter", "@scope/game", "remove", legacyFlameworkPackage},
		},
		{
			name:           "npm selects the owning workspace from the root",
			packageManager: "npm@11.0.0",
			lockfile:       "package-lock.json",
			wantExecutable: "npm",
			wantDir:        "workspace",
			wantArgs:       []string{"uninstall", "--workspace", "@scope/game", legacyFlameworkPackage},
		},
		{
			name:           "yarn delegates removal to the owning workspace from the root",
			packageManager: "yarn@4.0.0",
			lockfile:       "yarn.lock",
			wantExecutable: "yarn",
			wantDir:        "workspace",
			wantArgs:       []string{"workspace", "@scope/game", "remove", legacyFlameworkPackage},
		},
		{
			name:           "bun removes from the owning workspace",
			packageManager: "bun@1.2.0",
			lockfile:       "bun.lock",
			wantExecutable: "bun",
			wantDir:        "package",
			wantArgs:       []string{"remove", legacyFlameworkPackage},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Given
			root := t.TempDir()
			workspace := filepath.Join(root, "workspace")
			packageDir := filepath.Join(workspace, "packages", "game")
			writePackageFixture(t, filepath.Join(workspace, "package.json"), `{"name":"workspace","packageManager":"`+tt.packageManager+`","workspaces":["packages/*"]}`)
			writePackageFixture(t, filepath.Join(workspace, tt.lockfile), "")
			writePackageFixture(t, filepath.Join(packageDir, "package.json"), `{"name":"@scope/game"}`)
			writePackageFixture(t, filepath.Join(packageDir, "tsconfig.json"), "{}")

			// When
			plan, err := PlanPackageCleanup(filepath.Join(packageDir, "tsconfig.json"))
			// Then
			if err != nil {
				t.Fatalf("PlanPackageCleanup() error = %v", err)
			}
			if plan.Executable != tt.wantExecutable {
				t.Errorf("Executable = %q, want %q", plan.Executable, tt.wantExecutable)
			}
			wantDir := map[string]string{"workspace": workspace, "package": packageDir}[tt.wantDir]
			if plan.Dir != wantDir {
				t.Errorf("Dir = %q, want %q", plan.Dir, wantDir)
			}
			if !reflect.DeepEqual(plan.Args, tt.wantArgs) {
				t.Errorf("Args = %#v, want %#v", plan.Args, tt.wantArgs)
			}
			if !strings.Contains(plan.DisplayCommand, legacyFlameworkPackage) {
				t.Errorf("DisplayCommand = %q, want package name", plan.DisplayCommand)
			}
		})
	}
}

func TestPackageCleanupExecute_passes_malicious_workspace_name_as_one_argv_value(t *testing.T) {
	// Given
	root := t.TempDir()
	workspace := filepath.Join(root, "workspace")
	packageDir := filepath.Join(workspace, "packages", "game")
	maliciousName := `game; touch pwned; $(whoami)`
	writePackageFixture(t, filepath.Join(workspace, "package.json"), `{"name":"workspace","packageManager":"pnpm@10.0.0","workspaces":["packages/*"]}`)
	writePackageFixture(t, filepath.Join(workspace, "pnpm-lock.yaml"), "")
	writePackageFixture(t, filepath.Join(packageDir, "package.json"), `{"name":`+quoteJSON(t, maliciousName)+`}`)
	writePackageFixture(t, filepath.Join(packageDir, "tsconfig.json"), "{}")
	plan, err := PlanPackageCleanup(filepath.Join(packageDir, "tsconfig.json"))
	if err != nil {
		t.Fatalf("PlanPackageCleanup() error = %v", err)
	}
	capture := filepath.Join(root, "argv")
	fakePackageManager(t, "pnpm", capture, false)

	// When
	err = plan.Execute(context.Background())
	// Then
	if err != nil {
		t.Fatalf("Execute() error = %v", err)
	}
	got, err := os.ReadFile(capture)
	if err != nil {
		t.Fatalf("read captured argv: %v", err)
	}
	if got := strings.Split(strings.TrimSpace(string(got)), "\n"); !reflect.DeepEqual(got, plan.Args) {
		t.Errorf("argv = %#v, want %#v", got, plan.Args)
	} else {
		t.Logf("fake pnpm argv = %#v", got)
	}
	if _, err := os.Stat(filepath.Join(workspace, "pwned")); !errors.Is(err, os.ErrNotExist) {
		t.Errorf("malicious workspace name executed shell input: stat error = %v", err)
	}
	if !strings.Contains(plan.DisplayCommand, `'`+maliciousName+`'`) {
		t.Errorf("DisplayCommand = %q, want safely quoted workspace name", plan.DisplayCommand)
	}
}

func TestPackageCleanupExecute_returns_recovery_when_manager_is_signaled(t *testing.T) {
	// Given
	root := t.TempDir()
	packageDir := filepath.Join(root, "package")
	writePackageFixture(t, filepath.Join(packageDir, "package.json"), `{"name":"game","packageManager":"bun@1.2.0"}`)
	writePackageFixture(t, filepath.Join(packageDir, "bun.lock"), "")
	writePackageFixture(t, filepath.Join(packageDir, "tsconfig.json"), "{}")
	plan, err := PlanPackageCleanup(filepath.Join(packageDir, "tsconfig.json"))
	if err != nil {
		t.Fatalf("PlanPackageCleanup() error = %v", err)
	}
	fakePackageManager(t, "bun", "", true)

	// When
	err = plan.Execute(context.Background())

	// Then
	var recovery *PackageCleanupRecoveryError
	if !errors.As(err, &recovery) {
		t.Fatalf("Execute() error = %T %v, want PackageCleanupRecoveryError", err, err)
	}
	if recovery.RerunCommand != plan.DisplayCommand {
		t.Errorf("RerunCommand = %q, want %q", recovery.RerunCommand, plan.DisplayCommand)
	}
}

func TestPackageCleanupExecute_returns_recovery_with_exact_rerun_command_when_manager_fails(t *testing.T) {
	// Given
	root := t.TempDir()
	packageDir := filepath.Join(root, "package")
	writePackageFixture(t, filepath.Join(packageDir, "package.json"), `{"name":"game","packageManager":"bun@1.2.0"}`)
	writePackageFixture(t, filepath.Join(packageDir, "bun.lock"), "")
	writePackageFixture(t, filepath.Join(packageDir, "tsconfig.json"), "{}")
	plan, err := PlanPackageCleanup(filepath.Join(packageDir, "tsconfig.json"))
	if err != nil {
		t.Fatalf("PlanPackageCleanup() error = %v", err)
	}
	fakePackageManager(t, "bun", "", false)

	// When
	err = plan.Execute(context.Background())

	// Then
	var recovery *PackageCleanupRecoveryError
	if !errors.As(err, &recovery) {
		t.Fatalf("Execute() error = %T %v, want PackageCleanupRecoveryError", err, err)
	}
	if recovery.RerunCommand != plan.DisplayCommand {
		t.Errorf("RerunCommand = %q, want %q", recovery.RerunCommand, plan.DisplayCommand)
	}
	if !strings.Contains(err.Error(), plan.DisplayCommand) {
		t.Errorf("recovery message %q does not contain exact rerun command %q", err, plan.DisplayCommand)
	}
}

func writePackageFixture(t *testing.T, path, contents string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir %s: %v", path, err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}

func quoteJSON(t *testing.T, value string) string {
	t.Helper()
	return `"` + strings.ReplaceAll(strings.ReplaceAll(value, `\\`, `\\\\`), `"`, `\\"`) + `"`
}

func fakePackageManager(t *testing.T, name, capture string, signal bool) {
	t.Helper()
	shimDir := t.TempDir()
	if runtime.GOOS == "windows" {
		name += ".exe"
	}
	binary, err := os.ReadFile(os.Args[0])
	if err != nil {
		t.Fatalf("read test executable: %v", err)
	}
	path := filepath.Join(shimDir, name)
	if err := os.WriteFile(path, binary, 0o755); err != nil {
		t.Fatalf("write fake package manager: %v", err)
	}
	t.Setenv("PATH", shimDir)
	t.Setenv("ROTOR_PACKAGE_MANAGER_HELPER", "1")
	t.Setenv("ROTOR_PACKAGE_MANAGER_CAPTURE", capture)
	if signal {
		t.Setenv("ROTOR_PACKAGE_MANAGER_SIGNAL", "1")
	}
}
