package flamework

// allow: SIZE_OK — the table and two real-compiler adapters are one differential contract.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/scanner"
)

type task7TransformDiagnosticCase struct {
	row                        string
	fixture                    string
	wantFailure                bool
	baseline                   bool
	unreachable                bool
	wantUpstreamExceptionTuple string
	wantNativeExceptionTuples  []string
}

type task7UpstreamDiagnostic struct {
	Category string                    `json:"category"`
	Code     string                    `json:"code"`
	File     string                    `json:"file"`
	Start    *int                      `json:"start"`
	Message  string                    `json:"message"`
	Related  []task7UpstreamDiagnostic `json:"related"`
}

func TestTask7DiagnosticsTransform_matchesPinnedUpstreamOrderedTuples(t *testing.T) {
	// Given: the exact v1.3.2 transformer and canonical diagnostic branch fixtures.
	base := filepath.Join("testdata", "task7-differential", "project")
	fixtures := filepath.Join("testdata", "task7-diagnostics-transform")
	install := t.TempDir()
	copyTask7Tree(t, base, install)
	runTask7Command(t, install, 90*time.Second, "npm", "install", "--ignore-scripts", "--package-lock=false", "--no-audit", "--no-fund", "--prefer-offline")
	if version := runTask7Command(t, install, 10*time.Second, "node", "-e", `process.stdout.write(require("rbxts-transformer-flamework/package.json").version)`); version != FlameworkVersion {
		t.Fatalf("upstream version = %q, want %q", version, FlameworkVersion)
	}
	cases := []task7TransformDiagnosticCase{
		{row: "DIAG-013", fixture: "constant-nonliteral.ts", wantFailure: true},
		{row: "baseline", fixture: "template-literal-guard.ts", wantFailure: true, baseline: true},
		{row: "DIAG-008", fixture: "dynamic-obfuscated-access.ts", wantFailure: true},
		{row: "DIAG-008", fixture: "direct-attribute-assignment.ts", wantFailure: true},
		{row: "DIAG-008", fixture: "direct-attribute-delete.ts", wantFailure: true},
		{row: "DIAG-008", fixture: "direct-attribute-unary.ts", wantFailure: true},
		{row: "DIAG-012", fixture: "invalid-intrinsic-dispatch.ts", wantFailure: true, wantUpstreamExceptionTuple: "error|@flamework/core|case.ts|3|1|Flamework failure occurred here\nError: Invalid intrinsic usage", wantNativeExceptionTuples: []string{"error|@flamework/core|case.ts|3|1|Invalid path intrinsic usage"}},
		{row: "DIAG-012", fixture: "unknown-intrinsic.ts", wantFailure: true, wantUpstreamExceptionTuple: "error|@flamework/core|case.ts|3|1|Flamework failure occurred here\nTypeError: Cannot use 'in' operator to search for 'diagnostic' in Unexpected intrinsic ID 'task7-unknown' with 0 inputs", wantNativeExceptionTuples: []string{"error|@flamework/core|case.ts|3|1|Unexpected intrinsic ID \"task7-unknown\" with 0 inputs"}},
		{row: "DIAG-012", fixture: "missing-inner-unknown.ts", wantFailure: true},
		{row: "DIAG-012", fixture: "missing-inner-macro.ts", unreachable: true},
		{row: "DIAG-012", fixture: "unknown-macro-type.ts", wantFailure: true},
		{row: "DIAG-013", fixture: "constant-spread.ts", wantFailure: true},
		{row: "DIAG-013", fixture: "constant-linked-mismatch.ts"},
		{row: "DIAG-014", fixture: "unsupported-guard.ts", wantFailure: true},
		{row: "DIAG-014", fixture: "recursive-guard.ts", wantFailure: true, wantUpstreamExceptionTuple: "error|@flamework/core|case.ts|7|1|Flamework failure occurred here\nRangeError: Maximum call stack size exceeded", wantNativeExceptionTuples: []string{"error|@flamework/core|case.ts|7|1|generate guard for Recursive: recursive types cannot be represented without a previously emitted guard (type path: [<project>/invalid/case.ts:Recursive])", "error|@flamework/core|case.ts|3|10|Type was defined here: Recursive"}},
		{row: "DIAG-014", fixture: "type-parameter-guard.ts", wantFailure: true},
		{row: "DIAG-014", fixture: "nominal-intersection-guard.ts", wantFailure: true},
	}

	for _, testCase := range cases {
		t.Run(testCase.row+"/"+strings.TrimSuffix(testCase.fixture, ".ts"), func(t *testing.T) {
			oracleRoot, nativeRoot := t.TempDir(), t.TempDir()
			for _, root := range []string{oracleRoot, nativeRoot} {
				copyTask7Tree(t, base, root)
				linkTask7Modules(t, install, root)
				data, err := os.ReadFile(filepath.Join(fixtures, testCase.fixture))
				if err != nil {
					t.Fatalf("read fixture: %v", err)
				}
				writeTransformFixture(t, root, "invalid/case.ts", string(data))
			}
			plugin := `"noSemanticDiagnostics":true,"salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"full"`
			writeTransformFixture(t, oracleRoot, "tsconfig.json", task7TSFileConfig(plugin, "invalid/case.ts"))
			writeTransformFixture(t, nativeRoot, "tsconfig.json", task7TSFileConfig("", "invalid/case.ts"))

			// When: both public compiler/transformer entrypoints process the same source.
			oracleTuples, oracleFailed, rawOracle := task7RunUpstreamTransformDiagnostics(t, oracleRoot, fixtures)
			nativeTuples, nativeFailed := task7RunNativeTransformDiagnostics(t, nativeRoot)

			// Then: exit state and every ordered location/category/source/message/related tuple are exact.
			if testCase.unreachable {
				if oracleFailed || len(oracleTuples) != 0 || nativeFailed || len(nativeTuples) != 0 {
					t.Fatalf("%s %s public-unreachable contract changed\nupstream failed=%v tuples=%q\nnative failed=%v tuples=%q\nraw upstream:\n%s", testCase.row, testCase.fixture, oracleFailed, oracleTuples, nativeFailed, nativeTuples, rawOracle)
				}
				t.Logf("public-unreachable: getBasicUserMacro nil -> getUserMacroOfMany object fallback -> buildIntrinsicMacro bypass; both success with empty tuples")
				return
			}
			if testCase.wantUpstreamExceptionTuple != "" {
				wantOracle := []string{testCase.wantUpstreamExceptionTuple}
				normalizedOracle := task7NormalizeExceptionTuples(oracleTuples, oracleRoot)
				normalizedNative := task7NormalizeExceptionTuples(nativeTuples, nativeRoot)
				if !oracleFailed || !nativeFailed || !slices.Equal(normalizedOracle, wantOracle) || !slices.Equal(normalizedNative, testCase.wantNativeExceptionTuples) {
					t.Fatalf("%s %s exception mismatch\nupstream failed=%v tuples=%q\nnative failed=%v tuples=%q", testCase.row, testCase.fixture, oracleFailed, normalizedOracle, nativeFailed, normalizedNative)
				}
				for _, root := range []string{oracleRoot, nativeRoot} {
					if _, err := os.Stat(filepath.Join(root, "out")); !os.IsNotExist(err) {
						t.Fatalf("%s %s wrote compiler output after failure: %v", testCase.row, testCase.fixture, err)
					}
				}
				return
			}
			if testCase.baseline {
				if oracleFailed != testCase.wantFailure || nativeFailed != testCase.wantFailure || task7PrimaryTransformMessage(oracleTuples) != task7PrimaryTransformMessage(nativeTuples) {
					t.Fatalf("baseline %s message mismatch\nupstream failed=%v tuples=%q\nnative failed=%v tuples=%q\nraw upstream:\n%s", testCase.fixture, oracleFailed, oracleTuples, nativeFailed, nativeTuples, rawOracle)
				}
				return
			}
			if oracleFailed != testCase.wantFailure || nativeFailed != testCase.wantFailure || !slices.Equal(oracleTuples, nativeTuples) {
				t.Fatalf("%s %s differential mismatch\nupstream failed=%v tuples=%q\nnative failed=%v tuples=%q\nraw upstream:\n%s", testCase.row, testCase.fixture, oracleFailed, oracleTuples, nativeFailed, nativeTuples, rawOracle)
			}
		})
	}
}

