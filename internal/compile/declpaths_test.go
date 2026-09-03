package compile

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/tspath"
)

// declarationRewriterFixture writes a throwaway project, builds a Program from
// it, and returns the rewriter plus a lookup for the file declarations are
// emitted from. Specifiers resolve against the real filesystem, which is the
// point: the rewrite is a resolution result, never a string transform.
func declarationRewriterFixture(t *testing.T, options string, files map[string]string) (*declarationPathRewriter, func(string) *ast.SourceFile) {
	t.Helper()
	dir := t.TempDir()
	config := `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"declaration": true,
		"strict": true,
		"rootDir": ".",
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"outDir": "out",
		"baseUrl": ".",
		"paths": { "@alias/*": ["src/*"] }` + options + `
	},
	"include": ["src", "test"]
}`
	write := func(name, text string) {
		t.Helper()
		path := filepath.Join(dir, filepath.FromSlash(name))
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write("tsconfig.json", config)
	write("package.json", `{"name":"@scope/declpaths-fixture"}`)
	write("src/globals.d.ts", noLibGlobalStubs)
	for name, text := range files {
		write(name, text)
	}
	_, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (diags: %v)", err, diags)
	}
	rewriter := newDeclarationPathRewriter(program)
	if rewriter == nil {
		t.Fatal("newDeclarationPathRewriter returned nil for a project with paths")
	}
	return rewriter, func(name string) *ast.SourceFile {
		t.Helper()
		file := program.GetSourceFile(filepath.Join(dir, filepath.FromSlash(name)))
		if file == nil {
			t.Fatalf("fixture source file missing from the program: %s", name)
		}
		return file
	}
}

func rewriteDeclaration(t *testing.T, rewriter *declarationPathRewriter, entry *ast.SourceFile, declText string) string {
	t.Helper()
	declFileName := strings.TrimSuffix(entry.FileName(), ".ts") + ".d.ts"
	// Mirrors rewriteDeclarationEmit: the filter is what orders the edits.
	return applyTextEdits(declText, nonOverlappingTextEdits(rewriter.specifierEdits(entry, declFileName, declText)))
}

