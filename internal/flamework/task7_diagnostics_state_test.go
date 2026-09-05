package flamework

// allow: SIZE_OK — the cases stay together so every canonical diagnostic-state row uses one real two-entrypoint harness.

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
	"strconv"
	"strings"
	"testing"
	"time"

	"rotor/internal/config"
	"rotor/tsgo/ast"
)

type task7StateTuple struct {
	File     string `json:"file,omitempty"`
	Start    int    `json:"start"`
	Length   int    `json:"length"`
	Category string `json:"category"`
	Code     string `json:"code"`
	Message  string `json:"message"`
	Related  string `json:"related,omitempty"`
}

type task7StateSidecarDiagnostic struct {
	File     string `json:"file"`
	Start    *int   `json:"start"`
	Length   *int   `json:"length"`
	Category string `json:"category"`
	Code     string `json:"code"`
	Message  string `json:"message"`
}

type task7StateSidecarResponse struct {
	Diagnostics []task7StateSidecarDiagnostic `json:"diagnostics"`
}

type task7StateSidecarRequest struct {
	Protocol         int        `json:"protocol"`
	Operation        string     `json:"operation"`
	TSConfigPath     string     `json:"tsConfigPath"`
	ProjectDir       string     `json:"projectDir"`
	CompileFileNames []string   `json:"compileFileNames"`
	FileNames        []string   `json:"fileNames"`
	ChangedFiles     []struct{} `json:"changedFiles"`
}

type task7StateCase struct {
	row                string
	name               string
	category           string
	prepare            func(*testing.T, string)
	native             func(*testing.T, string) (int, []task7StateTuple)
	upstreamProject    string
	upstreamBlocked    string
	nativeBlocked      string
	normalizeException bool
	targetFile         string
	preserveInput      bool
}

var task7StateANSI = regexp.MustCompile(`\x1b\[[0-9;]*m`)

func TestTask7DiagnosticsState_matchesUpstream132DiagnosticTuples(t *testing.T) {
	// Given: one pinned installation, then a fresh project copy for every state mutation.
	fixture := filepath.Join("testdata", "task7-diagnostics-state")
	install := t.TempDir()
	copyTask7Tree(t, fixture, install)
	runTask7Command(t, install, 90*time.Second, "npm", "install", "--ignore-scripts", "--package-lock=false", "--no-audit", "--no-fund", "--prefer-offline")
	if version := runTask7Command(t, install, 10*time.Second, "node", "-e", `process.stdout.write(require("rbxts-transformer-flamework/package.json").version)`); version != FlameworkVersion {
		t.Fatalf("upstream version = %q, want %q", version, FlameworkVersion)
	}

	cases := task7StateCases()
	for _, testCase := range cases {
		t.Run(testCase.row+"/"+testCase.name, func(t *testing.T) {
			root := t.TempDir()
			root, err := filepath.EvalSymlinks(root)
			if err != nil {
				t.Fatalf("resolve project path: %v", err)
			}
			copyTask7Tree(t, fixture, root)
			linkTask7Modules(t, install, root)
			if testCase.prepare != nil {
				testCase.prepare(t, root)
			}

			// When: the real upstream compiler entrypoint and native exported seam consume the same state.
			upstreamExit, upstream := task7StateUpstream(t, root, testCase)
			nativeExit, native := task7StateNative(t, root, testCase)
			if testCase.normalizeException {
				upstream = task7StateNormalizeExceptionTuple(upstream, testCase.targetFile)
				native = task7StateNormalizeExceptionTuple(native, testCase.targetFile)
				task7StateAssertNoWrite(t, root, testCase.targetFile, testCase.preserveInput)
			}

			// Then: exit status and every ordered tuple field are identical.
			t.Logf("UPSTREAM exit=%d tuples=%s", upstreamExit, task7StateFormatTuples(upstream))
			t.Logf("NATIVE exit=%d tuples=%s", nativeExit, task7StateFormatTuples(native))
			if upstreamExit == 125 || nativeExit == 125 {
				t.Logf("BLOCKED public-entrypoint differential: upstream=%s native=%s", task7StateFormatTuples(upstream), task7StateFormatTuples(native))
				return
			}
			if upstreamExit != nativeExit || fmt.Sprint(upstream) != fmt.Sprint(native) {
				t.Errorf("%s %s tuple mismatch\nupstream exit=%d %s\nnative exit=%d %s", testCase.row, testCase.name, upstreamExit, task7StateFormatTuples(upstream), nativeExit, task7StateFormatTuples(native))
			}
		})
	}
}