func task7NormalizeExceptionTuples(tuples []string, root string) []string {
	normalized := slices.Clone(tuples)
	for index, tuple := range normalized {
		tuple = strings.ReplaceAll(tuple, filepath.ToSlash(root), "<project>")
		if stack := strings.Index(tuple, "\n    at "); stack >= 0 {
			tuple = tuple[:stack]
		}
		normalized[index] = tuple
	}
	return normalized
}

func task7PrimaryTransformMessage(tuples []string) string {
	if len(tuples) == 0 {
		return ""
	}
	parts := strings.SplitN(tuples[0], "|", 6)
	if len(parts) != 6 {
		return tuples[0]
	}
	return parts[5]
}

func task7RunUpstreamTransformDiagnostics(t *testing.T, root, fixtures string) ([]string, bool, string) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 45*time.Second)
	defer cancel()
	driver, err := filepath.Abs(filepath.Join(fixtures, "upstream-driver.cjs"))
	if err != nil {
		t.Fatalf("resolve upstream driver: %v", err)
	}
	command := exec.CommandContext(ctx, "node", driver, root)
	command.Dir = root
	command.Env = append(os.Environ(), "NO_COLOR=1", "NODE_OPTIONS=--require=./deterministic-random.cjs")
	output, err := command.CombinedOutput()
	if ctx.Err() != nil {
		t.Fatalf("upstream compiler timed out: %v", ctx.Err())
	}
	raw := task7ANSI.ReplaceAllString(string(output), "")
	marker := strings.LastIndex(raw, "TASK7_JSON:")
	if err != nil || marker < 0 {
		t.Fatalf("upstream diagnostic driver error=%v output:\n%s", err, raw)
	}
	var diagnostics []task7UpstreamDiagnostic
	if decodeErr := json.Unmarshal([]byte(strings.TrimSpace(raw[marker+len("TASK7_JSON:"):])), &diagnostics); decodeErr != nil {
		t.Fatalf("decode upstream diagnostics: %v\n%s", decodeErr, raw)
	}
	tuples := make([]string, 0, len(diagnostics))
	for _, diagnostic := range diagnostics {
		tuples = append(tuples, task7UpstreamTransformDiagnosticTuples(diagnostic)...)
	}
	return tuples, len(diagnostics) > 0, raw
}

