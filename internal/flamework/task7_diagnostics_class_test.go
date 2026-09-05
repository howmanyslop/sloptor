package flamework

// allow: SIZE_OK -- the assigned file keeps the executable sidecar/native differential in one auditable harness.

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"testing"
	"time"

	"rotor/internal/config"
	"rotor/tsgo/ast"
	"rotor/tsgo/diagnostics"
	"rotor/tsgo/scanner"
)

type task7ClassDiagnosticTuple struct {
	Category string                      `json:"category"`
	Code     string                      `json:"code"`
	File     string                      `json:"file,omitempty"`
	Start    int                         `json:"start"`
	Length   int                         `json:"length"`
	Message  string                      `json:"message"`
	Related  []task7ClassDiagnosticTuple `json:"related,omitempty"`
}

type task7ClassSidecarResponse struct {
	Diagnostics []task7ClassDiagnosticTuple `json:"diagnostics"`
	Transformed []struct {
		FileName string `json:"fileName"`
		Text     string `json:"text"`
	} `json:"transformed"`
}

func TestTask7DiagnosticsClass_canonicalDiagnosticBranchesMatchOracle(t *testing.T) {
	// Given
	want := map[string]struct {
		oracle []task7ClassDiagnosticTuple
		native []task7ClassDiagnosticTuple
	}{
		"constraint": {},
		"circular":   {},
		"uid-symbol": {
			oracle: []task7ClassDiagnosticTuple{{Category: "error", Code: " @flamework/core", File: "src/server/uid-symbol.server.ts", Start: 95, Length: 10, Message: "Could not find UID for symbol \"globalThis\""}},
			native: []task7ClassDiagnosticTuple{{Category: "error", Code: " @flamework/core", File: "src/server/uid-symbol.server.ts", Start: 95, Length: 10, Message: "Could not find UID for symbol \"globalThis\""}},
		},
		"uid-type": {
			oracle: []task7ClassDiagnosticTuple{{Category: "error", Code: " @flamework/core", File: "src/server/uid-type.server.ts", Start: 102, Length: 24, Message: "Could not find UID for type \"T extends string ? 1 : 2\""}},
			native: []task7ClassDiagnosticTuple{{Category: "error", Code: " @flamework/core", File: "src/server/uid-type.server.ts", Start: 102, Length: 24, Message: "Could not find UID for type \"T extends string ? 1 : 2\""}},
		},
	}

	for _, name := range []string{"constraint", "circular", "uid-symbol", "uid-type"} {
		t.Run(name, func(t *testing.T) {
			// When
			oracle, native := task7ClassRunDiagnosticCase(t, name)

			// Then
			if !reflect.DeepEqual(oracle, want[name].oracle) || !reflect.DeepEqual(native, want[name].native) {
				t.Fatalf("%s exact tuples differ\nactual oracle: %#v\nactual native: %#v", name, oracle, native)
			}
			if !reflect.DeepEqual(oracle, native) {
				t.Fatalf("%s canonical tuple parity RED\noracle: %#v\nnative: %#v", name, oracle, native)
			}
		})
	}
}

func TestTask7DiagnosticsClass_orderedDiagnosticArtifactMatchesOracle(t *testing.T) {
	// Given
	wantOracle := []task7ClassDiagnosticTuple{
		{Category: "error", Code: " @flamework/core", File: "src/server/ordered-symbol.server.ts", Start: 95, Length: 10, Message: "Could not find UID for symbol \"globalThis\""},
		{Category: "error", Code: " @flamework/core", File: "src/server/ordered-type.server.ts", Start: 102, Length: 24, Message: "Could not find UID for type \"T extends string ? 1 : 2\""},
	}
	wantNative := append([]task7ClassDiagnosticTuple(nil), wantOracle...)

	// When
	oracle, native := task7ClassRunDiagnosticCase(t, "ordered")

	// Then
	if !reflect.DeepEqual(oracle, wantOracle) || !reflect.DeepEqual(native, wantNative) {
		t.Fatalf("ordered diagnostic artifact differs\nactual oracle: %#v\nactual native: %#v", oracle, native)
	}
	if !reflect.DeepEqual(oracle, native) {
		t.Fatalf("ordered diagnostic artifact parity RED\noracle: %#v\nnative: %#v", oracle, native)
	}
}