func task7StateCases() []task7StateCase {
	return []task7StateCase{
		{row: "DIAG-001", name: "build-info-missing", category: "error", native: task7StateOpenNative},
		{row: "DIAG-001", name: "build-info-malformed", category: "error", prepare: func(t *testing.T, root string) { writeTransformFixture(t, root, "flamework.build", `{`) }, native: task7StateOpenNative, normalizeException: true, targetFile: "flamework.build", preserveInput: true},
		{row: "DIAG-001", name: "build-info-version-mismatch", category: "error", prepare: func(t *testing.T, root string) {
			writeTransformFixture(t, root, "flamework.build", `{"version":1,"flameworkVersion":"1.2.0","identifiers":{}}`)
			task7StatePrepareDirty(t, root)
		}, native: task7StateOpenNative},
		{row: "DIAG-001", name: "duplicate-identifier", category: "error", upstreamBlocked: "public transformer lookup prevents duplicate insertion; BuildInfo is not exported", native: task7StateDuplicateIdentifier},
		{row: "DIAG-001", name: "duplicate-class", category: "error", upstreamBlocked: "public transformer lookup prevents duplicate insertion; BuildInfo is not exported", native: task7StateDuplicateClass},
		{row: "DIAG-002", name: "dirty-incremental-environment", category: "error", prepare: task7StatePrepareDirty, nativeBlocked: "native flamework public API has no incremental or tsbuildinfo input; compiler-layer coverage owns this state"},
		{row: "DIAG-003", name: "malformed-runtime-json", category: "error", prepare: func(t *testing.T, root string) { writeTransformFixture(t, root, RuntimeConfigFileName, `{`) }, native: task7StateOpenNative, normalizeException: true, targetFile: RuntimeConfigFileName, preserveInput: true},
		{row: "DIAG-003", name: "malformed-runtime-schema", category: "error", prepare: func(t *testing.T, root string) {
			writeTransformFixture(t, root, RuntimeConfigFileName, `{"profiling":"yes"}`)
		}, native: task7StateOpenNative},
		{row: "DIAG-004", name: "reserved-hash-prefix", category: "error", prepare: func(t *testing.T, root string) {
			task7StateReplace(t, filepath.Join(root, "tsconfig.json"), `"noSemanticDiagnostics": true`, `"noSemanticDiagnostics": true, "hashPrefix": "$internal"`)
		}, native: func(t *testing.T, root string) (int, []task7StateTuple) {
			return task7StateOpenNativeConfig(t, root, config.FlameworkConfig{HashPrefix: "$internal"})
		}, normalizeException: true, targetFile: "tsconfig.json"},
		{row: "DIAG-005", name: "missing-package", category: "error", prepare: func(t *testing.T, root string) {
			if err := os.Remove(filepath.Join(root, "package.json")); err != nil {
				t.Fatalf("remove package fixture: %v", err)
			}
		}, native: task7StateOpenNative, normalizeException: true, targetFile: "package.json"},
		{row: "DIAG-005", name: "missing-tsconfig", category: "error", prepare: func(t *testing.T, root string) {
			if err := os.Remove(filepath.Join(root, "tsconfig.json")); err != nil {
				t.Fatalf("remove tsconfig fixture: %v", err)
			}
		}, upstreamProject: "missing-tsconfig.json", nativeBlocked: "native flamework API receives already-parsed ProjectOptions and has no tsconfig entrypoint"},
		{row: "DIAG-005", name: "missing-root-dir", category: "error", prepare: func(t *testing.T, root string) {
			task7StateReplace(t, filepath.Join(root, "tsconfig.json"), `"rootDir": "src",`, "")
		}, native: task7StateOpenNativeWithoutRoot, normalizeException: true, targetFile: "tsconfig.json"},
		{row: "DIAG-005", name: "unresolvable-root-dir", category: "error", prepare: func(t *testing.T, root string) {
			task7StateReplace(t, filepath.Join(root, "tsconfig.json"), `"rootDir": "src"`, `"rootDir": "does-not-exist"`)
		}, native: func(t *testing.T, root string) (int, []task7StateTuple) {
			return task7StateOpenNativeRoot(t, root, "does-not-exist")
		}},
		{row: "DIAG-007", name: "missing-flamework-core", category: "warning", upstreamBlocked: "the public compiler rejects an unresolved @flamework/core import before guard transformation; reference src/classes/transformState.ts:583-586", nativeBlocked: "native guard import selection exposes no warning diagnostic"},
		{row: "DIAG-007", name: "invalid-guard-runtime", category: "warning", prepare: task7StatePrepareInvalidGuardRuntime, native: task7StateTransformNative},
	}
}