func task7UpstreamTransformDiagnosticTuples(diagnostic task7UpstreamDiagnostic) []string {
	position := -1
	if diagnostic.Start != nil {
		position = *diagnostic.Start
	}
	category := diagnostic.Category
	if category == "message" || category == "suggestion" {
		category = "message"
	}
	tuples := []string{task7PositionTransformTuple(category, diagnostic.File, position, diagnostic.Message)}
	for _, related := range diagnostic.Related {
		tuples = append(tuples, task7UpstreamTransformDiagnosticTuples(related)...)
	}
	return tuples
}

func task7RunNativeTransformDiagnostics(t *testing.T, root string) ([]string, bool) {
	t.Helper()
	program := newTransformProgram(t, root)
	typeChecker, release := program.GetTypeChecker(context.Background())
	defer release()
	files := task7SourceFiles(program, root)
	project, err := OpenProject(ProjectOptions{ProjectDir: root, RootDir: "invalid", OutDir: "out", Config: config.FlameworkConfig{NoSemanticDiagnostics: true, Salt: "task7-controlled-salt", HashPrefix: "task7", IDGenerationMode: "full"}})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	result, transformErr := Transform(TransformInput{Program: program, Checker: typeChecker, Files: files, Project: project})
	tuples := make([]string, 0)
	if transformErr != nil {
		tuples = append(tuples, task7NativeErrorTuples(transformErr)...)
	}
	for _, diagnostic := range result.Diagnostics {
		tuples = append(tuples, task7ASTDiagnosticTuples(diagnostic)...)
	}
	return tuples, transformErr != nil || len(result.Diagnostics) > 0
}

func task7NativeErrorTuples(err error) []string {
	var macroErr *MacroError
	if errors.As(err, &macroErr) && macroErr.Node != nil {
		tuples := []string{task7NodeTransformTuple("error", macroErr.Node, macroErr.Error())}
		for _, related := range macroErr.RelatedInformation {
			tuples = append(tuples, task7NodeTransformTuple("message", related.Node, related.Message))
		}
		return tuples
	}
	var guardErr *GuardGenerationError
	if errors.As(err, &guardErr) {
		tuples := []string{task7PositionTransformTuple("error", guardErr.FileName, guardErr.Start, guardErr.Error())}
		for _, related := range guardErr.RelatedInformation {
			tuples = append(tuples, task7PositionTransformTuple("error", related.FileName, related.Start, "Type was defined here: "+related.TypeName))
		}
		return tuples
	}
	return []string{"fatal|@flamework/core||||" + err.Error()}
}

func task7ASTDiagnosticTuples(diagnostic *ast.Diagnostic) []string {
	category := strings.TrimPrefix(strings.ToLower(diagnostic.Category().String()), "category")
	tuple := task7PositionTransformTuple(category, diagnostic.File().FileName(), diagnostic.Pos(), diagnostic.String())
	tuples := []string{tuple}
	for _, related := range diagnostic.RelatedInformation() {
		tuples = append(tuples, task7ASTDiagnosticTuples(related)...)
	}
	return tuples
}

func task7NodeTransformTuple(category string, node *ast.Node, message string) string {
	file := ast.GetSourceFileOfNode(node)
	return task7PositionTransformTuple(category, file.FileName(), scanner.GetTokenPosOfNode(node, file, false), message)
}

func task7PositionTransformTuple(category, fileName string, position int, message string) string {
	file := ""
	if fileName != "" {
		file = filepath.Base(fileName)
	}
	data, err := os.ReadFile(fileName)
	if err != nil || position < 0 {
		return fmt.Sprintf("%s|@flamework/core|%s|||%s", category, file, message)
	}
	if position > len(data) {
		position = len(data)
	}
	prefix := string(data[:position])
	line := strings.Count(prefix, "\n") + 1
	column := len(prefix) - strings.LastIndex(prefix, "\n")
	return fmt.Sprintf("%s|@flamework/core|%s|%d|%d|%s", category, file, line, column, message)
}