func TestTask7DiagnosticsClass_sameCompilerFiveFamilyTreeMatchesOracle(t *testing.T) {
	// Given
	fixture := filepath.Join("testdata", "task7-differential", "project")
	install := t.TempDir()
	copyTask7Tree(t, fixture, install)
	runTask7Command(t, install, 90*time.Second, "npm", "install", "--ignore-scripts", "--package-lock=false", "--no-audit", "--no-fund", "--prefer-offline")
	version := runTask7Command(t, install, 10*time.Second, "node", "-e", `process.stdout.write(require("rbxts-transformer-flamework/package.json").version)`)
	if version != FlameworkVersion {
		t.Fatalf("oracle transformer version = %q, want %q", version, FlameworkVersion)
	}
	oracleRoot, nativeRoot := t.TempDir(), t.TempDir()
	limit2 := 2
	for _, root := range []string{oracleRoot, nativeRoot} {
		copyTask7Tree(t, fixture, root)
		linkTask7Modules(t, install, root)
		macrosPath := filepath.Join(root, "src", "shared", "macros.ts")
		macros, err := os.ReadFile(macrosPath)
		if err != nil {
			t.Fatalf("read five-family macros fixture: %v", err)
		}
		fiveFamilyMacros := strings.ReplaceAll(string(macros), "\nFlamework.addPaths(\"src/server\");\nFlamework.addPathsGlob(\"src/**/*.ts\");", "")
		if fiveFamilyMacros == string(macros) {
			t.Fatal("five-family fixture did not remove the separately covered macro-path family")
		}
		if err := os.WriteFile(macrosPath, []byte(fiveFamilyMacros), 0o644); err != nil {
			t.Fatalf("write five-family macros fixture: %v", err)
		}
		writeTransformFixture(t, root, "flamework.build", `{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"stringHashes":{"task4:payload":"00000000-0000-4000-8000-000000000004"},"salt":"task7-controlled-salt"}`)
	}
	plugin := `"salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"full","optimizations":{"guardGenerationDedupLimit":2}`
	writeTransformFixture(t, oracleRoot, "tsconfig.json", task7TSConfig(plugin, "src"))
	writeTransformFixture(t, nativeRoot, "tsconfig.json", task7TSConfig("", "src"))
	oracleFiles := task7ClassCompileFiles(t, oracleRoot)
	if len(oracleFiles) != 5 {
		t.Fatalf("representative family source count = %d, want 5", len(oracleFiles))
	}

	// When
	response := task7ClassRunSidecar(t, oracleRoot, oracleFiles)
	if len(response.Diagnostics) != 0 || len(response.Transformed) != len(oracleFiles) {
		t.Fatalf("sidecar diagnostics=%#v transformed=%d, want zero/%d", response.Diagnostics, len(response.Transformed), len(oracleFiles))
	}
	for _, transformed := range response.Transformed {
		if err := os.WriteFile(filepath.FromSlash(transformed.FileName), []byte(transformed.Text), 0o644); err != nil {
			t.Fatalf("write sidecar transform %s: %v", transformed.FileName, err)
		}
	}
	oracleProject, err := OpenProject(ProjectOptions{ProjectDir: oracleRoot, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{Salt: "task7-controlled-salt", HashPrefix: "task7", IDGenerationMode: "full", Optimizations: config.FlameworkOptimizations{GuardGenerationDedupLimit: &limit2}}})
	if err != nil {
		t.Fatalf("OpenProject() sidecar oracle: %v", err)
	}
	lowerTask7Files(t, oracleRoot, oracleProject)
	_, nativeDiagnostics, nativeErr := runTask7Native(t, nativeRoot, config.FlameworkConfig{Salt: "task7-controlled-salt", HashPrefix: "task7", IDGenerationMode: "full", Optimizations: config.FlameworkOptimizations{GuardGenerationDedupLimit: &limit2}})
	if nativeErr != nil || len(nativeDiagnostics) != 0 {
		t.Fatalf("native error=%v diagnostics=%v", nativeErr, nativeDiagnostics)
	}
	oracleTree := task7ClassTransformerTree(t, oracleRoot)
	nativeTree := task7ClassTransformerTree(t, nativeRoot)

	// Then
	if len(oracleTree) != 7 || len(nativeTree) != 7 {
		t.Fatalf("transformer-owned tree sizes oracle=%d native=%d, want 7", len(oracleTree), len(nativeTree))
	}
	if !reflect.DeepEqual(oracleTree, nativeTree) {
		t.Fatalf("same-compiler transformer-owned tree differs\noracle hash=%s\nnative hash=%s\n%s", task7ClassTreeHash(oracleTree), task7ClassTreeHash(nativeTree), task7ClassTreeDiff(oracleTree, nativeTree))
	}
	if task7ClassTreeHash(oracleTree) != task7ClassTreeHash(nativeTree) {
		t.Fatal("independent transformer-owned tree hashes differ after byte equality")
	}
	t.Logf("same-compiler boundary: five transformed sources, seven transformer-owned artifacts, sha256=%s; rbxtsc banners and tsbuildinfo are compiler-owned and excluded", task7ClassTreeHash(nativeTree))
}

