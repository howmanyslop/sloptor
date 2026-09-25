package conformance

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

type compatibilityCompiler int

const (
	compatibilityCompilerUpstream compatibilityCompiler = iota
	compatibilityCompilerRotorOptimized
	compatibilityCompilerRotorGeneral
)

func (environment compatibilityEnvironment) runRuntime(t *testing.T, compiler compatibilityCompiler, fixture string) {
	t.Helper()
	fixturePath := compatibilityFixturePath(environment.root, fixture)
	if _, err := os.Stat(fixturePath); err != nil {
		t.Fatalf("compatibility fixture %s: %v", fixturePath, err)
	}
	projectDir := environment.stageRuntimeProject(t, fixturePath, compiler == compatibilityCompilerRotorGeneral)

	var compileCmd *exec.Cmd
	switch compiler {
	case compatibilityCompilerUpstream:
		compileCmd = exec.Command("node", environment.upstreamCLI, "build", "--project", projectDir, "--type", "game")
	case compatibilityCompilerRotorOptimized:
		compileCmd = exec.Command("go", "run", "./cmd/rotor", "build", projectDir, "--type", "game", "--allowCommentDirectives")
	case compatibilityCompilerRotorGeneral:
		compileCmd = exec.Command("go", "run", "./cmd/rotor", "build", projectDir, "--type", "game", "--allowCommentDirectives", "--optimizedLoops=false")
	default:
		t.Fatalf("unknown compatibility compiler %d", compiler)
	}
	compileCmd.Dir = environment.root
	if output, err := compileCmd.CombinedOutput(); err != nil {
		t.Fatalf("compile %s with %s: %v\n%s", fixture, compiler, err, output)
	}

	placePath := filepath.Join(projectDir, "compatibility-tests.rbxlx")
	rojoCmd := exec.Command(environment.tools.Rojo, "build", "--output", placePath, filepath.Join(projectDir, "default.project.json"))
	rojoCmd.Dir = projectDir
	if output, err := rojoCmd.CombinedOutput(); err != nil {
		t.Fatalf("rojo build %s with %s output: %v\n%s", fixture, compiler, err, output)
	}

	runner := filepath.Join(environment.root, "reference", "roblox-ts", "tests", "runTestsWithLune.lua")
	luneCmd := exec.Command(environment.tools.Lune, "run", runner, placePath)
	luneCmd.Dir = environment.root
	if output, err := luneCmd.CombinedOutput(); err != nil {
		t.Fatalf("execute %s with %s output: %v\n%s", fixture, compiler, err, output)
	} else {
		t.Logf("%s with %s:\n%s", fixture, compiler, output)
	}
}

func (compiler compatibilityCompiler) String() string {
	switch compiler {
	case compatibilityCompilerUpstream:
		return "pinned upstream"
	case compatibilityCompilerRotorOptimized:
		return "Rotor optimized loops"
	case compatibilityCompilerRotorGeneral:
		return "Rotor general loops"
	default:
		return fmt.Sprintf("compiler(%d)", compiler)
	}
}

func (environment compatibilityEnvironment) stageRuntimeProject(t *testing.T, fixturePath string, generalLoops bool) string {
	t.Helper()
	baseProjectDir := filepath.Join(environment.root, "testdata", "conformance", "project")
	if err := ensureConformanceProjectDeps(baseProjectDir); err != nil {
		t.Fatal(err)
	}
	projectDir := t.TempDir()
	if err := copyFile(filepath.Join(baseProjectDir, "package.json"), filepath.Join(projectDir, "package.json")); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projectDir, "tsconfig.json"), []byte(compatibilityRuntimeTSConfig(generalLoops)), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projectDir, "default.project.json"), []byte(compatibilityRuntimeRojoConfig()), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := stageRuntimeTypeRoots(baseProjectDir, projectDir); err != nil {
		t.Fatal(err)
	}
	pinnedNodeModules := filepath.Dir(filepath.Dir(filepath.Dir(filepath.Dir(environment.upstreamCLI))))
	if err := copyTree(
		filepath.Join(pinnedNodeModules, "@rbxts", "types"),
		filepath.Join(projectDir, "node_modules", "@rbxts", "types"),
	); err != nil {
		t.Fatal(err)
	}
	if err := copyFile(fixturePath, filepath.Join(projectDir, "src", "tests", filepath.Base(fixturePath))); err != nil {
		t.Fatal(err)
	}
	if err := copyFile(filepath.Join(baseProjectDir, "src", "services.d.ts"), filepath.Join(projectDir, "src", "services.d.ts")); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projectDir, "src", "main.server.ts"), []byte(runtimeSuiteMainSource()), 0o644); err != nil {
		t.Fatal(err)
	}
	return projectDir
}

func compatibilityRuntimeRojoConfig() string {
	return `{
	"name": "compatibility-runtime",
	"tree": {
		"$className": "DataModel",
		"ReplicatedStorage": {
			"$className": "ReplicatedStorage",
			"include": {
				"$path": "include",
				"node_modules": {
					"$className": "Folder",
					"@rbxts": {"$path": "node_modules/@rbxts"}
				}
			}
		},
		"ServerScriptService": {
			"$className": "ServerScriptService",
			"main": {"$path": "out/main.server.luau"},
			"tests": {"$path": "out/tests"}
		}
	}
}`
}

func compatibilityRuntimeTSConfig(generalLoops bool) string {
	rbxtsOptions := ""
	if generalLoops {
		rbxtsOptions = "\n\t\"rbxts\": {\"optimizedLoops\": false},"
	}
	return fmt.Sprintf(`{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"downlevelIteration": true,
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"forceConsistentCasingInFileNames": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out",
		"baseUrl": "src"
	},%s
	"include": ["src"]
}`, rbxtsOptions)
}