// Every syntax that can carry a module specifier into a `.d.ts`, plus the
// shapes that must survive untouched.
func TestDeclarationPathRewriteSpecifierKinds(t *testing.T) {
	rewriter, sourceFile := declarationRewriterFixture(t, "", map[string]string{
		"src/main.ts":                            "export const main = 1;\n",
		"src/value.ts":                           "export interface Value { label: string; }\nexport const value: Value = { label: \"v\" };\n",
		"node_modules/@rbxts/dummy/package.json": `{"name":"@rbxts/dummy","types":"index.d.ts"}`,
		"node_modules/@rbxts/dummy/index.d.ts":   "export declare const dummy: number;\n",
	})
	entry := sourceFile("src/main.ts")

	for _, testCase := range []struct {
		name string
		decl string
		want string
	}{
		{
			name: "import declaration",
			decl: "import { Value } from \"@alias/value\";\n",
			want: "import { Value } from \"./value\";\n",
		},
		{
			name: "export declaration",
			decl: "export { Value } from \"@alias/value\";\n",
			want: "export { Value } from \"./value\";\n",
		},
		{
			name: "export star declaration",
			decl: "export * from \"@alias/value\";\n",
			want: "export * from \"./value\";\n",
		},
		{
			name: "external module reference",
			decl: "import value = require(\"@alias/value\");\n",
			want: "import value = require(\"./value\");\n",
		},
		{
			name: "import type node",
			decl: "export declare const value: import(\"@alias/value\").Value;\n",
			want: "export declare const value: import(\"./value\").Value;\n",
		},
		{
			name: "require call expression",
			decl: "const loaded = require(\"@alias/value\");\n",
			want: "const loaded = require(\"./value\");\n",
		},
		{
			name: "dynamic import call expression",
			decl: "const loaded = import(\"@alias/value\");\n",
			want: "const loaded = import(\"./value\");\n",
		},
		{
			name: "single quoted specifier is renormalized to double quotes",
			decl: "import { Value } from '@alias/value';\n",
			want: "import { Value } from \"./value\";\n",
		},
		{
			name: "several specifiers on one line",
			decl: "import { A } from \"@alias/value\"; import { B } from \"@alias/value\";\n",
			want: "import { A } from \"./value\"; import { B } from \"./value\";\n",
		},
		{
			name: "an already relative specifier resolves to itself",
			decl: "import { Value } from \"./value\";\n",
			want: "import { Value } from \"./value\";\n",
		},
		{
			name: "external library import is left alone",
			decl: "import { dummy } from \"@rbxts/dummy\";\n",
			want: "import { dummy } from \"@rbxts/dummy\";\n",
		},
		{
			name: "url specifier is left alone",
			decl: "import \"https://example.com/module\";\n",
			want: "import \"https://example.com/module\";\n",
		},
		{
			name: "unresolvable specifier is left alone",
			decl: "import { Missing } from \"@alias/missing\";\n",
			want: "import { Missing } from \"@alias/missing\";\n",
		},
		{
			name: "namespace re-export",
			decl: "export * as shared from \"@alias/value\";\n",
			want: "export * as shared from \"./value\";\n",
		},
		{
			name: "type-only import declaration",
			decl: "import type { Value } from \"@alias/value\";\n",
			want: "import type { Value } from \"./value\";\n",
		},
		{
			name: "type-only export declaration",
			decl: "export type { Value } from \"@alias/value\";\n",
			want: "export type { Value } from \"./value\";\n",
		},
		{
			name: "inline type modifier on a named import",
			decl: "import { type Value } from \"@alias/value\";\n",
			want: "import { type Value } from \"./value\";\n",
		},
		{
			// The JS transformer returned the updated ImportTypeNode without
			// descending into its type arguments, so an inner specifier kept
			// its alias spelling. Every specifier in the tree is visited here.
			name: "nested import type inside a type argument",
			decl: "export declare const nested: import(\"@alias/value\").Box<import(\"@alias/value\").Value>;\n",
			want: "export declare const nested: import(\"./value\").Box<import(\"./value\").Value>;\n",
		},
		{
			name: "query specifier is left alone",
			decl: "import \"@alias/value?raw\";\n",
			want: "import \"@alias/value?raw\";\n",
		},
		{
			name: "hash specifier is left alone",
			decl: "import \"@alias/value#fragment\";\n",
			want: "import \"@alias/value#fragment\";\n",
		},
		{
			name: "bare url specifier is left alone",
			decl: "import \"http://example.com\";\n",
			want: "import \"http://example.com\";\n",
		},
		{
			name: "a specifier-free declaration is returned unchanged",
			decl: "export declare const value: number;\n",
			want: "export declare const value: number;\n",
		},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			if got := rewriteDeclaration(t, rewriter, entry, testCase.decl); got != testCase.want {
				t.Fatalf("rewrite = %q, want %q", got, testCase.want)
			}
		})
	}
}

// The declaration emitter synthesizes `import("...")` types for INFERRED
// types, and those specifiers need never appear in the file's own import list.
// The JS transformer reached ts.resolveModuleName for them; without an
// equivalent fresh resolve they ship with their alias spelling intact.
func TestDeclarationPathRewriteResolvesSpecifiersTheFileNeverImported(t *testing.T) {
	rewriter, sourceFile := declarationRewriterFixture(t, "", map[string]string{
		"src/main.ts":        "export const main = 1;\n",
		"src/deep/nested.ts": "export interface Nested { id: number; }\n",
	})

	decl := "export declare const nested: import(\"@alias/deep/nested\").Nested;\n"
	want := "export declare const nested: import(\"./deep/nested\").Nested;\n"
	if got := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), decl); got != want {
		t.Fatalf("rewrite = %q, want %q", got, want)
	}
}