func task7ClassRunDiagnosticCase(t *testing.T, name string) ([]task7ClassDiagnosticTuple, []task7ClassDiagnosticTuple) {
	t.Helper()
	fixture := filepath.Join("testdata", "task7-differential", "project")
	oracleRoot, nativeRoot := t.TempDir(), t.TempDir()
	fixtureNames := []string{name}
	if name == "ordered" {
		fixtureNames = []string{"uid-symbol", "uid-type"}
	}
	sourcePaths := make([]string, len(fixtureNames))
	for index, fixtureName := range fixtureNames {
		outputName := name
		if name == "ordered" {
			outputName = "ordered-" + strings.TrimPrefix(fixtureName, "uid-")
		}
		sourcePaths[index] = "src/server/" + outputName + ".server.ts"
	}
	for _, root := range []string{oracleRoot, nativeRoot} {
		copyTask7Tree(t, fixture, root)
		task7ClassLinkPinnedModules(t, root)
		for index, fixtureName := range fixtureNames {
			source, err := os.ReadFile(filepath.Join("testdata", "task7-diagnostics-class", fixtureName+".ts"))
			if err != nil {
				t.Fatalf("read %s fixture: %v", fixtureName, err)
			}
			writeTransformFixture(t, root, sourcePaths[index], string(source))
		}
	}
	plugin := `"salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"full"`
	writeTransformFixture(t, oracleRoot, "tsconfig.json", task7ClassTSFilesConfig(plugin, sourcePaths))
	writeTransformFixture(t, nativeRoot, "tsconfig.json", task7ClassTSFilesConfig("", sourcePaths))
	oracleOutput := runTask7Oracle(t, oracleRoot, "tsconfig.json", name == "constraint" || name == "circular")
	t.Logf("raw upstream %s diagnostics:\n%s", name, oracleOutput)
	oracleDiagnostics := task7ClassOracleCLITuples(t, oracleRoot, oracleOutput)
	return task7ClassRelativizeTuples(oracleRoot, oracleDiagnostics), task7ClassRunNativeTuples(t, nativeRoot)
}