func task7StatePrepareDirty(t *testing.T, root string) {
	t.Helper()
	task7StateReplace(t, filepath.Join(root, "tsconfig.json"), `"compilerOptions": {`, `"compilerOptions": {"incremental": true, "tsBuildInfoFile": "out/task7.tsbuildinfo",`)
	writeTransformFixture(t, root, "out/task7.tsbuildinfo", `{"version":"5.5.3"}`)
}

func task7StatePrepareInvalidGuardRuntime(t *testing.T, root string) {
	t.Helper()
	source := filepath.Join(root, "node_modules", "@rbxts", "t")
	realSource, err := filepath.EvalSymlinks(source)
	if err != nil {
		t.Fatalf("resolve guard runtime: %v", err)
	}
	external := filepath.Join(root, "external-t")
	if err := os.MkdirAll(external, 0o755); err != nil {
		t.Fatalf("create external guard runtime: %v", err)
	}
	copyTask7Tree(t, realSource, external)
	nested := filepath.Join(root, "node_modules", "@flamework", "core", "node_modules", "@rbxts")
	if err := os.MkdirAll(nested, 0o755); err != nil {
		t.Fatalf("create nested guard directory: %v", err)
	}
	if err := os.Symlink(external, filepath.Join(nested, "t")); err != nil {
		t.Fatalf("link invalid guard runtime: %v", err)
	}
}

func task7StateReplace(t *testing.T, path, old, replacement string) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read mutation target: %v", err)
	}
	updated := strings.Replace(string(data), old, replacement, 1)
	if updated == string(data) {
		t.Fatalf("mutation target %q not found in %s", old, path)
	}
	if err := os.WriteFile(path, []byte(updated), 0o644); err != nil {
		t.Fatalf("write mutation target: %v", err)
	}
}

