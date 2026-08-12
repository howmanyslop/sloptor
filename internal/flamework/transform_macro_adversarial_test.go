package flamework

import (
	"context"
	"errors"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/printer"
)

func TestBuildPathIntrinsics_registersGlobAndEmitsRojoPath_whenLiteralPathsAreValid(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	writeTransformFixture(t, directory, "tsconfig.json", `{"compilerOptions":{"rootDir":"src","outDir":"out","strict":true},"files":["src/main.ts","src/assets/entry.ts"]}`)
	writeTransformFixture(t, directory, "default.project.json", `{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"}}}}`)
	writeTransformFixture(t, directory, "src/assets/entry.ts", "export {};\n")
	writeTransformFixture(t, directory, "src/main.ts", "type Path = \"src/assets/entry.ts\";\ntype Glob = \"./assets/*.ts\";\n")
	program := newTransformProgram(t, directory)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	file := program.GetSourceFile(filepath.ToSlash(filepath.Join(directory, "src/main.ts")))
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	state, err := newTransformState(TransformInput{Program: program, Checker: checker, Files: []*ast.SourceFile{file}, Project: project}, nil)
	if err != nil {
		t.Fatalf("newTransformState() error = %v", err)
	}
	pathType := state.checker.GetTypeFromTypeNode(file.Statements.Nodes[0].AsTypeAliasDeclaration().Type)
	globType := state.checker.GetTypeFromTypeNode(file.Statements.Nodes[1].AsTypeAliasDeclaration().Type)

	// When
	path, err := buildPathIntrinsic(state, file.AsNode(), pathType)
	if err != nil {
		t.Fatalf("buildPathIntrinsic() error = %v", err)
	}
	glob, err := buildPathGlobIntrinsic(state, file.AsNode(), globType)
	if err != nil {
		t.Fatalf("buildPathGlobIntrinsic() error = %v", err)
	}
	_, globsJSON, err := project.RuntimeArtifacts()
	if err != nil {
		t.Fatalf("RuntimeArtifacts() error = %v", err)
	}

	// Then
	printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil)
	pathText := printed.Emit(path, file)
	if !strings.Contains(pathText, `"ReplicatedStorage"`) || !strings.Contains(pathText, `"entry"`) {
		t.Fatalf("path intrinsic Rojo output = %q", pathText)
	}
	if got := printed.Emit(glob, file); got != `"src/assets/*.ts"` {
		t.Fatalf("pathglob intrinsic output = %q, want project-relative assets glob", got)
	}
	if got := string(globsJSON); got != `{"game":{"src/assets/*.ts":[["ReplicatedStorage","TS","assets","entry"]]},"packages":{}}` {
		t.Fatalf("Rojo glob artifact = %s", got)
	}
	t.Logf("Rojo TS path: %s", pathText)
	t.Logf("Rojo globs JSON: %s", globsJSON)
	snapshot := project.BuildInfoSnapshot()
	if snapshot.Metadata == nil || snapshot.Metadata.Globs == nil || snapshot.Metadata.Globs.Origins == nil {
		t.Fatalf("glob registration = %#v", snapshot.Metadata)
	}
	originGlobs := (*snapshot.Metadata.Globs.Origins)["src/main.ts"]
	if len(originGlobs) != 1 || originGlobs[0] != "src/assets/*.ts" {
		t.Fatalf("origin globs = %#v, want project-relative assets glob", originGlobs)
	}
}

func TestBuildPathIntrinsics_returnsMacroErrors_whenPathIsNotLiteral(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{}, "type Dynamic = string;")
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	target := firstTypeAliasType(state, file)

	// When
	_, pathErr := buildPathIntrinsic(state, file.AsNode(), target)
	_, globErr := buildPathGlobIntrinsic(state, file.AsNode(), target)

	// Then
	if !errors.Is(pathErr, ErrInvalidMacro) || !errors.Is(globErr, ErrInvalidMacro) {
		t.Fatalf("path error = %v, glob error = %v, want ErrInvalidMacro", pathErr, globErr)
	}
}