func task7ClassRunNativeTuples(t *testing.T, root string) []task7ClassDiagnosticTuple {
	t.Helper()
	program := newTransformProgram(t, root)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	files := task7SourceFiles(program, root)
	project, err := OpenProject(ProjectOptions{ProjectDir: root, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{Salt: "task7-controlled-salt", HashPrefix: "task7", IDGenerationMode: "full"}})
	if err != nil {
		t.Fatalf("OpenProject() native diagnostics: %v", err)
	}
	runtime := MacroRuntime{UUID: func() (string, error) { return "00000000-0000-4000-8000-000000000004", nil }, RandomIndex: func(int) (int, error) { return 0, nil }}
	result, transformErr := Transform(TransformInput{Program: program, Checker: checker, Files: files, Project: project, MacroRuntime: &runtime})
	if transformErr != nil {
		return []task7ClassDiagnosticTuple{task7ClassErrorTuple(root, transformErr)}
	}
	if len(result.Diagnostics) == 0 {
		return nil
	}
	tuples := make([]task7ClassDiagnosticTuple, 0, len(result.Diagnostics))
	for _, diagnostic := range result.Diagnostics {
		tuples = append(tuples, task7ClassASTTuple(root, diagnostic))
	}
	return tuples
}

func task7ClassASTTuple(root string, diagnostic *ast.Diagnostic) task7ClassDiagnosticTuple {
	var related []task7ClassDiagnosticTuple
	for _, info := range diagnostic.RelatedInformation() {
		related = append(related, task7ClassASTTuple(root, info))
	}
	category := "warning"
	if diagnostic.Category() == diagnostics.CategoryError {
		category = "error"
	}
	code := fmt.Sprint(diagnostic.Code())
	if stringCode, ok := diagnostic.StringCode(); ok {
		code = stringCode
	}
	return task7ClassDiagnosticTuple{Category: category, Code: code, File: task7ClassRelativePath(root, diagnostic.File().FileName()), Start: diagnostic.Pos(), Length: diagnostic.Len(), Message: diagnostic.String(), Related: related}
}

func task7ClassErrorTuple(root string, err error) task7ClassDiagnosticTuple {
	var macroErr *MacroError
	if errors.As(err, &macroErr) {
		file := ast.GetSourceFileOfNode(macroErr.Node)
		start := scanner.GetTokenPosOfNode(macroErr.Node, file, false)
		return task7ClassDiagnosticTuple{Category: "error", Code: "ErrMacroDiagnostic", File: task7ClassRelativePath(root, file.FileName()), Start: start, Length: macroErr.Node.End() - start, Message: macroErr.Message}
	}
	if errors.Is(err, ErrCircularDependency) {
		return task7ClassDiagnosticTuple{Category: "error", Code: "ErrCircularDependency", Start: -1, Message: err.Error()}
	}
	return task7ClassDiagnosticTuple{Category: "error", Code: fmt.Sprintf("%T", err), Start: -1, Message: err.Error()}
}