// Implicit extensions come off the RESOLVED file name; which ones count as
// implicit is an option question (allowJs, jsx, resolveJsonModule).
func TestDeclarationPathRewriteImplicitExtensions(t *testing.T) {
	t.Run("ts and d.ts are always implicit", func(t *testing.T) {
		rewriter, sourceFile := declarationRewriterFixture(t, "", map[string]string{
			"src/main.ts":      "export const main = 1;\n",
			"src/value.ts":     "export interface Value { label: string; }\n",
			"src/ambient.d.ts": "export declare const ambient: number;\n",
		})
		decl := "import \"@alias/value\";\nimport \"@alias/ambient\";\n"
		want := "import \"./value\";\nimport \"./ambient\";\n"
		if got := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), decl); got != want {
			t.Fatalf("rewrite = %q, want %q", got, want)
		}
	})

	t.Run("js is implicit only under allowJs", func(t *testing.T) {
		files := map[string]string{
			"src/main.ts":   "export const main = 1;\n",
			"src/legacy.js": "module.exports = { legacy: 1 };\n",
		}
		decl := "import \"@alias/legacy\";\n"
		rewriter, sourceFile := declarationRewriterFixture(t, ", \"allowJs\": true", files)
		if got, want := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), decl), "import \"./legacy\";\n"; got != want {
			t.Fatalf("with allowJs: rewrite = %q, want %q", got, want)
		}
		rewriter, sourceFile = declarationRewriterFixture(t, "", files)
		// Without allowJs the extension is not implicit, so it stays in the
		// rewritten specifier rather than being trimmed off.
		if got, want := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), decl), "import \"./legacy.js\";\n"; got != want {
			t.Fatalf("without allowJs: rewrite = %q, want %q", got, want)
		}
	})

	t.Run("json is implicit only under resolveJsonModule", func(t *testing.T) {
		files := map[string]string{
			"src/main.ts":   "export const main = 1;\n",
			"src/data.json": "{\"label\":\"data\"}\n",
		}
		decl := "import \"@alias/data.json\";\n"
		rewriter, sourceFile := declarationRewriterFixture(t, ", \"resolveJsonModule\": true", files)
		if got, want := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), decl), "import \"./data\";\n"; got != want {
			t.Fatalf("with resolveJsonModule: rewrite = %q, want %q", got, want)
		}
		rewriter, sourceFile = declarationRewriterFixture(t, "", files)
		if got, want := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), decl), "import \"./data.json\";\n"; got != want {
			t.Fatalf("without resolveJsonModule: rewrite = %q, want %q", got, want)
		}
	})

	t.Run("tsx is implicit only under jsx", func(t *testing.T) {
		files := map[string]string{
			"src/main.ts":    "export const main = 1;\n",
			"src/widget.tsx": "export const widget = 1;\n",
		}
		rewriter, sourceFile := declarationRewriterFixture(t, ", \"jsx\": \"react\"", files)
		decl := "import \"@alias/widget\";\n"
		if got, want := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), decl), "import \"./widget\";\n"; got != want {
			t.Fatalf("with jsx: rewrite = %q, want %q", got, want)
		}
	})
}

// rootDirs makes several directories one virtual directory, so a specifier
// that crosses them must not grow a `../` chain.
func TestDeclarationPathRewriteRootDirs(t *testing.T) {
	files := map[string]string{
		"src/main.ts":  "export const main = 1;\n",
		"src/value.ts": "export interface Value { label: string; }\n",
		"test/spec.ts": "export const spec = 1;\n",
	}
	decl := "import { Value } from \"@alias/value\";\n"

	rewriter, sourceFile := declarationRewriterFixture(t, "", files)
	if got, want := rewriteDeclaration(t, rewriter, sourceFile("test/spec.ts"), decl), "import { Value } from \"../src/value\";\n"; got != want {
		t.Fatalf("without rootDirs: rewrite = %q, want %q", got, want)
	}

	rewriter, sourceFile = declarationRewriterFixture(t, ", \"rootDirs\": [\"src\", \"test\"]", files)
	if got, want := rewriteDeclaration(t, rewriter, sourceFile("test/spec.ts"), decl), "import { Value } from \"./value\";\n"; got != want {
		t.Fatalf("with rootDirs: rewrite = %q, want %q", got, want)
	}
}

// Node's win32 path.relative compares case-insensitively; Go's filepath.Rel
// does not, and using it would emit a bogus `../` chain for a differently
// cased directory component on Windows.
func TestDeclarationPathRewriteRelativeIsCaseAware(t *testing.T) {
	insensitive := &declarationPathRewriter{compare: tspath.ComparePathsOptions{UseCaseSensitiveFileNames: false}}
	if got, want := insensitive.relativeFrom("D:/Project/Src", "d:/project/src/value.ts"), "value.ts"; got != want {
		t.Fatalf("case-insensitive relative = %q, want %q", got, want)
	}
	sensitive := &declarationPathRewriter{compare: tspath.ComparePathsOptions{UseCaseSensitiveFileNames: true}}
	if got, want := sensitive.relativeFrom("/project/Src", "/project/src/value.ts"), "../src/value.ts"; got != want {
		t.Fatalf("case-sensitive relative = %q, want %q", got, want)
	}
	if got := insensitive.relativeFrom("D:/project/src", "D:/project/src"); got != "" {
		t.Fatalf("relative to itself = %q, want %q", got, "")
	}
}

// A project with neither baseUrl nor paths can carry no alias spelling, so the
// emit path must not pay for a reparse per declaration file.
func TestDeclarationPathRewriterIsNilWithoutPathMappings(t *testing.T) {
	dir := writeProject(t, "@scope/no-path-mappings", "")
	_, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (diags: %v)", err, diags)
	}
	if newDeclarationPathRewriter(program) != nil {
		t.Fatal("newDeclarationPathRewriter returned a rewriter for a project with no baseUrl or paths")
	}
}

