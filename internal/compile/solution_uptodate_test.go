package compile

import (
	"path/filepath"
	"strings"
	"testing"
)

// writeSpecLibSolution is the issue #61 layout: a files-empty coordinator
// references a spec project that declares its own rbxts type and Rojo config,
// and the spec references a lib project that declares neither.
func writeSpecLibSolution(t *testing.T) string {
	t.Helper()
	root := t.TempDir()
	options := strings.Replace(crossProjectCompilerOptions(true), `"composite":true`, `"composite":true,"incremental":true`, 1)
	writeSolutionFile(t, root, "package.json", `{"name":"pkg"}`)
	writeSolutionFile(t, root, "default.project.json", `{"name":"pkg","tree":{"$className":"DataModel","ReplicatedStorage":{"include":{"$path":"include"},"lib":{"$path":"out"}}}}`)
	writeSolutionFile(t, root, "test.project.json", `{"name":"pkg-test","tree":{"$className":"DataModel","ReplicatedStorage":{"include":{"$path":"include"},"lib":{"$path":"out"},"tests":{"$path":"out-test"}}}}`)
	writeSolutionFile(t, root, "tsconfig.test.json", `{"files":[],"references":[{"path":"./tsconfig.spec.json"}]}`)
	writeSolutionFile(t, root, "tsconfig.lib.json", strings.TrimSuffix(options, "}")+`,"tsBuildInfoFile":"${configDir}/out/tsconfig.lib.tsbuildinfo"},"include":["src"]}`)
	spec := strings.Replace(options, `"rootDir":"src","outDir":"out"`, `"rootDir":"tests","outDir":"out-test","tsBuildInfoFile":"out-test/tsconfig.tsbuildinfo"`, 1)
	writeSolutionFile(t, root, "tsconfig.spec.json", spec+`,"rbxts":{"rojo":"./test.project.json","type":"game"},"include":["tests"],"references":[{"path":"./tsconfig.lib.json"}]}`)
	writeSolutionFile(t, root, "include/RuntimeLib.lua", "return {}\n")
	writeSolutionFile(t, root, "src/globals.d.ts", noLibGlobalStubs)
	writeSolutionFile(t, root, "src/value.ts", "export const value = 1;\n")
	writeSolutionFile(t, root, "tests/globals.d.ts", noLibGlobalStubs)
	writeSolutionFile(t, root, "tests/value.spec.ts", "import { value } from \"../src/value\";\nexport const doubled = value * 2;\n")
	return root
}

func TestSolutionBuildReusesDirectReferencedProjectBuild(t *testing.T) {
	// Given: the lib project was built on its own.
	root := writeSpecLibSolution(t)
	if _, messages, err := BuildProjectWithOptions(root, ProjectOptions{TsConfigPath: filepath.Join(root, "tsconfig.lib.json")}); err != nil {
		t.Fatalf("lib build: %v (%v)", err, messages)
	}

	// When: the solution that references it builds.
	result, messages, err := BuildSolutionWithOptions(filepath.Join(root, "tsconfig.test.json"), ProjectOptions{})
	if err != nil {
		t.Fatalf("solution build: %v (%v)", err, messages)
	}

	// Then: only the spec project compiles; the lib is already up to date.
	for path := range result.Outputs {
		if !strings.Contains(filepath.ToSlash(path), "/out-test/") {
			t.Errorf("solution build recompiled up-to-date lib output %s", path)
		}
	}
}

func TestSolutionGraphLayersReferencedRbxtsOptions(t *testing.T) {
	// Given: a coordinator and a reference that each declare rbxts options,
	// and a command line that sets one of them.
	root := t.TempDir()
	writeSolutionFile(t, root, "tsconfig.json", `{"files":[],"references":[{"path":"./lib"}],"rbxts":{"optimizedLoops":false,"luau":false,"noInclude":true}}`)
	writeSolutionFile(t, root, "lib/tsconfig.json", `{"rbxts":{"luau":true,"optimizedLoops":true}}`)
	optimizedLoops := false
	entry := ProjectOptions{LuaExtension: true, NoOptimizedLoops: true, SolutionArgv: &RbxtsOptions{OptimizedLoops: &optimizedLoops}}

	// When
	graph, err := BuildSolutionGraph(filepath.Join(root, "tsconfig.json"), entry)
	if err != nil {
		t.Fatal(err)
	}

	// Then: defaults < coordinator rbxts < own rbxts < argv.
	lib := graph.Projects[0].Options
	if lib.LuaExtension {
		t.Error("reference's luau did not override the coordinator's")
	}
	if !lib.NoOptimizedLoops {
		t.Error("command-line optimizedLoops did not override the reference's")
	}
	if lib.EmitIncludeFiles {
		t.Error("coordinator's noInclude did not reach the reference")
	}
}
