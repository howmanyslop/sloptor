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

func TestSolutionWatchCommentEditSkipsDependentCompile(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, nil)
	f.project("game", []string{"shared"}, nil)
	f.write("game/src/index.ts", `import { value } from "../../shared/src"; export const result = value;`)
	coordinator := f.coordinator("game")

	for _, text := range []string{
		"export const value = 1;\n// comment only\n",
		"export const value = 1;\n",
	} {
		f.write("shared/src/index.ts", text)
		coordinator.Invalidate(filepath.Join(f.dir, "shared/tsconfig.json"))
		result, messages, err := coordinator.Drain()
		if err != nil {
			t.Fatalf("watch drain: %v (%v)", err, messages)
		}
		if len(result.Outputs) != 1 {
			t.Fatalf("comment edit compiled %d files, want only the changed source: %v", len(result.Outputs), result.Outputs)
		}
		for path := range result.Outputs {
			if !strings.Contains(filepath.ToSlash(path), "/shared/") {
				t.Fatalf("comment edit recompiled dependent output %s", path)
			}
		}
	}
}

func TestSolutionWatchDeclarationChangeRecompilesDependent(t *testing.T) {
	f := newReferenceFixture(t)
	f.project("shared", nil, nil)
	f.project("game", []string{"shared"}, nil)
	f.write("shared/src/index.ts", "export const enum Value { Current = 1 }\n")
	f.write("game/src/index.ts", `import { Value } from "../../shared/src"; export const result = Value.Current;`)
	coordinator := f.coordinator("game")

	f.write("shared/src/index.ts", "export const enum Value { Current = 2 }\n")
	coordinator.Invalidate(filepath.Join(f.dir, "shared/tsconfig.json"))
	result, messages, err := coordinator.Drain()
	if err != nil {
		t.Fatalf("watch drain: %v (%v)", err, messages)
	}
	output := result.Outputs[filepath.Join(f.dir, "game/out/init.luau")]
	if !strings.Contains(output, "local result = 2") {
		t.Fatalf("dependent did not compile against the new declaration:\n%s", output)
	}
}

func TestSolutionGraphLayersReferencedRbxtsOptions(t *testing.T) {
	// Given: a coordinator and a reference that each declare rbxts options,
	// and a command line that sets one of them.
	root := t.TempDir()
	writeSolutionFile(t, root, "tsconfig.json", `{"files":[],"references":[{"path":"./lib"}],"rbxts":{"optimizedArrayAppends":true,"optimizedLoops":false,"luau":false,"noInclude":true}}`)
	writeSolutionFile(t, root, "lib/tsconfig.json", `{"rbxts":{"luau":true,"optimizedArrayAppends":false,"optimizedLoops":true}}`)
	optimizedArrayAppends := true
	optimizedLoops := false
	entry := ProjectOptions{LuaExtension: true, NoOptimizedLoops: true, SolutionArgv: &RbxtsOptions{OptimizedArrayAppends: &optimizedArrayAppends, OptimizedLoops: &optimizedLoops}}

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
	if !lib.OptimizedArrayAppends {
		t.Error("command-line optimizedArrayAppends did not override the reference's")
	}
	if lib.EmitIncludeFiles {
		t.Error("coordinator's noInclude did not reach the reference")
	}
}