func TestInlineMacroIntrinsic_selectsLinkedParameterAndPreservesReturnCast(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{}, strings.Join([]string{
		`/** @metadata {@link value intrinsic-inline} */`,
		`declare function select(discarded: string, value: number): boolean;`,
		`select("discarded", 42);`,
	}, "\n"))
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	calls := collectCallExpressions(file)
	if len(calls) != 1 {
		t.Fatalf("call count = %d, want 1", len(calls))
	}

	// When
	result, err := transformFlameworkCall(state, calls[0], MacroRuntime{})
	// Then
	if err != nil {
		t.Fatalf("transformFlameworkCall() error = %v", err)
	}
	got := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, nil).Emit(result.Expression, file)
	if got != "42 as boolean" {
		t.Fatalf("inline expression = %q, want selected argument cast to boolean", got)
	}
}

func TestShuffledIndexes_returnsMacroError_whenInjectedRandomIndexIsOutsideRange(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{Obfuscation: true}, "export {};")
	runtime := MacroRuntime{RandomIndex: func(int) (int, error) { return 4, nil }}

	// When
	_, err := shuffledIndexes(state, 4, runtime)

	// Then
	if !errors.Is(err, ErrInvalidMacro) || !strings.Contains(err.Error(), "outside [0, 4)") {
		t.Fatalf("shuffledIndexes() error = %v, want bounded ErrInvalidMacro", err)
	}
}

func TestGuardMacros_returnGuardBuilderAbsent_whenNoGuardRuntimeIsConfigured(t *testing.T) {
	// Given
	state := newMacroTestState(t, config.FlameworkConfig{}, "type Input = [name: string, ...values: number[]];")
	file := state.program.GetSourceFile(filepath.ToSlash(filepath.Join(state.project.projectDirectory, "src/main.ts")))
	target := firstTypeAliasType(state, file)

	// When
	_, _, genericErr := buildGenericMacro(state, file.AsNode(), userMacro{kind: macroGeneric, target: target, metadata: "guard"}, MacroRuntime{})
	_, _, tupleErr := buildIntrinsicMacro(state, file.AsNode(), userMacro{kind: macroIntrinsic, intrinsic: "tuple-guards", inputs: []*checker.Type{target}}, MacroRuntime{})

	// Then
	if !errors.Is(genericErr, ErrGuardBuilderAbsent) || !errors.Is(tupleErr, ErrGuardBuilderAbsent) {
		t.Fatalf("generic error = %v, tuple error = %v, want ErrGuardBuilderAbsent", genericErr, tupleErr)
	}
}

func TestProjectPersistence_preservesObfuscationHash_afterReopen(t *testing.T) {
	// Given
	directory := t.TempDir()
	writeTransformFixture(t, directory, "package.json", `{"name":"fixture-game","version":"1.0.0"}`)
	project, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{Obfuscation: true}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}

	// When
	hash, err := project.HashString("submitted", "remotes")
	if err != nil {
		t.Fatalf("HashString() error = %v", err)
	}
	if err := project.Persist(); err != nil {
		t.Fatalf("Persist() error = %v", err)
	}
	reopened, err := OpenProject(ProjectOptions{ProjectDir: directory, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{Obfuscation: true}})
	if err != nil {
		t.Fatalf("reopen OpenProject() error = %v", err)
	}
	reopenedHash, err := reopened.HashString("submitted", "remotes")
	if err != nil {
		t.Fatalf("reopened HashString() error = %v", err)
	}

	// Then
	if !regexp.MustCompile(`^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$`).MatchString(hash) {
		t.Fatalf("obfuscation hash = %q, want UUID v4", hash)
	}
	if reopenedHash != hash || reopened.BuildInfoSnapshot().StringHashes["remotes:submitted"] != hash {
		t.Fatalf("reopened obfuscation hashes = %q, %#v, want %q", reopenedHash, reopened.BuildInfoSnapshot().StringHashes, hash)
	}
}