// A `.d.ts` sitting next to a `.ts` of the same name is what resolution picks,
// and the rewritten specifier must name the module, not the file that lost.
func TestDeclarationPathRewriteDeclarationFileWinsOverSource(t *testing.T) {
	rewriter, sourceFile := declarationRewriterFixture(t, "", map[string]string{
		"src/main.ts":   "export const main = 1;\n",
		"src/pair.ts":   "export const fromSource = 1;\n",
		"src/pair.d.ts": "export declare const fromDeclaration: number;\n",
	})
	decl := "import \"@alias/pair\";\n"
	if got, want := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), decl), "import \"./pair\";\n"; got != want {
		t.Fatalf("rewrite = %q, want %q", got, want)
	}
}

// A JSON module is only ever found through its explicit extension, even under
// resolveJsonModule — TypeScript adds `.json` to a specifier's implicit
// extension list for OUTPUT (which is why `.json` is trimmed off a rewritten
// path) but never for INPUT resolution. An extensionless specifier therefore
// does not resolve, and an unresolved specifier is left exactly as it is.
func TestDeclarationPathRewriteExtensionlessJSONSpecifier(t *testing.T) {
	rewriter, sourceFile := declarationRewriterFixture(t, ", \"resolveJsonModule\": true", map[string]string{
		"src/main.ts":   "export const main = 1;\n",
		"src/data.json": "{\"label\":\"data\"}\n",
	})
	decl := "import \"@alias/data\";\n"
	if got := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), decl); got != decl {
		t.Fatalf("rewrite = %q, want it left alone (%q)", got, decl)
	}
	// The same module WITH its extension resolves, and `.json` comes off.
	withExtension := "import \"@alias/data.json\";\n"
	if got, want := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), withExtension), "import \"./data\";\n"; got != want {
		t.Fatalf("rewrite = %q, want %q", got, want)
	}
}

// Under `module: preserve` / `moduleResolution: bundler` a package with an
// `exports` map can be resolved under more than one mode, and the resolution
// cache is keyed by (name, mode). Looking a specifier up by name alone meant
// taking whichever entry Go's randomized map iteration reached first, which
// churned the emitted text — and the incremental hash — between builds.
func TestDeclarationPathRewriteIsDeterministicUnderConditionalExports(t *testing.T) {
	rewriter, sourceFile := declarationRewriterFixture(t, ", \"module\": \"preserve\", \"moduleResolution\": \"bundler\"", map[string]string{
		"src/main.ts":                               "import \"@alias/value\";\nimport \"conditional\";\nexport const main = 1;\n",
		"src/value.ts":                              "export interface Value { label: string; }\n",
		"node_modules/conditional/package.json":     `{"name":"conditional","exports":{".":{"types":"./types/index.d.ts","import":"./esm/index.js","require":"./cjs/index.js"}}}`,
		"node_modules/conditional/types/index.d.ts": "export declare const conditional: number;\n",
		"node_modules/conditional/esm/index.js":     "export const conditional = 1;\n",
		"node_modules/conditional/cjs/index.js":     "module.exports = { conditional: 1 };\n",
	})
	entry := sourceFile("src/main.ts")
	decl := "import \"@alias/value\";\nimport \"conditional\";\n"

	first := rewriteDeclaration(t, rewriter, entry, decl)
	for run := 0; run < 50; run++ {
		if got := rewriteDeclaration(t, rewriter, entry, decl); got != first {
			t.Fatalf("run %d produced %q, want %q from run 0", run, got, first)
		}
	}
	if !strings.Contains(first, `"./value"`) {
		t.Fatalf("path alias was not rewritten: %q", first)
	}
}

// A resolved path is free to contain a quote or a backslash on a POSIX
// filesystem; splicing one in raw would produce a `.d.ts` that no longer
// parses.
func TestQuoteSpecifier(t *testing.T) {
	for _, testCase := range []struct {
		name string
		in   string
		want string
	}{
		{name: "ordinary path", in: "./value", want: `"./value"`},
		{name: "quote", in: `./va"lue`, want: `"./va\"lue"`},
		{name: "backslash", in: `./va\lue`, want: `"./va\\lue"`},
		{name: "both", in: `./a"b\c`, want: `"./a\"b\\c"`},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			if got := quoteSpecifier(testCase.in); got != testCase.want {
				t.Fatalf("quoteSpecifier(%q) = %s, want %s", testCase.in, got, testCase.want)
			}
		})
	}
}