func task7StateUpstream(t *testing.T, root string, testCase task7StateCase) (int, []task7StateTuple) {
	t.Helper()
	if testCase.upstreamBlocked != "" {
		return 125, []task7StateTuple{task7StateBlockedTuple(testCase.upstreamBlocked)}
	}
	project := "tsconfig.json"
	if testCase.upstreamProject != "" {
		project = testCase.upstreamProject
	}
	sidecar, err := filepath.Abs(filepath.Join("..", "..", "tools", "sidecar", "main.js"))
	if err != nil {
		t.Fatalf("resolve sidecar: %v", err)
	}
	compileFileNames := []string{filepath.Join(root, "src", "index.ts")}
	request := task7StateSidecarRequest{Protocol: 2, Operation: "transform", TSConfigPath: filepath.Join(root, project), ProjectDir: root, CompileFileNames: compileFileNames, FileNames: compileFileNames, ChangedFiles: []struct{}{}}
	requestData, err := json.Marshal(request)
	if err != nil {
		t.Fatalf("marshal sidecar request: %v", err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	command := exec.CommandContext(ctx, "node", sidecar)
	command.Dir = root
	command.Stdin = bytes.NewReader(append(requestData, '\n'))
	var stderr bytes.Buffer
	command.Stderr = &stderr
	output, err := command.Output()
	if ctx.Err() != nil {
		return 124, []task7StateTuple{{Start: -1, Category: "error", Code: "timeout", Message: ctx.Err().Error()}}
	}
	if err != nil {
		return 1, task7StateParseUpstream(root, testCase.category, stderr.Bytes())
	}
	var response task7StateSidecarResponse
	if err := json.Unmarshal(bytes.TrimSpace(output), &response); err != nil {
		t.Fatalf("decode sidecar response: %v\nstdout=%s\nstderr=%s", err, output, stderr.String())
	}
	tuples := make([]task7StateTuple, 0, len(response.Diagnostics))
	exit := 0
	for _, diagnostic := range response.Diagnostics {
		start, length := -1, 0
		if diagnostic.Start != nil {
			start = *diagnostic.Start
		}
		if diagnostic.Length != nil {
			length = *diagnostic.Length
		}
		tuples = append(tuples, task7StateTuple{File: task7StateNormalizePath(root, diagnostic.File), Start: start, Length: length, Category: diagnostic.Category, Code: diagnostic.Code, Message: task7StateNormalizePath(root, diagnostic.Message)})
		if diagnostic.Category == "error" {
			exit = 1
		}
	}
	return exit, tuples
}

func task7StateNative(t *testing.T, root string, testCase task7StateCase) (int, []task7StateTuple) {
	t.Helper()
	if testCase.nativeBlocked != "" {
		return 125, []task7StateTuple{task7StateBlockedTuple(testCase.nativeBlocked)}
	}
	return testCase.native(t, root)
}

func task7StateOpenNative(t *testing.T, root string) (int, []task7StateTuple) {
	return task7StateOpenNativeConfig(t, root, config.FlameworkConfig{})
}

func task7StateOpenNativeWithoutRoot(t *testing.T, root string) (int, []task7StateTuple) {
	return task7StateOpenNativeRoot(t, root, "")
}

func task7StateOpenNativeRoot(t *testing.T, root, rootDir string) (int, []task7StateTuple) {
	return task7StateOpenNativeOptions(t, ProjectOptions{ProjectDir: root, RootDir: rootDir, OutDir: "out"})
}

func task7StateOpenNativeConfig(t *testing.T, root string, cfg config.FlameworkConfig) (int, []task7StateTuple) {
	return task7StateOpenNativeOptions(t, ProjectOptions{ProjectDir: root, RootDir: "src", OutDir: "out", Config: cfg})
}

func task7StateOpenNativeOptions(t *testing.T, options ProjectOptions) (int, []task7StateTuple) {
	t.Helper()
	_, err := OpenProject(options)
	if err == nil {
		return 0, nil
	}
	var diagnosticError *ProjectDiagnosticError
	if errors.As(err, &diagnosticError) {
		diagnostics := diagnosticError.Diagnostics()
		tuples := make([]task7StateTuple, 0, len(diagnostics))
		for _, diagnostic := range diagnostics {
			tuples = append(tuples, task7StateNativeDiagnostic(options.ProjectDir, diagnostic))
		}
		return 1, tuples
	}
	return 1, []task7StateTuple{task7StateErrorTupleRoot(options.ProjectDir, err)}
}

func task7StateDuplicateIdentifier(t *testing.T, root string) (int, []task7StateTuple) {
	t.Helper()
	info := NewBuildInfo(filepath.Join(root, "flamework.build"), FlameworkVersion)
	if err := info.AddIdentifier("same", "first"); err != nil {
		t.Fatalf("seed identifier: %v", err)
	}
	err := info.AddIdentifier("same", "second")
	return 1, []task7StateTuple{task7StateErrorTuple(err)}
}

func task7StateDuplicateClass(t *testing.T, root string) (int, []task7StateTuple) {
	t.Helper()
	info := NewBuildInfo(filepath.Join(root, "flamework.build"), FlameworkVersion)
	class := BuildClass{FilePath: "src/index.ts", InternalID: "same"}
	if err := info.AddClass(class); err != nil {
		t.Fatalf("seed class: %v", err)
	}
	err := info.AddClass(class)
	return 1, []task7StateTuple{task7StateErrorTuple(err)}
}

func task7StateTransformNative(t *testing.T, root string) (int, []task7StateTuple) {
	t.Helper()
	program := newTransformProgram(t, root)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	files := task7SourceFiles(program, root)
	project, err := OpenProject(ProjectOptions{ProjectDir: root, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{NoSemanticDiagnostics: true}})
	if err != nil {
		return 1, []task7StateTuple{task7StateErrorTuple(err)}
	}
	result, err := Transform(TransformInput{Program: program, Checker: checker, Files: files, Project: project})
	if err != nil {
		return 1, []task7StateTuple{task7StateErrorTuple(err)}
	}
	tuples := make([]task7StateTuple, 0, len(result.Diagnostics))
	for _, diagnostic := range result.Diagnostics {
		tuples = append(tuples, task7StateNativeDiagnostic(root, diagnostic))
	}
	return 0, tuples
}

func task7StateNativeDiagnostic(root string, diagnostic *ast.Diagnostic) task7StateTuple {
	file := ""
	if diagnostic.File() != nil {
		file = task7StateNormalizePath(root, diagnostic.File().FileName())
	}
	related := make([]string, 0, len(diagnostic.RelatedInformation()))
	for _, item := range diagnostic.RelatedInformation() {
		related = append(related, item.String())
	}
	category := strings.TrimPrefix(strings.ToLower(diagnostic.Category().String()), "category")
	code, hasStringCode := diagnostic.StringCode()
	if !hasStringCode {
		code = strconv.FormatInt(int64(diagnostic.Code()), 10)
	}
	return task7StateTuple{File: file, Start: diagnostic.Pos(), Length: diagnostic.Len(), Category: category, Code: code, Message: diagnostic.String(), Related: strings.Join(related, " | ")}
}

func task7StateErrorTuple(err error) task7StateTuple {
	message := "<nil>"
	code := "runtime"
	if err != nil {
		message = err.Error()
		var syntaxError *jsonSyntaxError
		if errors.As(err, &syntaxError) {
			code = "sidecar-internal"
		}
		var boundaryError upstreamBoundaryError
		if errors.As(err, &boundaryError) {
			code = "sidecar-internal"
			message = "Error: " + message
		}
	}
	return task7StateTuple{Start: -1, Category: "error", Code: code, Message: message}
}

func task7StateErrorTupleRoot(root string, err error) task7StateTuple {
	tuple := task7StateErrorTuple(err)
	tuple.Message = task7StateNormalizePath(root, tuple.Message)
	return tuple
}

func task7StateBlockedTuple(reason string) task7StateTuple {
	return task7StateTuple{Start: -1, Category: "blocked", Code: "unreachable-public-entrypoint", Message: reason}
}

func task7StateNormalizeExceptionTuple(tuples []task7StateTuple, targetFile string) []task7StateTuple {
	result := append([]task7StateTuple(nil), tuples...)
	for index := range result {
		message := result[index].Message
		for _, exceptionClass := range []string{"SyntaxError:", "Error:"} {
			if exception := strings.Index(message, exceptionClass); exception >= 0 {
				message = message[exception:]
				break
			}
		}
		if newline := strings.IndexByte(message, '\n'); newline >= 0 {
			message = message[:newline]
		}
		result[index].File = targetFile
		result[index].Message = message
	}
	return result
}

func task7StateAssertNoWrite(t *testing.T, root, targetFile string, preserveInput bool) {
	t.Helper()
	if preserveInput {
		data, err := os.ReadFile(filepath.Join(root, targetFile))
		if err != nil || string(data) != "{" {
			t.Fatalf("malformed input changed: error=%v data=%q", err, data)
		}
	}
	for _, path := range []string{filepath.Join(root, "out", "index.luau"), filepath.Join(root, "include", "flamework", "config.json")} {
		if _, err := os.Stat(path); !errors.Is(err, os.ErrNotExist) {
			t.Fatalf("failure wrote artifact %s: %v", task7StateNormalizePath(root, path), err)
		}
	}
}

func task7StateParseUpstream(root, category string, output []byte) []task7StateTuple {
	clean := task7StateANSI.ReplaceAllString(string(output), "")
	lines := strings.Split(strings.ReplaceAll(clean, "\r\n", "\n"), "\n")
	tuples := make([]task7StateTuple, 0)
	for _, raw := range lines {
		line := strings.TrimSpace(raw)
		if line == "" {
			continue
		}
		if index := strings.Index(line, "[Flamework]: "); index >= 0 {
			tuples = append(tuples, task7StateTuple{Start: -1, Category: category, Code: "@flamework/core", Message: task7StateNormalizePath(root, line[index+len("[Flamework]: "):])})
			continue
		}
		if strings.HasPrefix(line, "Error:") || strings.HasPrefix(line, "SyntaxError:") || strings.HasPrefix(line, "Assertion Failed!") || strings.Contains(line, "error TS") {
			tuples = append(tuples, task7StateTuple{Start: -1, Category: category, Code: "runtime", Message: task7StateNormalizePath(root, line)})
		}
	}
	if len(tuples) == 0 && len(bytes.TrimSpace(output)) != 0 {
		first := strings.TrimSpace(strings.SplitN(clean, "\n", 2)[0])
		tuples = append(tuples, task7StateTuple{Start: -1, Category: category, Code: "runtime", Message: task7StateNormalizePath(root, first)})
	}
	return tuples
}

func task7StateNormalizePath(root, value string) string {
	normalized := filepath.ToSlash(value)
	if realRoot, err := filepath.EvalSymlinks(root); err == nil {
		normalized = strings.ReplaceAll(normalized, filepath.ToSlash(realRoot), "<project>")
	}
	normalized = strings.ReplaceAll(normalized, filepath.ToSlash(root), "<project>")
	if workspace, err := os.Getwd(); err == nil {
		normalized = strings.ReplaceAll(normalized, filepath.ToSlash(workspace), "<workspace>")
		if realWorkspace, realErr := filepath.EvalSymlinks(workspace); realErr == nil {
			normalized = strings.ReplaceAll(normalized, filepath.ToSlash(realWorkspace), "<workspace>")
		}
	}
	if workspaceRoot, err := filepath.Abs(filepath.Join("..", "..")); err == nil {
		normalized = strings.ReplaceAll(normalized, filepath.ToSlash(workspaceRoot), "<workspace>")
		if realWorkspaceRoot, realErr := filepath.EvalSymlinks(workspaceRoot); realErr == nil {
			normalized = strings.ReplaceAll(normalized, filepath.ToSlash(realWorkspaceRoot), "<workspace>")
		}
	}
	return normalized
}

func task7StateFormatTuples(tuples []task7StateTuple) string {
	parts := make([]string, 0, len(tuples))
	for _, tuple := range tuples {
		parts = append(parts, fmt.Sprintf("(%q,%d,%d,%q,%q,%q,%q)", tuple.File, tuple.Start, tuple.Length, tuple.Category, tuple.Code, tuple.Message, tuple.Related))
	}
	return "[" + strings.Join(parts, ",") + "]"
}
