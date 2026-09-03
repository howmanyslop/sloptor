package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/tsgo/ast"
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
	return applyTextEdits(declText, rewriter.specifierEdits(entry, declFileName, declText))
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
