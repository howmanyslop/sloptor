package conformance

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

type compatibilityPackageBuild struct {
	compiler    compatibilityCompiler
	projectDir  string
	projectType string
}

func TestPinnedPackageInterop(t *testing.T) {
	environment := requireCompatibilityEnvironment(t)
	upstreamLibrary := environment.compilePackageLibrary(t, compatibilityCompilerUpstream)
	rotorLibrary := environment.compilePackageLibrary(t, compatibilityCompilerRotorOptimized)

	t.Run("Rotor-caller-upstream-library", func(t *testing.T) {
		consumer := environment.compilePackageConsumer(t, compatibilityCompilerRotorOptimized, upstreamLibrary)
		environment.runPackageConsumer(t, consumer)
	})
	t.Run("upstream-caller-Rotor-library", func(t *testing.T) {
		consumer := environment.compilePackageConsumer(t, compatibilityCompilerUpstream, rotorLibrary)
		environment.runPackageConsumer(t, consumer)
	})
}

func (environment compatibilityEnvironment) compilePackageLibrary(t *testing.T, compiler compatibilityCompiler) string {
	t.Helper()
	projectDir := t.TempDir()
	fixtureDir := filepath.Join(environment.root, "testdata", "compatibility", "packages", "library")
	if err := copyFile(filepath.Join(fixtureDir, "package.json"), filepath.Join(projectDir, "package.json")); err != nil {
		t.Fatal(err)
	}
	if err := copyTree(filepath.Join(fixtureDir, "src"), filepath.Join(projectDir, "src")); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projectDir, "tsconfig.json"), []byte(compatibilityPackageTSConfig(true)), 0o644); err != nil {
		t.Fatal(err)
	}
	environment.stagePackageTypeRoots(t, projectDir)
	environment.compilePackageProject(t, compatibilityPackageBuild{
		compiler:    compiler,
		projectDir:  projectDir,
		projectType: "package",
	})
	for _, rel := range []string{"out/init.luau", "out/index.d.ts"} {
		if _, err := os.Stat(filepath.Join(projectDir, filepath.FromSlash(rel))); err != nil {
			t.Fatalf("%s did not emit package artifact %s: %v", compiler, rel, err)
		}
	}
	return projectDir
}

func (environment compatibilityEnvironment) compilePackageConsumer(t *testing.T, compiler compatibilityCompiler, libraryProject string) string {
	t.Helper()
	projectDir := t.TempDir()
	fixtureDir := filepath.Join(environment.root, "testdata", "compatibility", "packages", "consumer")
	if err := copyFile(filepath.Join(fixtureDir, "package.json"), filepath.Join(projectDir, "package.json")); err != nil {
		t.Fatal(err)
	}
	if err := copyFile(filepath.Join(fixtureDir, "default.project.json"), filepath.Join(projectDir, "default.project.json")); err != nil {
		t.Fatal(err)
	}
	if err := copyTree(filepath.Join(fixtureDir, "src"), filepath.Join(projectDir, "src")); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projectDir, "tsconfig.json"), []byte(compatibilityPackageTSConfig(false)), 0o644); err != nil {
		t.Fatal(err)
	}
	environment.stagePackageTypeRoots(t, projectDir)
	libraryDir := filepath.Join(projectDir, "node_modules", "@rbxts", "compat-library")
	if err := copyFile(filepath.Join(libraryProject, "package.json"), filepath.Join(libraryDir, "package.json")); err != nil {
		t.Fatal(err)
	}
	if err := copyTree(filepath.Join(libraryProject, "out"), filepath.Join(libraryDir, "out")); err != nil {
		t.Fatal(err)
	}
	environment.compilePackageProject(t, compatibilityPackageBuild{
		compiler:    compiler,
		projectDir:  projectDir,
		projectType: "game",
	})
	return projectDir
}

func (environment compatibilityEnvironment) stagePackageTypeRoots(t *testing.T, projectDir string) {
	t.Helper()
	baseProjectDir := filepath.Join(environment.root, "testdata", "conformance", "project")
	if err := ensureConformanceProjectDeps(baseProjectDir); err != nil {
		t.Fatal(err)
	}
	baseTypes := filepath.Join(baseProjectDir, "node_modules", "@rbxts")
	if err := copyTree(baseTypes, filepath.Join(projectDir, "node_modules", "@rbxts")); err != nil {
		t.Fatal(err)
	}
	pinnedNodeModules := filepath.Dir(filepath.Dir(filepath.Dir(filepath.Dir(environment.upstreamCLI))))
	if err := copyTree(
		filepath.Join(pinnedNodeModules, "@rbxts", "types"),
		filepath.Join(projectDir, "node_modules", "@rbxts", "types"),
	); err != nil {
		t.Fatal(err)
	}
}

func (environment compatibilityEnvironment) compilePackageProject(t *testing.T, build compatibilityPackageBuild) {
	t.Helper()
	var cmd *exec.Cmd
	if build.compiler == compatibilityCompilerUpstream {
		cmd = exec.Command("node", environment.upstreamCLI, "build", "--project", build.projectDir, "--type", build.projectType)
	} else {
		cmd = exec.Command("go", "run", "./cmd/rotor", "build", build.projectDir, "--type", build.projectType, "--allowCommentDirectives")
	}
	cmd.Dir = environment.root
	if output, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("compile package fixture with %s: %v\n%s", build.compiler, err, output)
	}
}

func (environment compatibilityEnvironment) runPackageConsumer(t *testing.T, projectDir string) {
	t.Helper()
	placePath := filepath.Join(projectDir, "package-interop.rbxlx")
	rojoCmd := exec.Command(environment.tools.Rojo, "build", "--output", placePath, filepath.Join(projectDir, "default.project.json"))
	if output, err := rojoCmd.CombinedOutput(); err != nil {
		t.Fatalf("rojo build package interop: %v\n%s", err, output)
	}
	runner := filepath.Join(environment.root, "reference", "roblox-ts", "tests", "runTestsWithLune.lua")
	luneCmd := exec.Command(environment.tools.Lune, "run", runner, placePath)
	if output, err := luneCmd.CombinedOutput(); err != nil {
		mainSource, _ := os.ReadFile(filepath.Join(projectDir, "out", "main.server.luau"))
		t.Fatalf("execute package interop: %v\n%s\n--- caller ---\n%s", err, output, mainSource)
	}
}

func compatibilityPackageTSConfig(declaration bool) string {
	declarationOptions := ""
	if declaration {
		declarationOptions = "\n\t\t\"declaration\": true,"
	}
	return fmt.Sprintf(`{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,%s
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"forceConsistentCasingInFileNames": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out"
	},
	"include": ["src"]
}`, declarationOptions)
}