// The escaping has to survive a round trip: the spliced text is a `.d.ts` that
// tsc will read back, so it must still parse and still carry the exact path.
func TestQuoteSpecifierSurvivesAReparse(t *testing.T) {
	const specifier = `./awkward"dir\name/value`
	declText := "import { Value } from \"@alias/value\";\n"
	start := strings.Index(declText, `"@alias/value"`)
	spliced := applyTextEdits(declText, []textEdit{{start: start, end: start + len(`"@alias/value"`), text: quoteSpecifier(specifier)}})

	parsed := parser.ParseSourceFile(
		ast.SourceFileParseOptions{FileName: "/spliced.d.ts", Path: "/spliced.d.ts"},
		spliced,
		core.ScriptKindTS,
	)
	if parsed == nil {
		t.Fatalf("spliced declaration did not parse:\n%s", spliced)
	}
	if len(parsed.Diagnostics()) > 0 {
		t.Fatalf("spliced declaration has parse diagnostics %v:\n%s", parsed.Diagnostics(), spliced)
	}
	var found string
	var visit func(node *ast.Node) bool
	visit = func(node *ast.Node) bool {
		if node.Kind == ast.KindImportDeclaration {
			found = node.AsImportDeclaration().ModuleSpecifier.Text()
		}
		node.ForEachChild(visit)
		return false
	}
	parsed.AsNode().ForEachChild(visit)
	if found != specifier {
		t.Fatalf("reparsed specifier = %q, want %q (text: %s)", found, specifier, spliced)
	}
}

// The end-to-end shape of the same guard. Only a POSIX filesystem accepts a
// quote in a directory name, so Windows cannot host the fixture.
func TestDeclarationPathRewriteQuoteInDirectoryName(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("a Windows path cannot contain a quote")
	}
	rewriter, sourceFile := declarationRewriterFixture(t, "", map[string]string{
		"src/main.ts":       "export const main = 1;\n",
		`src/od"d/value.ts`: "export interface Value { label: string; }\n",
	})
	decl := "import \"@alias/od\\\"d/value\";\n"
	got := rewriteDeclaration(t, rewriter, sourceFile("src/main.ts"), decl)
	if !strings.Contains(got, `\"`) {
		t.Fatalf("quote in the resolved path was not escaped: %s", got)
	}
}

// The text splice and the map fixup must see the SAME edits. A specifier
// literal that itself contains `types="types"` produces two edits that overlap;
// whichever one is dropped, both consumers have to agree on it.
func TestNonOverlappingTextEdits(t *testing.T) {
	edits := []textEdit{
		{start: 10, end: 20, text: "aaa"},
		{start: 0, end: 5, text: "b"},
		{start: 12, end: 16, text: "dropped"},
		{start: 20, end: 25, text: "c"},
	}
	kept := nonOverlappingTextEdits(edits)
	if len(kept) != 3 {
		t.Fatalf("kept %d edits, want 3: %+v", len(kept), kept)
	}
	wantStarts := []int{0, 10, 20}
	for index, edit := range kept {
		if edit.start != wantStarts[index] {
			t.Fatalf("kept starts = %+v, want %v", kept, wantStarts)
		}
	}
}

// The invariant the filtering exists for: after it, the length the map fixup
// believes the text changed by is the length it actually changed by.
func TestOverlappingEditsKeepTextAndMapAgreed(t *testing.T) {
	// A specifier whose text happens to contain the type-reference marker.
	const declText = "import \"@alias/types=\\\"types\\\"/value\";\n"
	specifierStart := strings.Index(declText, `"@alias/`)
	specifierEnd := strings.Index(declText, `;`)
	markerStart := strings.Index(declText, `types="types"`)
	edits := nonOverlappingTextEdits([]textEdit{
		{start: markerStart, end: markerStart + len(`types="types"`), text: `types="@rbxts/types"`},
		{start: specifierStart, end: specifierEnd, text: `"./value"`},
	})
	if len(edits) != 1 {
		t.Fatalf("overlapping edits were not collapsed: %+v", edits)
	}

	rewritten := applyTextEdits(declText, edits)
	shifts := declarationColumnShifts(declText, edits, 0)
	total := 0
	for _, lineShifts := range shifts {
		for _, shift := range lineShifts {
			total += shift.delta
		}
	}
	if got := len(rewritten) - len(declText); got != total {
		t.Fatalf("text changed by %d but the map was shifted by %d", got, total)
	}
}
