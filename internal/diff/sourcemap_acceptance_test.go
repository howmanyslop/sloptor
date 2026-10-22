package diff

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"rotor/internal/compile"
	"rotor/internal/forkparity"
	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/scanner"
	"rotor/tsgo/sourcemap"
	"rotor/tsgo/tspath"
)

type sourceStatementSpan struct{ first, last int }

type sourceMapCorpusFile struct {
	spans   []sourceStatementSpan
	exports map[string]sourceStatementSpan
	marked  string
}

const sourceMapGroupMarker = "__source_map_statement_"

// Derive statement ownership independently of origins, renderer ranges and
// source maps: parse authored TS, insert comment delimiters between top-level
// statements, then compile WITHOUT maps. Removing the delimiters must recover
// the exact original Luau bytes. This also assigns hoists/prereqs to the
// statement that produced them. Export epilogues are matched to TS declarations.
func markSourceMapStatements(t *testing.T, path, text string) sourceMapCorpusFile {
	t.Helper()
	file := parser.ParseSourceFile(ast.SourceFileParseOptions{
		FileName: filepath.ToSlash(path), Path: tspath.Path(filepath.ToSlash(path)),
	}, text, core.GetScriptKindFromFileName(path))
	if len(file.Diagnostics()) != 0 {
		t.Fatalf("parse %s: %v", path, file.Diagnostics())
	}
	result := sourceMapCorpusFile{exports: map[string]sourceStatementSpan{}}
	var marked strings.Builder
	previous := 0
	for index, statement := range file.Statements.Nodes {
		first, _ := scanner.GetECMALineAndUTF16CharacterOfPosition(file, scanner.GetTokenPosOfNode(statement, file, false))
		last, _ := scanner.GetECMALineAndUTF16CharacterOfPosition(file, statement.End()-1)
		span := sourceStatementSpan{first, last}
		result.spans = append(result.spans, span)
		marked.WriteString(text[previous:statement.Pos()])
		fmt.Fprintf(&marked, "\n// %s%d__\n", sourceMapGroupMarker, index)
		previous = statement.Pos()
		if ast.HasSyntacticModifier(statement, ast.ModifierFlagsExport) {
			switch {
			case ast.HasSyntacticModifier(statement, ast.ModifierFlagsDefault):
				result.exports["default"] = span
			case ast.IsVariableStatement(statement):
				for _, declaration := range statement.AsVariableStatement().DeclarationList.AsVariableDeclarationList().Declarations.Nodes {
					if !ast.IsIdentifier(declaration.Name()) {
						t.Fatalf("add export binding-pattern ownership for %s", path)
					}
					result.exports[declaration.Name().Text()] = span
				}
			default:
				if name := statement.Name(); name != nil {
					result.exports[name.Text()] = span
				}
			}
		}
	}
	marked.WriteString(text[previous:])
	fmt.Fprintf(&marked, "\n// %send__\n", sourceMapGroupMarker)
	result.marked = marked.String()
	return result
}

func sourceMapOutputGroups(t *testing.T, text string, file sourceMapCorpusFile) (string, map[int]sourceStatementSpan) {
	t.Helper()
	var clean strings.Builder
	groups := map[int]sourceStatementSpan{}
	current, line := -1, 0
	for _, content := range strings.SplitAfter(text, "\n") {
		if content == "" {
			continue
		}
		trimmed := strings.TrimSpace(content)
		if marker, ok := strings.CutPrefix(trimmed, "-- "+sourceMapGroupMarker); ok {
			marker = strings.TrimSuffix(marker, "__")
			if marker == "end" {
				current = -1
			} else {
				var err error
				current, err = strconv.Atoi(marker)
				if err != nil || current < 0 || current >= len(file.spans) {
					t.Fatalf("invalid output group marker %q", content)
				}
			}
			continue
		}
		if current >= 0 {
			groups[line] = file.spans[current]
		} else if lhs, _, ok := strings.Cut(trimmed, " = "); ok {
			if span, found := file.exports[strings.TrimPrefix(lhs, "exports.")]; found {
				groups[line] = span
			}
		}
		clean.WriteString(content)
		line++
	}
	return clean.String(), groups
}