func task7ClassRunSidecar(t *testing.T, root string, compileFiles []string) task7ClassSidecarResponse {
	t.Helper()
	sidecar, err := filepath.Abs(filepath.Join("..", "..", "tools", "sidecar", "main.js"))
	if err != nil {
		t.Fatalf("resolve sidecar: %v", err)
	}
	request := map[string]any{"protocol": 1, "operation": "transform", "tsConfigPath": filepath.Join(root, "tsconfig.json"), "projectDir": root, "compileFileNames": compileFiles, "changedFiles": []struct{}{}}
	requestData, err := json.Marshal(request)
	if err != nil {
		t.Fatalf("marshal sidecar request: %v", err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	command := exec.CommandContext(ctx, "node", sidecar, "--project", filepath.Join(root, "tsconfig.json"), "--rojo", filepath.Join(root, "default.project.json"))
	command.Dir = root
	command.Env = append(os.Environ(), "NODE_OPTIONS=--require=./deterministic-random.cjs")
	command.Stdin = bytes.NewReader(append(requestData, '\n'))
	var stderr bytes.Buffer
	command.Stderr = &stderr
	output, err := command.Output()
	if err != nil || ctx.Err() != nil {
		t.Fatalf("sidecar error=%v timeout=%v stderr=%s", err, ctx.Err(), stderr.String())
	}
	var response task7ClassSidecarResponse
	if err := json.Unmarshal(bytes.TrimSpace(output), &response); err != nil {
		t.Fatalf("decode sidecar response: %v\nstdout=%s\nstderr=%s", err, output, stderr.String())
	}
	return response
}

func task7ClassOracleCLITuples(t *testing.T, root, output string) []task7ClassDiagnosticTuple {
	t.Helper()
	clean := task7ANSI.ReplaceAllString(output, "")
	linePattern := regexp.MustCompile(`(?m)^\s*([^\n:]+):(\d+):(\d+) - (error|message) TS([^:]+): ([^\n]+)$`)
	matches := linePattern.FindAllStringSubmatchIndex(clean, -1)
	tuples := make([]task7ClassDiagnosticTuple, 0, len(matches))
	for matchIndex, indices := range matches {
		fields := linePattern.FindStringSubmatch(clean[indices[0]:indices[1]])
		line, lineErr := strconv.Atoi(fields[2])
		column, columnErr := strconv.Atoi(fields[3])
		if lineErr != nil || columnErr != nil {
			t.Fatalf("parse oracle location %q: line=%v column=%v", fields[0], lineErr, columnErr)
		}
		fileName := filepath.ToSlash(strings.TrimSpace(fields[1]))
		source, err := os.ReadFile(filepath.Join(root, filepath.FromSlash(fileName)))
		if err != nil {
			t.Fatalf("read oracle diagnostic source %s: %v", fileName, err)
		}
		start := task7ClassLineColumnOffset(string(source), line, column)
		blockEnd := len(clean)
		if matchIndex+1 < len(matches) {
			blockEnd = matches[matchIndex+1][0]
		}
		underlineLength := 0
		for _, outputLine := range strings.Split(clean[indices[1]:blockEnd], "\n") {
			if count := strings.Count(outputLine, "~"); count > 0 {
				underlineLength = count
				break
			}
		}
		category := fields[4]
		if category == "message" {
			category = "warning"
		}
		tuples = append(tuples, task7ClassDiagnosticTuple{Category: category, Code: fields[5], File: fileName, Start: start, Length: underlineLength, Message: strings.TrimSuffix(fields[6], "\r")})
	}
	return tuples
}

func task7ClassLineColumnOffset(source string, line, column int) int {
	offset := 0
	lines := strings.Split(source, "\n")
	for index := 0; index < line-1; index++ {
		offset += len(lines[index]) + 1
	}
	return offset + column - 1
}

func task7ClassLinkPinnedModules(t *testing.T, root string) {
	t.Helper()
	modules, err := filepath.Abs(filepath.Join("..", "..", "testdata", "transformers", "project", "node_modules"))
	if err != nil {
		t.Fatalf("resolve pinned modules: %v", err)
	}
	version, err := os.ReadFile(filepath.Join(modules, "rbxts-transformer-flamework", "package.json"))
	if err != nil || !bytes.Contains(version, []byte(`"version": "1.3.2"`)) {
		t.Fatalf("pinned transformer unavailable or wrong version: %v", err)
	}
	if err := os.Symlink(modules, filepath.Join(root, "node_modules")); err != nil {
		t.Fatalf("link pinned modules: %v", err)
	}
}

func task7ClassCompileFiles(t *testing.T, root string) []string {
	t.Helper()
	program := newTransformProgram(t, root)
	files := task7SourceFiles(program, root)
	names := make([]string, len(files))
	for index, file := range files {
		names[index] = filepath.FromSlash(file.FileName())
	}
	return names
}

func task7ClassTSFilesConfig(plugin string, sources []string) string {
	configuration := task7TSConfig(plugin, "src")
	quoted := make([]string, len(sources))
	for index, source := range sources {
		quoted[index] = strconv.Quote(source)
	}
	return strings.Replace(configuration, `"include":["src"]`, `"files":[`+strings.Join(quoted, ",")+`]`, 1)
}

func task7ClassRelativizeTuples(root string, tuples []task7ClassDiagnosticTuple) []task7ClassDiagnosticTuple {
	result := append([]task7ClassDiagnosticTuple(nil), tuples...)
	for index := range result {
		result[index].File = task7ClassRelativePath(root, result[index].File)
	}
	return result
}

func task7ClassRelativePath(root, path string) string {
	if path == "" {
		return ""
	}
	relative, err := filepath.Rel(root, filepath.FromSlash(path))
	if err != nil {
		return filepath.ToSlash(path)
	}
	return filepath.ToSlash(relative)
}

func task7ClassTransformerTree(t *testing.T, root string) map[string]string {
	t.Helper()
	result := map[string]string{}
	err := filepath.WalkDir(filepath.Join(root, "out"), func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil || entry.IsDir() || filepath.Ext(path) != ".luau" {
			return walkErr
		}
		data, err := os.ReadFile(path)
		if err == nil {
			result[task7ClassRelativePath(root, path)] = string(data)
		}
		return err
	})
	if err != nil {
		t.Fatalf("walk transformer Luau tree: %v", err)
	}
	artifactNames := []string{"flamework.build"}
	artifactRoot := filepath.Join(root, "include", "flamework")
	err = filepath.WalkDir(artifactRoot, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil || entry.IsDir() || filepath.Ext(path) != ".json" {
			return walkErr
		}
		artifactNames = append(artifactNames, task7ClassRelativePath(root, path))
		return nil
	})
	if err != nil {
		t.Fatalf("walk transformer JSON artifacts: %v", err)
	}
	sort.Strings(artifactNames)
	for _, name := range artifactNames {
		data, err := os.ReadFile(filepath.Join(root, filepath.FromSlash(name)))
		if err != nil {
			t.Fatalf("read transformer artifact %s: %v", name, err)
		}
		var artifact any
		if err := json.Unmarshal(data, &artifact); err != nil {
			t.Fatalf("decode transformer artifact %s: %v", name, err)
		}
		normalizeTask7JSON(artifact, false, filepath.Base(name) == "globs.json")
		canonical, err := json.Marshal(artifact)
		if err != nil {
			t.Fatalf("canonicalize transformer artifact %s: %v", name, err)
		}
		result[name] = string(canonical)
	}
	return result
}

