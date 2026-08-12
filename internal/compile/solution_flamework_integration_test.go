package compile

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestSolutionFlameworkPackageBuildDataReachesDependentNativeTransform(t *testing.T) {
	// Given: a native Flamework package linked into a dependent game with stale build data.
	root := t.TempDir()
	packageDir := filepath.Join(root, "package")
	gameDir := filepath.Join(root, "game")
	writeSolutionFile(t, root, "tsconfig.json", `{"files":[],"references":[{"path":"./package"},{"path":"./game"}]}`)
	writeNativeFlameworkSolutionProject(t, packageDir, "@scope/solution-package")
	writeNativeFlameworkSolutionProject(t, gameDir, "solution-game")
	linkFlameworkFixtureDependencies(t, packageDir)
	linkFlameworkFixtureDependencies(t, gameDir)
	writeSolutionFile(t, packageDir, "src/index.ts", `/** @metadata reflect */ export class PackageService {}`)
	writeSolutionFile(t, gameDir, "src/index.ts", strings.Join([]string{
		`import { OnStart, Service } from "@flamework/core";`,
		`import { PackageService } from "@scope/solution-package";`,
		`@Service() export class GameService implements OnStart { constructor(value: PackageService) {} public onStart(): void {} }`,
	}, "\n"))
	writeSolutionFile(t, packageDir, "flamework.build", `{"version":1,"flameworkVersion":"1.3.2","identifierPrefix":"stale-prefix","identifiers":{}}`)
	packageLink := filepath.Join(gameDir, "node_modules", "@scope", "solution-package")
	if err := os.MkdirAll(filepath.Dir(packageLink), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(packageDir, packageLink); err != nil {
		t.Fatal(err)
	}

	// When: the actual solution build compiles both projects in one drain.
	result, messages, err := BuildSolutionWithOptions(filepath.Join(root, "tsconfig.json"), ProjectOptions{})
	// Then: package metadata is published before the game transform resolves its constructor ID.
	if err != nil {
		t.Fatalf("BuildSolutionWithOptions: %v (%v)", err, messages)
	}
	packageBuildData := string(mustReadFile(t, filepath.Join(packageDir, "flamework.build")))
	if !strings.Contains(packageBuildData, `"identifierPrefix": "@scope/solution-package"`) {
		t.Fatalf("package build data was not refreshed:\n%s", packageBuildData)
	}
	gameOutput := ""
	for _, output := range result.Outputs {
		if strings.Contains(output, "GameService") {
			gameOutput = output
			break
		}
	}
	if gameOutput == "" {
		t.Fatalf("solution output missing GameService: %v", result.Outputs)
	}
	if strings.Contains(gameOutput, "stale-prefix:") || !strings.Contains(gameOutput, "@scope/solution-package:") {
		t.Fatalf("dependent output did not use newly published package identifier:\n%s", gameOutput)
	}
}

func writeNativeFlameworkSolutionProject(t *testing.T, dir, packageName string) {
	t.Helper()
	writeSolutionFile(t, dir, "package.json", `{"name":"`+packageName+`","version":"1.0.0","types":"out/index.d.ts"}`)
	writeSolutionFile(t, dir, "rotor.toml", "[flamework]\n")
	config := strings.Replace(crossProjectCompilerOptions(true), `"types":[],`, "", 1)
	config = strings.Replace(config, `"typeRoots":["node_modules/@rbxts"]`, `"typeRoots":["node_modules/@rbxts","node_modules/@flamework"]`, 1)
	config = strings.Replace(config, `,"rootDir"`, `,"experimentalDecorators":true,"rootDir"`, 1) + `,"include":["src"]}`
	if packageName == "solution-game" {
		config = strings.TrimSuffix(config, "}") + `,"references":[{"path":"../package"}]}`
		writeSolutionFile(t, dir, "default.project.json", `{"name":"game","tree":{"$className":"DataModel","ReplicatedStorage":{"out":{"$path":"out"},"include":{"$path":"include","node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"},"@flamework":{"$path":"node_modules/@flamework"},"@scope":{"$path":"node_modules/@scope"}}}}}}`)
	}
	writeSolutionFile(t, dir, "tsconfig.json", config)
}

func linkFlameworkFixtureDependencies(t *testing.T, projectDir string) {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	dependencies := filepath.Join(filepath.Dir(file), "..", "..", "testdata", "transformers", "project", "node_modules")
	for _, scope := range []string{"@flamework", "@rbxts"} {
		target := filepath.Join(dependencies, scope)
		if _, err := os.Stat(target); err != nil {
			skipOrFailFixture(t, "Flamework fixture dependencies not installed: %v", err)
		}
		link := filepath.Join(projectDir, "node_modules", scope)
		if err := os.MkdirAll(filepath.Dir(link), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.Symlink(target, link); err != nil {
			t.Fatal(err)
		}
	}
}