func TestDifferentialSourceMapCoverage(t *testing.T) {
	root := repoRoot(t)
	project := filepath.Join(root, "testdata", "diff", "project")
	dir := t.TempDir()
	if err := os.CopyFS(filepath.Join(dir, "src"), os.DirFS(filepath.Join(project, "src"))); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"package.json", "default.project.json", "tsconfig.json"} {
		contents, err := os.ReadFile(filepath.Join(project, name))
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, name), contents, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := os.Stat(filepath.Join(project, "node_modules", "@rbxts", "types", "package.json")); err != nil {
		t.Fatalf("install corpus dependencies with bun install --no-save: %v", err)
	}
	if err := os.Symlink(filepath.Join(project, "node_modules"), filepath.Join(dir, "node_modules")); err != nil {
		t.Fatal(err)
	}
	configPath := filepath.Join(dir, "tsconfig.json")
	config, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatal(err)
	}
	withMaps := strings.Replace(string(config), `"outDir": "out"`, `"outDir": "out", "sourceMap": true`, 1)
	if withMaps == string(config) {
		t.Fatal("could not enable source maps")
	}
	if err := os.WriteFile(configPath, []byte(withMaps), 0o644); err != nil {
		t.Fatal(err)
	}
	build, diags, err := compile.BuildProjectWithOptions(dir, compile.ProjectOptions{})
	if err != nil || len(diags) != 0 {
		t.Fatalf("build with maps: %v (diagnostics: %v)", err, diags)
	}
	if err := os.WriteFile(configPath, config, 0o644); err != nil {
		t.Fatal(err)
	}
	withoutMaps, diags, err := compile.CompileProject(dir)
	if err != nil || len(diags) != 0 {
		t.Fatalf("compile without maps: %v (diagnostics: %v)", err, diags)
	}
	files := map[string]sourceMapCorpusFile{}
	overlays := map[string]string{}
	for _, name := range EnabledFixtures {
		matches, err := filepath.Glob(filepath.Join(dir, "src", name+".ts*"))
		if err != nil || len(matches) != 1 {
			t.Fatalf("find source %s: %v, %v", name, matches, err)
		}
		source, err := os.ReadFile(matches[0])
		if err != nil {
			t.Fatal(err)
		}
		file := markSourceMapStatements(t, matches[0], string(source))
		files[name] = file
		overlays[filepath.ToSlash(matches[0])] = file.marked
	}
	markedOutputs, diags, err := compile.CompileProjectWithOptions(dir, compile.ProjectOptions{Overlays: overlays})
	if err != nil || len(diags) != 0 {
		t.Fatalf("compile output groups without maps: %v (diagnostics: %v)", err, diags)
	}
	ledger, err := forkparity.ReadDivergenceLedger(filepath.Join(root, "testdata", "forkparity", "divergence-ledger.json"))
	if err != nil {
		t.Fatal(err)
	}
	corrections := map[string]string{}
	for _, row := range ledger.Rows {
		if row.Classification == forkparity.DivergenceUpstreamCorrected {
			corrections[row.ID] = row.BehavioralTest
		}
	}
	verified := map[string]bool{}
	mappedLines := 0
	for _, name := range EnabledFixtures {
		t.Run(name, func(t *testing.T) {
			outputPath := "out/" + name + ".luau"
			text, ok := build.Outputs[outputPath]
			if !ok {
				t.Fatalf("missing %s", outputPath)
			}
			if text != withoutMaps[outputPath] {
				t.Fatal("enabling maps changed Luau bytes")
			}
			golden, err := os.ReadFile(filepath.Join(root, "testdata", "diff", "golden", name+".luau"))
			if err != nil {
				t.Fatal(err)
			}
			if behavioralTest, corrected := corrections["differential/"+name]; corrected {
				if !verified[behavioralTest] {
					if err := forkparity.VerifyBehavioralTest(t.Context(), root, behavioralTest); err != nil {
						t.Fatal(err)
					}
					verified[behavioralTest] = true
				}
				t.Logf("approved upstream correction verified by %s; map properties still required", behavioralTest)
			} else if text != string(golden) {
				reportFirstDiff(t, text, string(golden))
			}
			clean, groups := sourceMapOutputGroups(t, markedOutputs[outputPath], files[name])
			if clean != text {
				t.Fatal("statement delimiters changed output bytes; ownership oracle is invalid")
			}
			mapBytes, err := os.ReadFile(filepath.Join(dir, "out", name+".luau.map"))
			if err != nil {
				t.Fatal(err)
			}
			var sourceMap struct{ Mappings string }
			if err := json.Unmarshal(mapBytes, &sourceMap); err != nil {
				t.Fatal(err)
			}
			segments := map[int][]sourcemap.Mapping{}
			decoder := sourcemap.DecodeMappings(sourceMap.Mappings)
			for mapping := range decoder.Values() {
				segments[mapping.GeneratedLine] = append(segments[mapping.GeneratedLine], *mapping)
			}
			if err := decoder.Error(); err != nil {
				t.Fatal(err)
			}
			lines := strings.Split(strings.TrimSuffix(text, "\n"), "\n")
			exportWrapper := false
			for line, content := range lines {
				trimmed := strings.TrimSpace(content)
				excluded := trimmed == "" || strings.HasPrefix(trimmed, "--") ||
					strings.HasPrefix(content, "local TS = ") || content == "local exports = {}" ||
					content == "return nil" || content == "return exports" || content == "return {" ||
					exportWrapper && content == "}"
				if content == "return {" {
					exportWrapper = true
				}
				mappings := segments[line]
				if excluded {
					if len(mappings) != 0 {
						t.Errorf("excluded Luau line %d has segments: %q", line+1, content)
					}
					continue
				}
				if len(mappings) != 1 || mappings[0].GeneratedCharacter != 0 || !mappings[0].IsSourceMapping() {
					t.Errorf("Luau line %d needs one sourced column-zero segment: %q, got %+v", line+1, content, mappings)
					continue
				}
				mappedLines++
				span, found := groups[line]
				if !found {
					t.Errorf("no independently derived TS statement for Luau line %d: %q", line+1, content)
				} else if original := mappings[0].SourceLine; original < span.first || original > span.last {
					t.Errorf("Luau line %d (%q) maps to TS %d, outside producing statement %d..%d", line+1, content, original+1, span.first+1, span.last+1)
				}
			}
			for line := range segments {
				if line < 0 || line >= len(lines) {
					t.Errorf("segment outside generated output: %d", line+1)
				}
			}
		})
	}
	t.Logf("checked all %d enabled fixtures and %d mapped lines", len(EnabledFixtures), mappedLines)
}