func task7ClassTreeHash(tree map[string]string) string {
	names := make([]string, 0, len(tree))
	for name := range tree {
		names = append(names, name)
	}
	sort.Strings(names)
	hash := sha256.New()
	for _, name := range names {
		_, _ = hash.Write([]byte(name))
		_, _ = hash.Write([]byte{0})
		_, _ = hash.Write([]byte(tree[name]))
		_, _ = hash.Write([]byte{0})
	}
	return hex.EncodeToString(hash.Sum(nil))
}

func task7ClassTreeDiff(oracle, native map[string]string) string {
	names := map[string]bool{}
	for name := range oracle {
		names[name] = true
	}
	for name := range native {
		names[name] = true
	}
	ordered := make([]string, 0, len(names))
	for name := range names {
		ordered = append(ordered, name)
	}
	sort.Strings(ordered)
	var differences []string
	for _, name := range ordered {
		if oracle[name] != native[name] {
			if filepath.Ext(name) == ".luau" {
				differences = append(differences, fmt.Sprintf("%s oracle=%x native=%x %s", name, sha256.Sum256([]byte(oracle[name])), sha256.Sum256([]byte(native[name])), task7ClassByteDiff(oracle[name], native[name])))
			} else {
				differences = append(differences, fmt.Sprintf("%s\noracle=%s\nnative=%s", name, oracle[name], native[name]))
			}
		}
	}
	return strings.Join(differences, "\n")
}

func task7ClassByteDiff(oracle, native string) string {
	limit := len(oracle)
	if len(native) < limit {
		limit = len(native)
	}
	offset := 0
	for offset < limit && oracle[offset] == native[offset] {
		offset++
	}
	start := offset - 120
	if start < 0 {
		start = 0
	}
	oracleEnd, nativeEnd := offset+240, offset+240
	if oracleEnd > len(oracle) {
		oracleEnd = len(oracle)
	}
	if nativeEnd > len(native) {
		nativeEnd = len(native)
	}
	return fmt.Sprintf("first-byte=%d oracle-len=%d native-len=%d oracle-context=%q native-context=%q", offset, len(oracle), len(native), oracle[start:oracleEnd], native[start:nativeEnd])
}
