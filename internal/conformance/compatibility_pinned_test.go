package conformance

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

const (
	pinnedRobloxTSVersion = "3.0.0-dev-3106b14"
	pinnedRobloxTSCommit  = "3106b1492a2cf5b5b06f73354e94b03e3eb45b4e"
	pinnedRobloxTypes     = "1.0.954"
)

var pinnedCompilerInstallMu sync.Mutex

type compatibilityEnvironment struct {
	root        string
	tools       RuntimeTools
	upstreamCLI string
}

func TestPinnedCompatibility(t *testing.T) {
	for _, fixture := range []string{"loops", "calling-conventions", "iterator-rest", "switch"} {
		t.Run(fixture, func(t *testing.T) {
			runPinnedCompatibilityFixture(t, fixture)
		})
	}
}

func runPinnedCompatibilityFixture(t *testing.T, fixture string) {
	t.Helper()
	environment := requireCompatibilityEnvironment(t)

	environment.runRuntime(t, compatibilityCompilerUpstream, fixture)
	environment.runRuntime(t, compatibilityCompilerRotorOptimized, fixture)
	if fixture == "loops" {
		environment.runRuntime(t, compatibilityCompilerRotorGeneral, fixture)
	}
}

func requireCompatibilityEnvironment(t *testing.T) compatibilityEnvironment {
	t.Helper()
	return compatibilityEnvironment{
		root:        repoRoot(t),
		tools:       requireCompatibilityRuntimeTools(t),
		upstreamCLI: requirePinnedCompiler(t),
	}
}

func requireCompatibilityRuntimeTools(t *testing.T) RuntimeTools {
	t.Helper()
	tools := detectRuntimeTools()
	if tools.Rojo == "" || tools.Lune == "" {
		t.Fatalf("pinned compatibility runtime requires installed tools: %s", runtimeSuiteSkipReason(tools))
	}
	return tools
}

func requirePinnedCompiler(t *testing.T) string {
	t.Helper()
	if override := strings.TrimSpace(os.Getenv("ROTOR_PINNED_RBXTS_CLI")); override != "" {
		verifyPinnedCompiler(t, override)
		return override
	}

	pinnedCompilerInstallMu.Lock()
	defer pinnedCompilerInstallMu.Unlock()

	cacheRoot, err := os.UserCacheDir()
	if err != nil {
		t.Fatal(err)
	}
	installDir := filepath.Join(cacheRoot, "rotor", "compatibility", "roblox-ts-"+pinnedRobloxTSVersion+"-types-"+pinnedRobloxTypes)
	cli := filepath.Join(installDir, "node_modules", "roblox-ts", "out", "CLI", "cli.js")
	if pinnedCompilerInstallValid(installDir, cli) {
		verifyPinnedCompiler(t, cli)
		return cli
	}
	if _, err := os.Stat(installDir); err == nil {
		t.Fatalf("pinned compiler cache is incomplete: %s", installDir)
	}

	parent := filepath.Dir(installDir)
	if err := os.MkdirAll(parent, 0o755); err != nil {
		t.Fatal(err)
	}
	tmpDir, err := os.MkdirTemp(parent, ".roblox-ts-install-")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tmpDir)

	verifyPinnedPackageMetadata(t)
	cmd := exec.Command(
		"npm", "install", "--prefix", tmpDir, "--no-save", "--ignore-scripts", "--package-lock=false",
		"roblox-ts@"+pinnedRobloxTSVersion,
		"@rbxts/types@"+pinnedRobloxTypes,
	)
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("install pinned roblox-ts: %v\n%s", err, output)
	}
	if err := os.WriteFile(filepath.Join(tmpDir, ".git-head"), []byte(pinnedRobloxTSCommit+"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Rename(tmpDir, installDir); err != nil {
		if winnerCLI := filepath.Join(installDir, "node_modules", "roblox-ts", "out", "CLI", "cli.js"); pinnedCompilerInstallValid(installDir, winnerCLI) {
			verifyPinnedCompiler(t, winnerCLI)
			return winnerCLI
		}
		t.Fatalf("publish pinned compiler cache: %v", err)
	}
	cli = filepath.Join(installDir, "node_modules", "roblox-ts", "out", "CLI", "cli.js")
	verifyPinnedCompiler(t, cli)
	return cli
}

func pinnedCompilerInstallValid(installDir, cli string) bool {
	commit, err := os.ReadFile(filepath.Join(installDir, ".git-head"))
	if err != nil || strings.TrimSpace(string(commit)) != pinnedRobloxTSCommit {
		return false
	}
	if _, err = os.Stat(cli); err != nil {
		return false
	}
	packageJSON, err := os.ReadFile(filepath.Join(installDir, "node_modules", "@rbxts", "types", "package.json"))
	if err != nil {
		return false
	}
	var packageInfo struct {
		Version string `json:"version"`
	}
	return json.Unmarshal(packageJSON, &packageInfo) == nil && packageInfo.Version == pinnedRobloxTypes
}

func verifyPinnedPackageMetadata(t *testing.T) {
	t.Helper()
	cmd := exec.Command("npm", "view", "roblox-ts@"+pinnedRobloxTSVersion, "gitHead", "--json")
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("resolve pinned roblox-ts metadata: %v\n%s", err, output)
	}
	var commit string
	if err := json.Unmarshal(output, &commit); err != nil {
		t.Fatalf("decode pinned roblox-ts metadata: %v", err)
	}
	if commit != pinnedRobloxTSCommit {
		t.Fatalf("roblox-ts@%s gitHead = %s, want %s", pinnedRobloxTSVersion, commit, pinnedRobloxTSCommit)
	}
}

func verifyPinnedCompiler(t *testing.T, cli string) {
	t.Helper()
	cmd := exec.Command("node", cli, "--version")
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("run pinned roblox-ts compiler: %v\n%s", err, output)
	}
	if version := strings.TrimSpace(string(output)); version != pinnedRobloxTSVersion {
		t.Fatalf("pinned roblox-ts version = %q, want %q", version, pinnedRobloxTSVersion)
	}
}

func compatibilityFixturePath(root, fixture string) string {
	return filepath.Join(root, "testdata", "compatibility", "runtime", fixture+".spec.ts")
}
