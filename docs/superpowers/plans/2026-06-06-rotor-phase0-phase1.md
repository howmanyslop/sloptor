# rotor Phase 0 + Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the rotor repo, vendor typescript-go as an importable mirror, prove we can drive its TypeChecker from Go (Phase 0), then port roblox-ts's `@roblox-ts/luau-ast` package — Luau AST, factories, and renderer — with exact output fidelity (Phase 1).

**Architecture:** Single Go module `rotor`. typescript-go's `internal/` packages are mirrored into `tsgo/` by a vendoring tool that rewrites import paths (Apache-2.0, attribution retained). rotor's own code lives under `internal/`. The Luau AST is a struct-per-node design with kind-range type guards, parent pointers with clone-on-reparent semantics (faithful to upstream `create()`), and a doubly-linked statement list. The renderer is a faithful port that returns strings; performance optimization happens only after differential tests exist (Phase 2+).

**Tech Stack:** Go ≥ 1.25 (match tsgo's go.mod), git, standard library only (no test frameworks). Reference sources vendored under `reference/` (MIT).

**Spec:** `docs/superpowers/specs/2026-06-05-rotor-design.md`. This plan covers Phase 0 and Phase 1 only; Phases 2–5 get their own plans.

---

## File Structure

```text
rotor/
├── go.mod                          module rotor (rename before publishing)
├── .gitattributes                  force LF on .lua/.luau/golden fixtures
├── .gitignore
├── README.md
├── tools/mirror/main.go            tsgo vendoring tool
├── tsgo/                           GENERATED mirror of typescript-go internal/ (committed)
│   ├── LICENSE  NOTICE  MIRROR.md  (attribution + pinned SHA + statement of changes)
│   └── compiler/ checker/ ast/ ...  (rewritten import paths)
├── reference/                      vendored reference sources (committed, MIT)
│   ├── VERSIONS.md                 source repos + SHAs
│   ├── roblox-ts/                  v3.0.0
│   └── luau-ast/                   @roblox-ts/luau-ast 2.0.0
├── internal/
│   ├── spike/                      Phase 0 checker spike (test only)
│   │   ├── testdata/spike/{tsconfig.json, src/main.ts}
│   │   └── spike_test.go
│   └── luau/                       Luau AST package
│       ├── kind.go  kind_string.go node.go  list.go  nodes.go  create.go
│       ├── guards.go  validate.go  globals.go  strings.go
│       ├── *_test.go
│       └── render/                 Luau renderer package
│           ├── state.go  render.go  visit.go  solvetempids.go
│           ├── expressions.go  statements.go  fields.go
│           ├── ending.go  parens.go  util.go
│           └── *_test.go
```

Reference source roots used throughout (paths relative to `reference/luau-ast/src/`):

- AST: `LuauAST/types/nodes.ts`, `LuauAST/types/operators.ts`, `LuauAST/impl/{enums,create,List,typeGuards,globals,strings}.ts`, `LuauAST/util/*.ts`
- Renderer: `LuauRenderer/render.ts`, `LuauRenderer/RenderState.ts`, `LuauRenderer/solveTempIds.ts`, `LuauRenderer/nodes/**/*.ts`, `LuauRenderer/util/*.ts`

---

## Phase 0 — Foundation

### Task 1: Repo scaffolding

**Files:**

- Create: `go.mod`, `.gitattributes`, `.gitignore`, `README.md`

- [ ] **Step 1: Verify Go toolchain**

Run: `go version`
Expected: go1.25 or newer. If older, install latest Go before continuing (tsgo requires a recent toolchain; check `reference` later — the mirror task re-verifies against tsgo's own go.mod).

- [ ] **Step 2: Create go.mod**

Run: `go mod init rotor`

Note: single-segment module path is deliberate for local development; rename to a hosted path (one `go.mod` edit + goimports rewrite) before publishing.

- [ ] **Step 3: Create .gitattributes**

```gitattributes
* text=auto
*.go text eol=lf
*.lua text eol=lf
*.luau text eol=lf
*.ts text eol=lf
*.json text eol=lf
```

LF is forced on Lua/TS fixtures so golden comparisons are byte-stable on Windows.

- [ ] **Step 4: Create .gitignore**

```gitignore
*.exe
*.test
/tmp/
```

- [ ] **Step 5: Create README.md**

```markdown
# rotor

Native-speed drop-in replacement for the roblox-ts compiler (`rbxtsc`), written in Go
on top of [typescript-go](https://github.com/microsoft/typescript-go).

Goal: 1:1 output compatibility with roblox-ts 3.0.0 — same `@rbxts/*` packages, same
CLI, byte-identical Luau output — at ~10x the speed.

See `docs/superpowers/specs/2026-06-05-rotor-design.md` for the design.

## Layout
- `tsgo/` — generated mirror of typescript-go internals (do not edit; run `go run ./tools/mirror`)
- `reference/` — pinned roblox-ts sources we port from and diff against
- `internal/luau` — Luau AST + renderer (port of @roblox-ts/luau-ast)
```

- [ ] **Step 6: Commit**

```powershell
git add -A && git commit -m "Scaffold rotor Go module"
```

### Task 2: Vendor reference sources

**Files:**

- Create: `reference/roblox-ts/` (from <https://github.com/roblox-ts/roblox-ts> at tag `v3.0.0`)
- Create: `reference/luau-ast/` (from <https://github.com/roblox-ts/luau-ast> at the commit where `package.json` version is `2.0.0`)
- Create: `reference/VERSIONS.md`

Shallow clones may already exist at `C:\Users\user\AppData\Local\Temp\roblox-ts-research` and `C:\Users\user\AppData\Local\Temp\roblox-ts-luau-ast` — do NOT reuse them blindly; they are default-branch HEADs, not the pinned versions. Clone fresh at the right refs.

- [ ] **Step 1: Clone roblox-ts at v3.0.0**

```powershell
git clone --depth 1 --branch v3.0.0 https://github.com/roblox-ts/roblox-ts $env:TEMP\rotor-ref-roblox-ts
```

If the tag name differs (list with `git ls-remote --tags https://github.com/roblox-ts/roblox-ts`), use the tag whose `package.json` version field is `3.0.0`.

- [ ] **Step 2: Clone luau-ast at version 2.0.0**

```powershell
git clone https://github.com/roblox-ts/luau-ast $env:TEMP\rotor-ref-luau-ast
```

Check `package.json` version at HEAD. If it is not `2.0.0`, find the right commit: `git log --oneline -p -- package.json | Select-String -Context 5 '"version"'` and `git checkout <commit>` where version is `2.0.0`. Prefer a `v2.0.0` tag if one exists.

- [ ] **Step 3: Copy into reference/ and record versions**

```powershell
New-Item -ItemType Directory -Force reference | Out-Null
Copy-Item -Recurse $env:TEMP\rotor-ref-roblox-ts reference\roblox-ts
Copy-Item -Recurse $env:TEMP\rotor-ref-luau-ast reference\luau-ast
# record SHAs BEFORE deleting .git
$rtsSha = git -C reference\roblox-ts rev-parse HEAD
$luauSha = git -C reference\luau-ast rev-parse HEAD
Remove-Item -Recurse -Force reference\roblox-ts\.git, reference\luau-ast\.git
@"
# Vendored reference sources (MIT, see each package's LICENSE)
- roblox-ts v3.0.0 — https://github.com/roblox-ts/roblox-ts @ $rtsSha
- @roblox-ts/luau-ast 2.0.0 — https://github.com/roblox-ts/luau-ast @ $luauSha
"@ | Set-Content reference\VERSIONS.md
```

- [ ] **Step 4: Verify LICENSE files exist**

Run: `Test-Path reference\roblox-ts\LICENSE; Test-Path reference\luau-ast\LICENSE`
Expected: both `True`. If luau-ast has no LICENSE file, copy roblox-ts's MIT LICENSE text with the roblox-ts org attribution into `reference\luau-ast\LICENSE`.

- [ ] **Step 5: Commit**

```powershell
git add reference && git commit -m "Vendor reference sources: roblox-ts v3.0.0, luau-ast 2.0.0"
```

### Task 3: tsgo mirror tool

**Files:**

- Create: `tools/mirror/main.go`

No unit test — this is a build tool; Task 4 is its verification.

- [ ] **Step 1: Write the mirror tool**

```go
// Command mirror vendors microsoft/typescript-go's internal packages into ./tsgo
// with import paths rewritten so they are importable from the rotor module.
//
// Usage: go run ./tools/mirror [-ref main] [-repo URL]
package main

import (
	"bytes"
	"flag"
	"fmt"
	"io/fs"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"
)

const (
	srcModule = "github.com/microsoft/typescript-go/internal/"
	dstModule = "rotor/tsgo/"
	outDir    = "tsgo"
)

func main() {
	repo := flag.String("repo", "https://github.com/microsoft/typescript-go", "source repo")
	ref := flag.String("ref", "main", "git ref to vendor")
	flag.Parse()

	tmp, err := os.MkdirTemp("", "tsgo-mirror-*")
	if err != nil {
		log.Fatal(err)
	}
	defer os.RemoveAll(tmp)

	run(tmp, "git", "init", "-q")
	run(tmp, "git", "remote", "add", "origin", *repo)
	run(tmp, "git", "fetch", "-q", "--depth", "1", "origin", *ref)
	run(tmp, "git", "checkout", "-q", "FETCH_HEAD")
	sha := strings.TrimSpace(output(tmp, "git", "rev-parse", "HEAD"))

	if err := os.RemoveAll(outDir); err != nil {
		log.Fatal(err)
	}

	srcRoot := filepath.Join(tmp, "internal")
	nFiles := 0
	err = filepath.WalkDir(srcRoot, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, _ := filepath.Rel(srcRoot, path)
		if d.IsDir() {
			// skip test fixtures wholesale
			if d.Name() == "testdata" {
				return filepath.SkipDir
			}
			return os.MkdirAll(filepath.Join(outDir, rel), 0o755)
		}
		if strings.HasSuffix(d.Name(), "_test.go") {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		if strings.HasSuffix(d.Name(), ".go") {
			data = bytes.ReplaceAll(data, []byte(srcModule), []byte(dstModule))
		}
		nFiles++
		return os.WriteFile(filepath.Join(outDir, rel), data, 0o644)
	})
	if err != nil {
		log.Fatal(err)
	}

	// Apache-2.0 obligations: license, notice, statement of changes.
	copyFile(filepath.Join(tmp, "LICENSE"), filepath.Join(outDir, "LICENSE"))
	copyFile(filepath.Join(tmp, "NOTICE.txt"), filepath.Join(outDir, "NOTICE"))
	mirrorMD := fmt.Sprintf(`# Mirror of microsoft/typescript-go internals

- Source: %s
- Commit: %s
- Vendored: %s
- Changes: files copied from internal/ with import paths rewritten
  ("%s" -> "%s"); *_test.go files and testdata/ directories omitted.
  No other modifications. Regenerate with: go run ./tools/mirror
`, *repo, sha, time.Now().UTC().Format(time.RFC3339), srcModule, dstModule)
	if err := os.WriteFile(filepath.Join(outDir, "MIRROR.md"), []byte(mirrorMD), 0o644); err != nil {
		log.Fatal(err)
	}

	fmt.Printf("mirrored %d files at %s\nnow run: go mod tidy && go build ./tsgo/...\n", nFiles, sha)
}

func run(dir string, name string, args ...string) {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	cmd.Stdout, cmd.Stderr = os.Stdout, os.Stderr
	if err := cmd.Run(); err != nil {
		log.Fatalf("%s %v: %v", name, args, err)
	}
}

func output(dir string, name string, args ...string) string {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	out, err := cmd.Output()
	if err != nil {
		log.Fatalf("%s %v: %v", name, args, err)
	}
	return string(out)
}

func copyFile(src, dst string) {
	data, err := os.ReadFile(src)
	if err != nil {
		// NOTICE may not exist under that exact name; try NOTICE without extension
		alt := strings.TrimSuffix(src, ".txt")
		if data2, err2 := os.ReadFile(alt); err2 == nil {
			data = data2
		} else {
			log.Printf("warning: could not copy %s: %v", src, err)
			return
		}
	}
	if err := os.WriteFile(dst, data, 0o644); err != nil {
		log.Fatal(err)
	}
}
```

- [ ] **Step 2: Verify it compiles**

Run: `go vet ./tools/mirror`
Expected: no output (success).

- [ ] **Step 3: Commit**

```powershell
git add tools && git commit -m "Add tsgo mirror tool"
```

### Task 4: Run the mirror and build tsgo

**Files:**

- Create (generated): `tsgo/**`
- Modify: `go.mod`, `go.sum` (tsgo's dependencies)
- [ ] **Step 1: Run the mirror**

Run: `go run ./tools/mirror`
Expected: `mirrored N files at <sha>` (N in the thousands).

- [ ] **Step 2: Check Go version requirement**

Look at the `go` directive the mirror needs: open the cloned repo's go.mod requirement — simplest is `Select-String -Path tsgo\..\go.mod` — no: tsgo/ has no go.mod (it's part of our module). Instead check the upstream requirement printed during `go mod tidy` failures, or pre-check: `(Invoke-WebRequest https://raw.githubusercontent.com/microsoft/typescript-go/main/go.mod).Content | Select-String '^go '`. If it exceeds your toolchain, upgrade Go and update rotor's go.mod `go` directive to match.

- [ ] **Step 3: Tidy and build**

Run: `go mod tidy` then `go build ./tsgo/...`
Expected: tidy pulls tsgo's deps (dlclark/regexp2, go-json-experiment/json, etc.); build succeeds.

**Contingencies (work through in order if the build fails):**

1. `//go:embed` errors in `tsgo/bundled/...` about missing files → the embedded lib files were excluded or live outside `internal/`. Check upstream `internal/bundled/` layout; copy any non-Go asset dirs it embeds (e.g. `libs/`) into the same relative spot under `tsgo/` (extend the mirror tool if they sit outside `internal/`, then re-run it — don't hand-copy).
2. Imports referencing `github.com/microsoft/typescript-go/` WITHOUT `/internal/` (rare) → extend the mirror tool's rewrite to handle those paths, re-run.
3. Build-tag-gated files failing on Windows → check whether upstream builds those packages on Windows; if a package is irrelevant to compilation (e.g. fuzzing helpers), extend the mirror tool to skip that directory, re-run, and document the skip in MIRROR.md's changes list.
4. Generated-code staleness (`go:generate` references) → ignore; we never regenerate inside the mirror.

- [ ] **Step 4: Verify the packages rotor needs exist**

Run: `go list ./tsgo/compiler ./tsgo/checker ./tsgo/ast ./tsgo/tsoptions ./tsgo/vfs/osvfs ./tsgo/bundled ./tsgo/core`
Expected: all seven paths print. These are the packages the spike (Task 5) imports.

- [ ] **Step 5: Commit**

```powershell
git add -A && git commit -m "Vendor typescript-go mirror"
```

(This is a large commit by design; the mirror is regenerable and pinned in MIRROR.md.)

### Task 5: Checker spike — drive tsgo's TypeChecker from Go

**Files:**

- Create: `internal/spike/testdata/spike/tsconfig.json`
- Create: `internal/spike/testdata/spike/src/main.ts`
- Test: `internal/spike/spike_test.go`

This is the de-risking spike. The code below is written against the API shapes confirmed by research (June 2026); exact signatures may have drifted — when something doesn't compile, **read the mirrored source in `tsgo/` and adjust the call site**, do not guess. The test's assertions are the contract; the plumbing may flex.

- [ ] **Step 1: Create the fixture project**

`internal/spike/testdata/spike/tsconfig.json`:

```json
{
	"compilerOptions": {
		"target": "ESNext",
		"module": "CommonJS",
		"strict": true,
		"types": []
	},
	"include": ["src"]
}
```

`internal/spike/testdata/spike/src/main.ts`:

```typescript
const greeting = "hello";
const count = 42;
const items = [1, 2, 3];
const lookup = new Map<string, number>();

function add(a: number, b: number) {
	return a + b;
}

const total = add(count, items.size ? 0 : 1);
```

Note: `items.size` is intentionally an error-free property probe ONLY if it type-checks — it will NOT under plain lib types. Remove that line; keep the fixture diagnostic-free:

```typescript
const greeting = "hello";
const count = 42;
const items = [1, 2, 3];
const lookup = new Map<string, number>();

function add(a: number, b: number) {
	return a + b;
}

const total = add(count, items.length);
```

- [ ] **Step 2: Write the failing spike test**

`internal/spike/spike_test.go`:

```go
package spike

import (
	"context"
	"path/filepath"
	"testing"
	"time"

	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/vfs/osvfs"
)

func TestCheckerSpike(t *testing.T) {
	start := time.Now()

	dir, err := filepath.Abs(filepath.Join("testdata", "spike"))
	if err != nil {
		t.Fatal(err)
	}
	dir = filepath.ToSlash(dir)

	fs := bundled.WrapFS(osvfs.FS())
	host := compiler.NewCompilerHost(dir, fs, bundled.LibPath(), nil, nil)

	configPath := dir + "/tsconfig.json"
	parsed, diags := tsoptions.GetParsedCommandLineOfConfigFile(configPath, nil, nil, host, nil)
	if len(diags) > 0 {
		t.Fatalf("config diagnostics: %v", diags)
	}

	program := compiler.NewProgram(compiler.ProgramOptions{
		Host:   host,
		Config: parsed,
	})

	ctx := context.Background()
	if semDiags := program.GetSemanticDiagnostics(ctx, nil); len(semDiags) > 0 {
		for _, d := range semDiags {
			t.Errorf("unexpected diagnostic: %v", d.Message())
		}
		t.FatalF("fixture must be diagnostic-free")
	}

	checker, release := program.GetTypeChecker(ctx)
	defer release()

	sf := program.GetSourceFile(dir + "/src/main.ts")
	if sf == nil {
		t.Fatal("source file not found")
	}

	// Collect the initializer type of each top-level `const`.
	got := map[string]string{}
	for _, stmt := range sf.Statements.Nodes {
		if stmt.Kind != ast.KindVariableStatement {
			continue
		}
		declList := stmt.AsVariableStatement().DeclarationList.AsVariableDeclarationList()
		for _, decl := range declList.Declarations.Nodes {
			d := decl.AsVariableDeclaration()
			name := d.Name().Text()
			typ := checker.GetTypeAtLocation(decl)
			got[name] = checker.TypeToString(typ)
		}
	}

	want := map[string]string{
		"greeting": `"hello"`,
		"count":    "42",
		"items":    "number[]",
		"lookup":   "Map<string, number>",
		"total":    "number",
	}
	for name, w := range want {
		if got[name] != w {
			t.Errorf("type of %s = %q, want %q", name, got[name], w)
		}
	}

	t.Logf("program create + check + query: %s", time.Since(start))
}
```

- [ ] **Step 3: Iterate until it compiles**

Run: `go test ./internal/spike/ -run TestCheckerSpike -v`
Expected first runs: compile errors. For each, open the mirrored source (e.g. `tsgo/compiler/host.go`, `tsgo/compiler/program.go`, `tsgo/tsoptions/`, `tsgo/ast/`) and fix the call site to the real signature. Known likely drift points: `NewCompilerHost` arity, `ProgramOptions` field names, `GetSemanticDiagnostics` signature, AST accessor names (`AsVariableStatement`, `Name().Text()`), `TypeToString` location (may need `checker.TypeToStringEx` or be on a different receiver).

- [ ] **Step 4: Iterate until assertions pass**

Run: `go test ./internal/spike/ -run TestCheckerSpike -v`
Expected: PASS, with a logged duration. Literal-type strings (`"hello"`, `42`) may render differently (e.g. widened) depending on how `GetTypeAtLocation` treats the declaration node — if so, adjust the fixture/want table to whatever the REAL `tsc` would report (verify with `npx tsc` + hover semantics or the playground; the contract is "tsgo gives real checker answers", not specific literal widening).

- [ ] **Step 5: Commit**

```powershell
git add internal go.mod go.sum && git commit -m "Phase 0 spike: drive tsgo TypeChecker from Go"
```

**Phase 0 exit criterion: this test passing proves the core bet of the whole project.**

---

## Phase 1 — Luau AST + Renderer (port of @roblox-ts/luau-ast 2.0.0)

General porting rules for every task below:

- Reference TS file paths are given per task; port behavior **exactly** — including quirks (e.g. `getKindName` special cases, `renderNumberLiteral` underscore handling). Do not "improve" output.
- Upstream `assert(...)` becomes `panic(...)` with the same message; these indicate compiler bugs, not user errors.
- All output uses `\t` for indent and `\n` for newlines — never `\r\n`.

### Task 6: SyntaxKind + Node interfaces

**Files:**

- Create: `internal/luau/kind.go`, `internal/luau/node.go`
- Test: `internal/luau/kind_test.go`
- Reference: `LuauAST/impl/enums.ts`, `LuauAST/types/nodes.ts` (base types), `LuauAST/util/getKindName.ts`, `LuauAST/types/operators.ts`
- [ ] **Step 1: Write the failing test**

`internal/luau/kind_test.go`:

```go
package luau

import "testing"

func TestKindOrdering(t *testing.T) {
	// category ranges must mirror upstream enums.ts exactly
	if FirstIndexableExpression != KindIdentifier || LastIndexableExpression != KindParenthesizedExpression {
		t.Error("indexable expression range wrong")
	}
	if FirstExpression != KindIdentifier || LastExpression != KindMixedTable {
		t.Error("expression range wrong")
	}
	if FirstStatement != KindAssignment || LastStatement != KindComment {
		t.Error("statement range wrong")
	}
	if FirstField != KindMapField || LastField != KindInterpolatedStringPart {
		t.Error("field range wrong")
	}
}

func TestKindName(t *testing.T) {
	cases := map[SyntaxKind]string{
		KindIdentifier:             "Identifier",
		KindParenthesizedExpression: "ParenthesizedExpression",
		KindSet:                    "Set",
		KindAssignment:             "Assignment",
		KindComment:                "Comment",
		KindMapField:               "MapField",
		KindInterpolatedString:     "InterpolatedString",
		KindNumericForStatement:    "NumericForStatement",
	}
	for k, want := range cases {
		if got := k.String(); got != want {
			t.Errorf("kind %d String() = %q, want %q", k, got, want)
		}
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/luau/ -v`
Expected: FAIL (package doesn't compile — types undefined).

- [ ] **Step 3: Implement kind.go and node.go**

`internal/luau/kind.go`:

```go
package luau

type SyntaxKind uint8

const (
	// indexable expressions
	KindIdentifier SyntaxKind = iota
	KindTemporaryIdentifier
	KindComputedIndexExpression
	KindPropertyAccessExpression
	KindCallExpression
	KindMethodCallExpression
	KindParenthesizedExpression

	// expressions
	KindNone
	KindNilLiteral
	KindFalseLiteral
	KindTrueLiteral
	KindNumberLiteral
	KindStringLiteral
	KindVarArgsLiteral
	KindFunctionExpression
	KindBinaryExpression
	KindUnaryExpression
	KindIfExpression
	KindInterpolatedString
	KindArray
	KindMap
	KindSet
	KindMixedTable

	// statements
	KindAssignment
	KindBreakStatement
	KindCallStatement
	KindContinueStatement
	KindDoStatement
	KindWhileStatement
	KindRepeatStatement
	KindIfStatement
	KindNumericForStatement
	KindForStatement
	KindFunctionDeclaration
	KindMethodDeclaration
	KindVariableDeclaration
	KindReturnStatement
	KindComment

	// fields
	KindMapField
	KindInterpolatedStringPart

	FirstIndexableExpression = KindIdentifier
	LastIndexableExpression  = KindParenthesizedExpression
	FirstExpression          = KindIdentifier
	LastExpression           = KindMixedTable
	FirstStatement           = KindAssignment
	LastStatement            = KindComment
	FirstField               = KindMapField
	LastField                = KindInterpolatedStringPart
)

var kindNames = [...]string{
	KindIdentifier:              "Identifier",
	KindTemporaryIdentifier:     "TemporaryIdentifier",
	KindComputedIndexExpression: "ComputedIndexExpression",
	KindPropertyAccessExpression: "PropertyAccessExpression",
	KindCallExpression:          "CallExpression",
	KindMethodCallExpression:    "MethodCallExpression",
	KindParenthesizedExpression: "ParenthesizedExpression",
	KindNone:                    "None",
	KindNilLiteral:              "NilLiteral",
	KindFalseLiteral:            "FalseLiteral",
	KindTrueLiteral:             "TrueLiteral",
	KindNumberLiteral:           "NumberLiteral",
	KindStringLiteral:           "StringLiteral",
	KindVarArgsLiteral:          "VarArgsLiteral",
	KindFunctionExpression:      "FunctionExpression",
	KindBinaryExpression:        "BinaryExpression",
	KindUnaryExpression:         "UnaryExpression",
	KindIfExpression:            "IfExpression",
	KindInterpolatedString:      "InterpolatedString",
	KindArray:                   "Array",
	KindMap:                     "Map",
	KindSet:                     "Set",
	KindMixedTable:              "MixedTable",
	KindAssignment:              "Assignment",
	KindBreakStatement:          "BreakStatement",
	KindCallStatement:           "CallStatement",
	KindContinueStatement:       "ContinueStatement",
	KindDoStatement:             "DoStatement",
	KindWhileStatement:          "WhileStatement",
	KindRepeatStatement:         "RepeatStatement",
	KindIfStatement:             "IfStatement",
	KindNumericForStatement:     "NumericForStatement",
	KindForStatement:            "ForStatement",
	KindFunctionDeclaration:     "FunctionDeclaration",
	KindMethodDeclaration:       "MethodDeclaration",
	KindVariableDeclaration:     "VariableDeclaration",
	KindReturnStatement:         "ReturnStatement",
	KindComment:                 "Comment",
	KindMapField:                "MapField",
	KindInterpolatedStringPart:  "InterpolatedStringPart",
}

func (k SyntaxKind) String() string { return kindNames[k] }

type BinaryOperator string     // "+" "-" "*" "/" "//" "^" "%" ".." "<" "<=" ">" ">=" "==" "~=" "and" "or"
type UnaryOperator string      // "-" "not" "#"
type AssignmentOperator string // "=" "+=" "-=" "*=" "/=" "//=" "%=" "^=" "..="
```

`internal/luau/node.go`:

```go
package luau

// NodeOrList is the union of any AST node and any *List[T] — mirrors upstream
// fields typed `luau.X | luau.List<luau.Y>`.
type NodeOrList interface{ nodeOrList() }

type Node interface {
	NodeOrList
	Kind() SyntaxKind
	Parent() Node
	setParent(Node)
	shallowClone() Node
}

// Marker interfaces mirror upstream category types.
type Expression interface {
	Node
	expressionNode()
}

type IndexableExpression interface {
	Expression
	indexableNode()
}

type Statement interface {
	Node
	statementNode()
}

type FieldNode interface {
	Node
	fieldNode()
}

// AnyIdentifier = Identifier | TemporaryIdentifier
type AnyIdentifier interface {
	IndexableExpression
	anyIdentifierNode()
}

// WritableExpression = AnyIdentifier | PropertyAccessExpression | ComputedIndexExpression
type WritableExpression interface {
	Expression
	writableNode()
}

// HasParameters mirrors the upstream HasParameters interface.
type HasParameters interface {
	Node
	ParamData() (params *List[AnyIdentifier], hasDotDotDot bool)
}

// base is embedded in every node struct.
type base struct{ parent Node }

func (b *base) Parent() Node      { return b.parent }
func (b *base) setParent(p Node)  { b.parent = p }
func (b *base) nodeOrList()       {}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/luau/ -v`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```powershell
git add internal\luau && git commit -m "luau: SyntaxKind enum and node interfaces"
```

### Task 7: List

**Files:**

- Create: `internal/luau/list.go`
- Test: `internal/luau/list_test.go`
- Reference: `LuauAST/impl/List.ts`
- [ ] **Step 1: Write the failing test**

`internal/luau/list_test.go` (uses `*Identifier` from Task 8 — define a minimal placeholder node in the test if Task 8 isn't done yet; the executor of Task 8 deletes the placeholder):

```go
package luau

import "testing"

// placeholder until nodes.go lands (Task 8); delete then.
type testNode struct {
	base
	v int
}

func (*testNode) Kind() SyntaxKind { return KindIdentifier }
func (n *testNode) shallowClone() Node { c := *n; return &c }

func tn(v int) *testNode { return &testNode{v: v} }

func values(l *List[*testNode]) []int {
	out := []int{}
	l.ForEach(func(n *testNode) { out = append(out, n.v) })
	return out
}

func eq(a, b []int) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestListPushShiftUnshift(t *testing.T) {
	l := NewList[*testNode]()
	if !l.IsEmpty() {
		t.Fatal("new list not empty")
	}
	l.Push(tn(2))
	l.Push(tn(3))
	l.Unshift(tn(1))
	if got := values(l); !eq(got, []int{1, 2, 3}) {
		t.Fatalf("got %v", got)
	}
	if l.Size() != 3 {
		t.Fatalf("size %d", l.Size())
	}
	first, ok := l.Shift()
	if !ok || first.v != 1 {
		t.Fatalf("shift got %v %v", first, ok)
	}
	if got := values(l); !eq(got, []int{2, 3}) {
		t.Fatalf("after shift got %v", got)
	}
}

func TestListPushListMarksReadonly(t *testing.T) {
	a := NewList(tn(1), tn(2))
	b := NewList(tn(3), tn(4))
	a.PushList(b)
	if got := values(a); !eq(got, []int{1, 2, 3, 4}) {
		t.Fatalf("got %v", got)
	}
	if !b.ReadOnly {
		t.Error("source list must be marked readonly after PushList")
	}
	defer func() {
		if recover() == nil {
			t.Error("pushing to readonly list must panic")
		}
	}()
	b.Push(tn(5))
}

func TestListUnshiftList(t *testing.T) {
	a := NewList(tn(3), tn(4))
	b := NewList(tn(1), tn(2))
	a.UnshiftList(b)
	if got := values(a); !eq(got, []int{1, 2, 3, 4}) {
		t.Fatalf("got %v", got)
	}
}

func TestListSomeEvery(t *testing.T) {
	l := NewList(tn(1), tn(2), tn(3))
	if !l.Some(func(n *testNode) bool { return n.v == 2 }) {
		t.Error("Some failed")
	}
	if l.Every(func(n *testNode) bool { return n.v < 3 }) {
		t.Error("Every failed")
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/luau/ -v`
Expected: FAIL — `List` undefined.

- [ ] **Step 3: Implement list.go**

```go
package luau

type ListNode[T Node] struct {
	Prev, Next *ListNode[T]
	Value      T
}

type List[T Node] struct {
	Head, Tail *ListNode[T]
	ReadOnly   bool
}

func (*List[T]) nodeOrList() {}

// AnyList lets union fields (NodeOrList) be tested for list-ness without
// knowing the element type. Mirrors upstream luau.list.isList.
type AnyList interface {
	NodeOrList
	anyList()
}

func (*List[T]) anyList() {}

func IsList(v NodeOrList) bool {
	_, ok := v.(AnyList)
	return ok
}

func NewList[T Node](values ...T) *List[T] {
	l := &List[T]{}
	for _, v := range values {
		l.Push(v)
	}
	return l
}

func (l *List[T]) assertWritable() {
	if l.ReadOnly {
		panic("list is readonly")
	}
}

func (l *List[T]) Push(value T) {
	l.assertWritable()
	node := &ListNode[T]{Value: value}
	if l.Tail != nil {
		l.Tail.Next = node
		node.Prev = l.Tail
	} else {
		l.Head = node
	}
	l.Tail = node
}

func (l *List[T]) PushList(other *List[T]) {
	l.assertWritable()
	other.assertWritable()
	other.ReadOnly = true
	if other.Head != nil {
		if l.Head != nil {
			l.Tail.Next = other.Head
			other.Head.Prev = l.Tail
			l.Tail = other.Tail
		} else {
			l.Head = other.Head
			l.Tail = other.Tail
		}
	}
}

func (l *List[T]) Shift() (T, bool) {
	l.assertWritable()
	var zero T
	if l.Head == nil {
		return zero, false
	}
	head := l.Head
	if head.Next != nil {
		l.Head = head.Next
		head.Next.Prev = nil
	} else {
		l.Head, l.Tail = nil, nil
	}
	return head.Value, true
}

func (l *List[T]) Unshift(value T) {
	l.assertWritable()
	node := &ListNode[T]{Value: value}
	if l.Head != nil {
		l.Head.Prev = node
		node.Next = l.Head
	} else {
		l.Tail = node
	}
	l.Head = node
}

func (l *List[T]) UnshiftList(other *List[T]) {
	l.assertWritable()
	other.assertWritable()
	other.ReadOnly = true
	if other.Head != nil {
		if l.Head != nil {
			l.Head.Prev = other.Tail
			other.Tail.Next = l.Head
			l.Head = other.Head
		} else {
			l.Head = other.Head
			l.Tail = other.Tail
		}
	}
}

func (l *List[T]) IsEmpty() bool    { return l.Head == nil }
func (l *List[T]) IsNonEmpty() bool { return l.Head != nil }

func (l *List[T]) ForEach(f func(T)) {
	for n := l.Head; n != nil; n = n.Next {
		f(n.Value)
	}
}

func (l *List[T]) ForEachNode(f func(*ListNode[T])) {
	for n := l.Head; n != nil; n = n.Next {
		f(n)
	}
}

func (l *List[T]) ToSlice() []T {
	out := []T{}
	l.ForEach(func(v T) { out = append(out, v) })
	return out
}

func (l *List[T]) Some(f func(T) bool) bool {
	for n := l.Head; n != nil; n = n.Next {
		if f(n.Value) {
			return true
		}
	}
	return false
}

func (l *List[T]) Every(f func(T) bool) bool {
	for n := l.Head; n != nil; n = n.Next {
		if !f(n.Value) {
			return false
		}
	}
	return true
}

func (l *List[T]) Size() int {
	size := 0
	for n := l.Head; n != nil; n = n.Next {
		size++
	}
	return size
}

// Clone makes a list of shallow-cloned elements (upstream list.clone).
func (l *List[T]) Clone() *List[T] {
	out := NewList[T]()
	l.ForEach(func(v T) { out.Push(v.shallowClone().(T)) })
	return out
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/luau/ -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add internal\luau && git commit -m "luau: doubly-linked node list"
```

### Task 8: Node structs

**Files:**

- Create: `internal/luau/nodes.go`
- Test: `internal/luau/nodes_test.go`
- Reference: `LuauAST/types/nodes.ts`, `LuauAST/types/mapping.ts`

Every node: embeds `base`, has `Kind()`, `shallowClone()`, and marker methods per its categories. Mechanical; the full listing follows. Delete the `testNode` placeholder from `list_test.go` and switch those tests to `*Identifier` (use `Name` instead of `v`; compare names like "a","b","c").

- [ ] **Step 1: Write the failing test**

`internal/luau/nodes_test.go`:

```go
package luau

import "testing"

func TestCategoryMarkers(t *testing.T) {
	var _ IndexableExpression = (*Identifier)(nil)
	var _ IndexableExpression = (*TemporaryIdentifier)(nil)
	var _ IndexableExpression = (*ComputedIndexExpression)(nil)
	var _ IndexableExpression = (*PropertyAccessExpression)(nil)
	var _ IndexableExpression = (*CallExpression)(nil)
	var _ IndexableExpression = (*MethodCallExpression)(nil)
	var _ IndexableExpression = (*ParenthesizedExpression)(nil)

	var _ Expression = (*NilLiteral)(nil)
	var _ Expression = (*MixedTable)(nil)

	var _ Statement = (*Assignment)(nil)
	var _ Statement = (*Comment)(nil)

	var _ FieldNode = (*MapField)(nil)
	var _ FieldNode = (*InterpolatedStringPart)(nil)

	var _ AnyIdentifier = (*Identifier)(nil)
	var _ AnyIdentifier = (*TemporaryIdentifier)(nil)

	var _ WritableExpression = (*Identifier)(nil)
	var _ WritableExpression = (*TemporaryIdentifier)(nil)
	var _ WritableExpression = (*PropertyAccessExpression)(nil)
	var _ WritableExpression = (*ComputedIndexExpression)(nil)

	var _ HasParameters = (*FunctionExpression)(nil)
	var _ HasParameters = (*FunctionDeclaration)(nil)
	var _ HasParameters = (*MethodDeclaration)(nil)
}

func TestKinds(t *testing.T) {
	if (&Identifier{}).Kind() != KindIdentifier {
		t.Error("Identifier kind")
	}
	if (&InterpolatedStringPart{}).Kind() != KindInterpolatedStringPart {
		t.Error("InterpolatedStringPart kind")
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/luau/ -v`
Expected: FAIL — types undefined.

- [ ] **Step 3: Implement nodes.go**

```go
package luau

// ---- indexable expressions ----

type Identifier struct {
	base
	Name string
}

type TemporaryIdentifier struct {
	base
	Name string
	ID   int
}

type ComputedIndexExpression struct {
	base
	Expression IndexableExpression
	Index      Expression
}

type PropertyAccessExpression struct {
	base
	Expression IndexableExpression
	Name       string
}

type CallExpression struct {
	base
	Expression IndexableExpression
	Args       *List[Expression]
}

type MethodCallExpression struct {
	base
	Name       string
	Expression IndexableExpression
	Args       *List[Expression]
}

type ParenthesizedExpression struct {
	base
	Expression Expression
}

// ---- expressions ----

type None struct{ base }
type NilLiteral struct{ base }
type FalseLiteral struct{ base }
type TrueLiteral struct{ base }

type NumberLiteral struct {
	base
	Value string
}

type StringLiteral struct {
	base
	Value string
}

type VarArgsLiteral struct{ base }

type FunctionExpression struct {
	base
	Parameters   *List[AnyIdentifier]
	HasDotDotDot bool
	Statements   *List[Statement]
}

type BinaryExpression struct {
	base
	Left     Expression
	Operator BinaryOperator
	Right    Expression
}

type UnaryExpression struct {
	base
	Operator   UnaryOperator
	Expression Expression
}

type IfExpression struct {
	base
	Condition   Expression
	Expression  Expression
	Alternative Expression
}

type InterpolatedString struct {
	base
	Parts *List[Node] // InterpolatedStringPart | Expression
}

type Array struct {
	base
	Members *List[Expression]
}

type Map struct {
	base
	Fields *List[*MapField]
}

type Set struct {
	base
	Members *List[Expression]
}

type MixedTable struct {
	base
	Fields *List[Node] // MapField | Expression
}

// ---- statements ----

type Assignment struct {
	base
	Left     NodeOrList // WritableExpression | *List[WritableExpression]
	Operator AssignmentOperator
	Right    NodeOrList // Expression | *List[Expression]
}

type BreakStatement struct{ base }

type CallStatement struct {
	base
	Expression Expression // CallExpression | MethodCallExpression
}

type ContinueStatement struct{ base }

type DoStatement struct {
	base
	Statements *List[Statement]
}

type WhileStatement struct {
	base
	Condition  Expression
	Statements *List[Statement]
}

type RepeatStatement struct {
	base
	Condition  Expression
	Statements *List[Statement]
}

type IfStatement struct {
	base
	Condition  Expression
	Statements *List[Statement]
	ElseBody   NodeOrList // *IfStatement | *List[Statement]
}

type NumericForStatement struct {
	base
	ID         AnyIdentifier
	Start      Expression
	End        Expression
	Step       Expression // may be nil
	Statements *List[Statement]
}

type ForStatement struct {
	base
	IDs        *List[AnyIdentifier]
	Expression Expression
	Statements *List[Statement]
}

type FunctionDeclaration struct {
	base
	Localize     bool
	Name         Expression // AnyIdentifier | PropertyAccessExpression
	Parameters   *List[AnyIdentifier]
	HasDotDotDot bool
	Statements   *List[Statement]
}

type MethodDeclaration struct {
	base
	Expression   IndexableExpression
	Name         string
	Parameters   *List[AnyIdentifier]
	HasDotDotDot bool
	Statements   *List[Statement]
}

type VariableDeclaration struct {
	base
	Left  NodeOrList // AnyIdentifier | *List[AnyIdentifier]
	Right NodeOrList // Expression | *List[Expression] | nil
}

type ReturnStatement struct {
	base
	Expression NodeOrList // Expression | *List[Expression]
}

type Comment struct {
	base
	Text string
}

// ---- fields ----

type MapField struct {
	base
	Index Expression
	Value Expression
}

type InterpolatedStringPart struct {
	base
	Text string
}

// ---- Kind() ----

func (*Identifier) Kind() SyntaxKind               { return KindIdentifier }
func (*TemporaryIdentifier) Kind() SyntaxKind      { return KindTemporaryIdentifier }
func (*ComputedIndexExpression) Kind() SyntaxKind  { return KindComputedIndexExpression }
func (*PropertyAccessExpression) Kind() SyntaxKind { return KindPropertyAccessExpression }
func (*CallExpression) Kind() SyntaxKind           { return KindCallExpression }
func (*MethodCallExpression) Kind() SyntaxKind     { return KindMethodCallExpression }
func (*ParenthesizedExpression) Kind() SyntaxKind  { return KindParenthesizedExpression }
func (*None) Kind() SyntaxKind                     { return KindNone }
func (*NilLiteral) Kind() SyntaxKind               { return KindNilLiteral }
func (*FalseLiteral) Kind() SyntaxKind             { return KindFalseLiteral }
func (*TrueLiteral) Kind() SyntaxKind              { return KindTrueLiteral }
func (*NumberLiteral) Kind() SyntaxKind            { return KindNumberLiteral }
func (*StringLiteral) Kind() SyntaxKind            { return KindStringLiteral }
func (*VarArgsLiteral) Kind() SyntaxKind           { return KindVarArgsLiteral }
func (*FunctionExpression) Kind() SyntaxKind       { return KindFunctionExpression }
func (*BinaryExpression) Kind() SyntaxKind         { return KindBinaryExpression }
func (*UnaryExpression) Kind() SyntaxKind          { return KindUnaryExpression }
func (*IfExpression) Kind() SyntaxKind             { return KindIfExpression }
func (*InterpolatedString) Kind() SyntaxKind       { return KindInterpolatedString }
func (*Array) Kind() SyntaxKind                    { return KindArray }
func (*Map) Kind() SyntaxKind                      { return KindMap }
func (*Set) Kind() SyntaxKind                      { return KindSet }
func (*MixedTable) Kind() SyntaxKind               { return KindMixedTable }
func (*Assignment) Kind() SyntaxKind               { return KindAssignment }
func (*BreakStatement) Kind() SyntaxKind           { return KindBreakStatement }
func (*CallStatement) Kind() SyntaxKind            { return KindCallStatement }
func (*ContinueStatement) Kind() SyntaxKind        { return KindContinueStatement }
func (*DoStatement) Kind() SyntaxKind              { return KindDoStatement }
func (*WhileStatement) Kind() SyntaxKind           { return KindWhileStatement }
func (*RepeatStatement) Kind() SyntaxKind          { return KindRepeatStatement }
func (*IfStatement) Kind() SyntaxKind              { return KindIfStatement }
func (*NumericForStatement) Kind() SyntaxKind      { return KindNumericForStatement }
func (*ForStatement) Kind() SyntaxKind             { return KindForStatement }
func (*FunctionDeclaration) Kind() SyntaxKind      { return KindFunctionDeclaration }
func (*MethodDeclaration) Kind() SyntaxKind        { return KindMethodDeclaration }
func (*VariableDeclaration) Kind() SyntaxKind      { return KindVariableDeclaration }
func (*ReturnStatement) Kind() SyntaxKind          { return KindReturnStatement }
func (*Comment) Kind() SyntaxKind                  { return KindComment }
func (*MapField) Kind() SyntaxKind                 { return KindMapField }
func (*InterpolatedStringPart) Kind() SyntaxKind   { return KindInterpolatedStringPart }
```

Then add, for every node type `X` above, the boilerplate (write it out for all 40 — no shortcuts):

```go
func (n *X) shallowClone() Node { c := *n; return &c }
```

and the category markers:

- `expressionNode()` on all expression types (Identifier through MixedTable, including all indexable)
- `indexableNode()` on the 7 indexable types
- `statementNode()` on all statement types (Assignment through Comment)
- `fieldNode()` on MapField and InterpolatedStringPart
- `anyIdentifierNode()` on Identifier and TemporaryIdentifier
- `writableNode()` on Identifier, TemporaryIdentifier, PropertyAccessExpression, ComputedIndexExpression
- `ParamData()` on FunctionExpression, FunctionDeclaration, MethodDeclaration:

```go
func (n *FunctionExpression) ParamData() (*List[AnyIdentifier], bool) {
	return n.Parameters, n.HasDotDotDot
}
```

(same body for the other two).

- [ ] **Step 4: Update list_test.go**

Delete `testNode`/`tn`/placeholder helpers; rebuild the tests on `*Identifier` with `Name` values "1".."5" (keep assertions identical in spirit).

- [ ] **Step 5: Run tests**

Run: `go test ./internal/luau/ -v`
Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add internal\luau && git commit -m "luau: node structs for all 40 syntax kinds"
```

### Task 9: Factories with clone-on-reparent

**Files:**

- Create: `internal/luau/create.go`
- Test: `internal/luau/create_test.go`
- Reference: `LuauAST/impl/create.ts`

Upstream `create()` sets `parent` on each node/list-element field; if a child ALREADY has a parent it shallow-clones the child first (lists: replaces `listNode.value` with the clone). This is essential — the transformer reuses node references freely.

- [ ] **Step 1: Write the failing test**

`internal/luau/create_test.go`:

```go
package luau

import "testing"

func TestAdoptSetsParent(t *testing.T) {
	left := ID("a")
	right := ID("b")
	bin := NewBinary(left, "+", right)
	if left.Parent() != Node(bin) || right.Parent() != Node(bin) {
		t.Error("children must be adopted")
	}
}

func TestAdoptClonesWhenAlreadyParented(t *testing.T) {
	shared := ID("x")
	first := NewBinary(shared, "+", ID("y"))
	second := NewBinary(shared, "-", ID("z"))
	if first.Left != Expression(shared) {
		t.Error("first use must keep the original node")
	}
	if second.Left == Expression(shared) {
		t.Error("second use must be a clone, not the original")
	}
	if second.Left.(*Identifier).Name != "x" {
		t.Error("clone must preserve fields")
	}
	if second.Left.Parent() != Node(second) {
		t.Error("clone must be adopted by the new parent")
	}
	if shared.Parent() != Node(first) {
		t.Error("original node's parent must be untouched")
	}
}

func TestListAdoption(t *testing.T) {
	a, b := ID("a"), ID("b")
	arr := NewArray(NewList[Expression](a, b))
	if a.Parent() != Node(arr) || b.Parent() != Node(arr) {
		t.Error("list elements must be adopted")
	}
	// reuse a in another list: the LIST NODE's value gets replaced by a clone
	l2 := NewList[Expression](a)
	arr2 := NewArray(l2)
	if l2.Head.Value == Expression(a) {
		t.Error("already-parented list element must be cloned")
	}
	if l2.Head.Value.Parent() != Node(arr2) {
		t.Error("cloned element must be adopted")
	}
}

func TestNumHandlesNegatives(t *testing.T) {
	n := Num(5)
	if lit, ok := n.(*NumberLiteral); !ok || lit.Value != "5" {
		t.Fatalf("Num(5) = %#v", n)
	}
	neg := Num(-3)
	un, ok := neg.(*UnaryExpression)
	if !ok || un.Operator != "-" {
		t.Fatalf("Num(-3) = %#v", neg)
	}
	if lit := un.Expression.(*NumberLiteral); lit.Value != "3" {
		t.Errorf("inner literal %q", lit.Value)
	}
}

func TestTempIDsUnique(t *testing.T) {
	a, b := TempID(""), TempID("foo")
	if a.ID == b.ID {
		t.Error("temp ids must be unique")
	}
	if b.Name != "foo" {
		t.Error("temp name")
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/luau/ -v`
Expected: FAIL — factories undefined.

- [ ] **Step 3: Implement create.go**

```go
package luau

import (
	"math"
	"strconv"
	"sync/atomic"
)

// adopt mirrors upstream create(): set child's parent; clone first if it
// already has one. Callers must not pass typed-nil interfaces (use nil checks
// for optional fields before calling).
func adopt[T Node](parent Node, child T) T {
	if any(child) == nil {
		return child
	}
	if child.Parent() != nil {
		child = child.shallowClone().(T)
	}
	child.setParent(parent)
	return child
}

func adoptList[T Node](parent Node, list *List[T]) *List[T] {
	if list == nil {
		return nil
	}
	list.ForEachNode(func(ln *ListNode[T]) {
		if ln.Value.Parent() != nil {
			ln.Value = ln.Value.shallowClone().(T)
		}
		ln.Value.setParent(parent)
	})
	return list
}

// adoptNodeOrList handles union fields (WritableExpression | List, etc.).
func adoptNodeOrList(parent Node, v NodeOrList) NodeOrList {
	switch x := v.(type) {
	case nil:
		return nil
	case *List[Expression]:
		return adoptList(parent, x)
	case *List[WritableExpression]:
		return adoptList(parent, x)
	case *List[AnyIdentifier]:
		return adoptList(parent, x)
	case *List[Statement]:
		return adoptList(parent, x)
	case Node:
		return adopt(parent, x)
	default:
		panic("adoptNodeOrList: unsupported value")
	}
}

// ---- constructors (one per kind; each adopts its children) ----

func NewIdentifier(name string) *Identifier { return &Identifier{Name: name} }

var lastTempID atomic.Int64

func TempID(name string) *TemporaryIdentifier {
	return &TemporaryIdentifier{Name: name, ID: int(lastTempID.Add(1))}
}

func NewComputedIndex(expression IndexableExpression, index Expression) *ComputedIndexExpression {
	n := &ComputedIndexExpression{}
	n.Expression = adopt[IndexableExpression](n, expression)
	n.Index = adopt[Expression](n, index)
	return n
}

func NewPropertyAccess(expression IndexableExpression, name string) *PropertyAccessExpression {
	n := &PropertyAccessExpression{Name: name}
	n.Expression = adopt[IndexableExpression](n, expression)
	return n
}

func NewCall(expression IndexableExpression, args *List[Expression]) *CallExpression {
	n := &CallExpression{}
	n.Expression = adopt[IndexableExpression](n, expression)
	n.Args = adoptList(n, args)
	return n
}

func NewMethodCall(name string, expression IndexableExpression, args *List[Expression]) *MethodCallExpression {
	n := &MethodCallExpression{Name: name}
	n.Expression = adopt[IndexableExpression](n, expression)
	n.Args = adoptList(n, args)
	return n
}

func NewParenthesized(expression Expression) *ParenthesizedExpression {
	n := &ParenthesizedExpression{}
	n.Expression = adopt[Expression](n, expression)
	return n
}

func NewNone() *None               { return &None{} }
func Nil() *NilLiteral             { return &NilLiteral{} }
func NewVarArgs() *VarArgsLiteral  { return &VarArgsLiteral{} }

func Bool(value bool) Expression {
	if value {
		return &TrueLiteral{}
	}
	return &FalseLiteral{}
}

func NewNumberLiteral(value string) *NumberLiteral { return &NumberLiteral{Value: value} }

// Num mirrors upstream luau.number(): negatives become unary minus.
func Num(value float64) Expression {
	if value >= 0 {
		return NewNumberLiteral(formatNum(value))
	}
	return NewUnary("-", Num(math.Abs(value)))
}

// formatNum mirrors JS String(number) closely enough for the values the
// compiler generates (integers and simple decimals).
func formatNum(value float64) string {
	if value == math.Trunc(value) && math.Abs(value) < 1e21 {
		return strconv.FormatFloat(value, 'f', -1, 64)
	}
	return strconv.FormatFloat(value, 'g', -1, 64)
}

func Str(value string) *StringLiteral { return &StringLiteral{Value: value} }
func ID(name string) *Identifier      { return NewIdentifier(name) }

func NewComment(text string) *Comment { return &Comment{Text: text} }

func NewFunctionExpression(params *List[AnyIdentifier], hasDotDotDot bool, statements *List[Statement]) *FunctionExpression {
	n := &FunctionExpression{HasDotDotDot: hasDotDotDot}
	n.Parameters = adoptList(n, params)
	n.Statements = adoptList(n, statements)
	return n
}

func NewBinary(left Expression, op BinaryOperator, right Expression) *BinaryExpression {
	n := &BinaryExpression{Operator: op}
	n.Left = adopt[Expression](n, left)
	n.Right = adopt[Expression](n, right)
	return n
}

func NewUnary(op UnaryOperator, expression Expression) *UnaryExpression {
	n := &UnaryExpression{Operator: op}
	n.Expression = adopt[Expression](n, expression)
	return n
}

func NewIfExpression(condition, expression, alternative Expression) *IfExpression {
	n := &IfExpression{}
	n.Condition = adopt[Expression](n, condition)
	n.Expression = adopt[Expression](n, expression)
	n.Alternative = adopt[Expression](n, alternative)
	return n
}

func NewInterpolatedString(parts *List[Node]) *InterpolatedString {
	n := &InterpolatedString{}
	n.Parts = adoptList(n, parts)
	return n
}

func NewInterpolatedStringPart(text string) *InterpolatedStringPart {
	return &InterpolatedStringPart{Text: text}
}

func NewArray(members *List[Expression]) *Array {
	n := &Array{}
	n.Members = adoptList(n, members)
	return n
}

func NewSet(members *List[Expression]) *Set {
	n := &Set{}
	n.Members = adoptList(n, members)
	return n
}

func NewMapField(index, value Expression) *MapField {
	n := &MapField{}
	n.Index = adopt[Expression](n, index)
	n.Value = adopt[Expression](n, value)
	return n
}

func NewMap(fields *List[*MapField]) *Map {
	n := &Map{}
	n.Fields = adoptList(n, fields)
	return n
}

func NewMixedTable(fields *List[Node]) *MixedTable {
	n := &MixedTable{}
	n.Fields = adoptList(n, fields)
	return n
}

func NewAssignment(left NodeOrList, op AssignmentOperator, right NodeOrList) *Assignment {
	n := &Assignment{Operator: op}
	n.Left = adoptNodeOrList(n, left)
	n.Right = adoptNodeOrList(n, right)
	return n
}

func NewBreak() *BreakStatement       { return &BreakStatement{} }
func NewContinue() *ContinueStatement { return &ContinueStatement{} }

func NewCallStatement(expression Expression) *CallStatement {
	n := &CallStatement{}
	n.Expression = adopt[Expression](n, expression)
	return n
}

func NewDo(statements *List[Statement]) *DoStatement {
	n := &DoStatement{}
	n.Statements = adoptList(n, statements)
	return n
}

func NewWhile(condition Expression, statements *List[Statement]) *WhileStatement {
	n := &WhileStatement{}
	n.Condition = adopt[Expression](n, condition)
	n.Statements = adoptList(n, statements)
	return n
}

func NewRepeat(condition Expression, statements *List[Statement]) *RepeatStatement {
	n := &RepeatStatement{}
	n.Statements = adoptList(n, statements)
	n.Condition = adopt[Expression](n, condition)
	return n
}

func NewIf(condition Expression, statements *List[Statement], elseBody NodeOrList) *IfStatement {
	n := &IfStatement{}
	n.Condition = adopt[Expression](n, condition)
	n.Statements = adoptList(n, statements)
	n.ElseBody = adoptNodeOrList(n, normalizeElseBody(elseBody))
	return n
}

// normalizeElseBody: nil means "no else" — store an empty statement list,
// matching how upstream transform code constructs IfStatements.
func normalizeElseBody(v NodeOrList) NodeOrList {
	if v == nil {
		return NewList[Statement]()
	}
	return v
}

func NewNumericFor(id AnyIdentifier, start, end, step Expression, statements *List[Statement]) *NumericForStatement {
	n := &NumericForStatement{}
	n.ID = adopt[AnyIdentifier](n, id)
	n.Start = adopt[Expression](n, start)
	n.End = adopt[Expression](n, end)
	if step != nil {
		n.Step = adopt[Expression](n, step)
	}
	n.Statements = adoptList(n, statements)
	return n
}

func NewFor(ids *List[AnyIdentifier], expression Expression, statements *List[Statement]) *ForStatement {
	n := &ForStatement{}
	n.IDs = adoptList(n, ids)
	n.Expression = adopt[Expression](n, expression)
	n.Statements = adoptList(n, statements)
	return n
}

func NewFunctionDeclaration(localize bool, name Expression, params *List[AnyIdentifier], hasDotDotDot bool, statements *List[Statement]) *FunctionDeclaration {
	n := &FunctionDeclaration{Localize: localize, HasDotDotDot: hasDotDotDot}
	n.Name = adopt[Expression](n, name)
	n.Parameters = adoptList(n, params)
	n.Statements = adoptList(n, statements)
	return n
}

func NewMethodDeclaration(expression IndexableExpression, name string, params *List[AnyIdentifier], hasDotDotDot bool, statements *List[Statement]) *MethodDeclaration {
	n := &MethodDeclaration{Name: name, HasDotDotDot: hasDotDotDot}
	n.Expression = adopt[IndexableExpression](n, expression)
	n.Parameters = adoptList(n, params)
	n.Statements = adoptList(n, statements)
	return n
}

func NewVariableDeclaration(left NodeOrList, right NodeOrList) *VariableDeclaration {
	n := &VariableDeclaration{}
	n.Left = adoptNodeOrList(n, left)
	if right != nil {
		n.Right = adoptNodeOrList(n, right)
	}
	return n
}

func NewReturn(expression NodeOrList) *ReturnStatement {
	n := &ReturnStatement{}
	n.Expression = adoptNodeOrList(n, expression)
	return n
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/luau/ -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add internal\luau && git commit -m "luau: node factories with clone-on-reparent adoption"
```

### Task 10: Type guards, validators, globals

**Files:**

- Create: `internal/luau/guards.go`, `internal/luau/validate.go`, `internal/luau/globals.go`
- Test: `internal/luau/guards_test.go`, `internal/luau/validate_test.go`
- Reference: `LuauAST/impl/typeGuards.ts`, `LuauAST/util/{isValidIdentifier,isValidNumberLiteral,isMetamethod,isReservedIdentifier,isReservedClassField}.ts`, `LuauAST/impl/globals.ts`, `LuauAST/impl/strings.ts`
- [ ] **Step 1: Write the failing tests**

`internal/luau/guards_test.go`:

```go
package luau

import "testing"

func TestRangeGuards(t *testing.T) {
	if !IsExpression(ID("x")) || !IsIndexableExpression(ID("x")) {
		t.Error("identifier guards")
	}
	if IsStatement(ID("x")) || IsExpression(NewBreak()) {
		t.Error("category confusion")
	}
	if !IsStatement(NewBreak()) || !IsField(NewMapField(Str("k"), Nil())) {
		t.Error("statement/field guards")
	}
}

func TestCompositeGuards(t *testing.T) {
	if !IsSimple(Str("s")) || !IsSimple(TempID("")) || IsSimple(NewArray(NewList[Expression]())) {
		t.Error("IsSimple")
	}
	if !IsSimplePrimitive(Nil()) || IsSimplePrimitive(ID("x")) {
		t.Error("IsSimplePrimitive")
	}
	if !IsTable(NewSet(NewList[Expression]())) || IsTable(Str("s")) {
		t.Error("IsTable")
	}
	if !IsFinalStatement(NewBreak()) || !IsFinalStatement(NewContinue()) || IsFinalStatement(NewDo(NewList[Statement]())) {
		t.Error("IsFinalStatement")
	}
	c := NewCall(ID("f"), NewList[Expression]())
	if !IsCall(c) || !IsCall(NewMethodCall("m", ID("o"), NewList[Expression]())) {
		t.Error("IsCall")
	}
	if !IsFunctionLike(NewFunctionExpression(NewList[AnyIdentifier](), false, NewList[Statement]())) {
		t.Error("IsFunctionLike")
	}
	if !HasStatements(NewDo(NewList[Statement]())) || HasStatements(NewBreak()) {
		t.Error("HasStatements")
	}
	if !IsExpressionWithPrecedence(NewBinary(ID("a"), "+", ID("b"))) || IsExpressionWithPrecedence(ID("a")) {
		t.Error("IsExpressionWithPrecedence")
	}
}
```

`internal/luau/validate_test.go`:

```go
package luau

import "testing"

func TestIsValidIdentifier(t *testing.T) {
	valid := []string{"x", "_foo", "A1_b2", "_"}
	invalid := []string{"and", "end", "nil", "local", "until", "then", "elseif", "repeat", "1x", "a-b", "a b", "", "héllo"}
	for _, s := range valid {
		if !IsValidIdentifier(s) {
			t.Errorf("%q should be valid", s)
		}
	}
	for _, s := range invalid {
		if IsValidIdentifier(s) {
			t.Errorf("%q should be invalid", s)
		}
	}
}

func TestIsValidNumberLiteral(t *testing.T) {
	valid := []string{"1", "1_000", "1.5", ".5", "1e5", "1E+5", "1.2e-3", "0b1010", "0B_1010", "0xFF", "0X_ff_0"}
	invalid := []string{"", "abc", "0b", "0x", "1.2.3", "e5"}
	for _, s := range valid {
		if !IsValidNumberLiteral(s) {
			t.Errorf("%q should be valid", s)
		}
	}
	for _, s := range invalid {
		if IsValidNumberLiteral(s) {
			t.Errorf("%q should be invalid", s)
		}
	}
}

func TestMetamethodsAndReserved(t *testing.T) {
	if !IsMetamethod("__index") || IsMetamethod("__banana") {
		t.Error("IsMetamethod")
	}
	if !IsReservedClassField("new") || !IsReservedClassField("__index") || IsReservedClassField("foo") {
		t.Error("IsReservedClassField")
	}
	if !IsReservedIdentifier("TS") || !IsReservedIdentifier("game") || IsReservedIdentifier("myVar") {
		t.Error("IsReservedIdentifier")
	}
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `go test ./internal/luau/ -v`
Expected: FAIL — guards undefined.

- [ ] **Step 3: Implement guards.go**

```go
package luau

func IsIndexableExpression(n Node) bool {
	return n.Kind() >= FirstIndexableExpression && n.Kind() <= LastIndexableExpression
}
func IsExpression(n Node) bool { return n.Kind() >= FirstExpression && n.Kind() <= LastExpression }
func IsStatement(n Node) bool  { return n.Kind() >= FirstStatement && n.Kind() <= LastStatement }
func IsField(n Node) bool      { return n.Kind() >= FirstField && n.Kind() <= LastField }

func IsSimple(n Node) bool {
	switch n.Kind() {
	case KindIdentifier, KindTemporaryIdentifier, KindNilLiteral, KindTrueLiteral,
		KindFalseLiteral, KindNumberLiteral, KindStringLiteral:
		return true
	}
	return false
}

func IsSimplePrimitive(n Node) bool {
	switch n.Kind() {
	case KindNilLiteral, KindTrueLiteral, KindFalseLiteral, KindNumberLiteral, KindStringLiteral:
		return true
	}
	return false
}

func IsTable(n Node) bool {
	switch n.Kind() {
	case KindArray, KindSet, KindMap, KindMixedTable:
		return true
	}
	return false
}

func IsFinalStatement(n Node) bool {
	switch n.Kind() {
	case KindBreakStatement, KindReturnStatement, KindContinueStatement:
		return true
	}
	return false
}

func IsCall(n Node) bool {
	return n.Kind() == KindCallExpression || n.Kind() == KindMethodCallExpression
}

func IsFunctionLike(n Node) bool {
	switch n.Kind() {
	case KindFunctionDeclaration, KindFunctionExpression, KindMethodDeclaration:
		return true
	}
	return false
}

func HasStatements(n Node) bool {
	switch n.Kind() {
	case KindForStatement, KindNumericForStatement, KindFunctionExpression, KindDoStatement,
		KindFunctionDeclaration, KindIfStatement, KindMethodDeclaration, KindRepeatStatement,
		KindWhileStatement:
		return true
	}
	return false
}

// StatementsOf returns the statement list of any HasStatements node.
func StatementsOf(n Node) *List[Statement] {
	switch x := n.(type) {
	case *ForStatement:
		return x.Statements
	case *NumericForStatement:
		return x.Statements
	case *FunctionExpression:
		return x.Statements
	case *DoStatement:
		return x.Statements
	case *FunctionDeclaration:
		return x.Statements
	case *IfStatement:
		return x.Statements
	case *MethodDeclaration:
		return x.Statements
	case *RepeatStatement:
		return x.Statements
	case *WhileStatement:
		return x.Statements
	}
	return nil
}

func IsExpressionWithPrecedence(n Node) bool {
	switch n.Kind() {
	case KindIfExpression, KindUnaryExpression, KindBinaryExpression:
		return true
	}
	return false
}
```

(Kind-specific guards like upstream `isIdentifier(x)` are unnecessary in Go — use type assertions `x, ok := n.(*Identifier)` at call sites.)

- [ ] **Step 4: Implement validate.go**

```go
package luau

import "regexp"

var luauReservedKeywords = map[string]struct{}{
	"and": {}, "break": {}, "do": {}, "else": {}, "elseif": {}, "end": {},
	"false": {}, "for": {}, "function": {}, "if": {}, "in": {}, "local": {},
	"nil": {}, "not": {}, "or": {}, "repeat": {}, "return": {}, "then": {},
	"true": {}, "until": {}, "while": {},
}

var luauIdentifierRegex = regexp.MustCompile(`^[A-Za-z_][A-Za-z0-9_]*$`)

func IsValidIdentifier(id string) bool {
	if _, reserved := luauReservedKeywords[id]; reserved {
		return false
	}
	return luauIdentifierRegex.MatchString(id)
}

var (
	decimalLiteralRegex     = regexp.MustCompile(`^(?:\d[\d_]*(?:\.[\d_]*)?|\.\d[\d_]*)(?:[eE][+-]?_*\d[\d_]*)?$`)
	binaryLiteralRegex      = regexp.MustCompile(`^0_*[bB]_*[01][01_]*$`)
	hexadecimalLiteralRegex = regexp.MustCompile(`^0_*[xX]_*[\da-fA-F][\da-fA-F_]*$`)
)

func IsValidNumberLiteral(text string) bool {
	return decimalLiteralRegex.MatchString(text) ||
		binaryLiteralRegex.MatchString(text) ||
		hexadecimalLiteralRegex.MatchString(text)
}

var luauMetamethods = map[string]struct{}{
	"__index": {}, "__newindex": {}, "__call": {}, "__concat": {}, "__unm": {},
	"__add": {}, "__sub": {}, "__mul": {}, "__div": {}, "__mod": {}, "__pow": {},
	"__tostring": {}, "__metatable": {}, "__eq": {}, "__lt": {}, "__le": {},
	"__mode": {}, "__gc": {}, "__len": {},
}

func IsMetamethod(id string) bool {
	_, ok := luauMetamethods[id]
	return ok
}

var luauReservedClassFields = map[string]struct{}{"__index": {}, "new": {}}

func IsReservedClassField(id string) bool {
	_, ok := luauReservedClassFields[id]
	return ok
}

func IsReservedIdentifier(id string) bool {
	_, ok := reservedGlobalNames[id]
	return ok
}
```

- [ ] **Step 5: Implement globals.go**

Port `LuauAST/impl/globals.ts` and `strings.ts`. Globals are accessed constantly by the transformer; in Go they must be FUNCTIONS that build fresh nodes (upstream relies on `create()` cloning already-parented nodes — fresh construction is the simpler equivalent with identical behavior):

```go
package luau

// Globals mirrors upstream luau.globals — call to get a fresh node.
// reservedGlobalNames must contain exactly the top-level keys of upstream globals.ts.
var reservedGlobalNames = map[string]struct{}{
	"_G": {}, "TS": {}, "assert": {}, "bit32": {}, "coroutine": {}, "error": {},
	"exports": {}, "getmetatable": {}, "ipairs": {}, "next": {}, "pairs": {},
	"pcall": {}, "require": {}, "script": {}, "select": {}, "self": {},
	"setmetatable": {}, "string": {}, "super": {}, "table": {}, "utf8": {},
	"math": {}, "tostring": {}, "type": {}, "typeof": {}, "unpack": {}, "game": {},
}

func GlobalID(name string) *Identifier { return ID(name) }

func GlobalProperty(object, name string) *PropertyAccessExpression {
	return NewPropertyAccess(ID(object), name)
}
```

(The full per-name accessor surface — `globals.table.insert` etc. — is consumed by the transformer in Phase 2/3; defining `GlobalProperty("table", "insert")` call sites there keeps this file small. `reservedGlobalNames` is what `IsReservedIdentifier` needs now. Port `strings.ts` the same way when Phase 3 macros need it.)

- [ ] **Step 6: Run tests**

Run: `go test ./internal/luau/ -v`
Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add internal\luau && git commit -m "luau: type guards, validators, reserved globals"
```

### Task 11: RenderState + render utilities

**Files:**

- Create: `internal/luau/render/state.go`, `internal/luau/render/ending.go`, `internal/luau/render/parens.go`, `internal/luau/render/util.go`, `internal/luau/render/visit.go`
- Create: `internal/luau/render/render.go` (dispatcher stub — full in Tasks 13/14)
- Test: `internal/luau/render/util_test.go`
- Reference: `LuauRenderer/RenderState.ts`, `LuauRenderer/util/{getEnding,needsParentheses,getSafeBracketEquals,renderArguments,renderParameters,renderStatements,visit}.ts`
- [ ] **Step 1: Write the failing test**

`internal/luau/render/util_test.go`:

```go
package render

import (
	"testing"

	"rotor/internal/luau"
)

func TestGetSafeBracketEquals(t *testing.T) {
	cases := map[string]string{
		"hello":      "",
		"a ]] b":     "=",
		"a ]=] b":    "==",
		"ends with ]": "=",
	}
	for in, want := range cases {
		if got := getSafeBracketEquals(in); got != want {
			t.Errorf("getSafeBracketEquals(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestNeedsParentheses(t *testing.T) {
	// (a + b) * c — lower precedence child on the left of higher precedence parent
	inner := luau.NewBinary(luau.ID("a"), "+", luau.ID("b"))
	luau.NewBinary(inner, "*", luau.ID("c"))
	if !needsParentheses(inner) {
		t.Error("(a + b) * c: left + needs parens under *")
	}

	// a + b * c — higher precedence child needs none
	inner2 := luau.NewBinary(luau.ID("b"), "*", luau.ID("c"))
	luau.NewBinary(luau.ID("a"), "+", inner2)
	if needsParentheses(inner2) {
		t.Error("a + b * c: right * needs no parens under +")
	}

	// a - (b - c) — equal precedence on the right needs parens
	inner3 := luau.NewBinary(luau.ID("b"), "-", luau.ID("c"))
	luau.NewBinary(luau.ID("a"), "-", inner3)
	if !needsParentheses(inner3) {
		t.Error("a - (b - c): equal precedence right operand needs parens")
	}

	// a - b - c (left-assoc: left child same precedence, no parens)
	inner4 := luau.NewBinary(luau.ID("a"), "-", luau.ID("b"))
	luau.NewBinary(inner4, "-", luau.ID("c"))
	if needsParentheses(inner4) {
		t.Error("(a - b) - c: left operand needs no parens")
	}
}

func TestRenderStateIndent(t *testing.T) {
	s := NewRenderState()
	if got := s.Line("x"); got != "x\n" {
		t.Errorf("Line at depth 0 = %q", got)
	}
	out := s.Block(func() string { return s.Line("y") })
	if out != "\ty\n" {
		t.Errorf("Line at depth 1 = %q", out)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/luau/render/ -v`
Expected: FAIL — package doesn't exist.

- [ ] **Step 3: Implement state.go**

```go
package render

import (
	"rotor/internal/luau"
)

const indentCharacter = "\t"

type RenderState struct {
	indent         string
	seenTempNodes  map[int]string
	tempIDFallback int
	listNodesStack []*luau.ListNode[luau.Statement]
}

func NewRenderState() *RenderState {
	return &RenderState{seenTempNodes: map[int]string{}}
}

func (s *RenderState) pushIndent() { s.indent += indentCharacter }
func (s *RenderState) popIndent()  { s.indent = s.indent[len(indentCharacter):] }

// getTempName: solveTempIds should have populated seenTempNodes; the counter
// fallback mirrors upstream's safety net.
func (s *RenderState) getTempName(node *luau.TemporaryIdentifier) string {
	if name, ok := s.seenTempNodes[node.ID]; ok {
		return name
	}
	name := "_" + itoa(s.tempIDFallback)
	s.tempIDFallback++
	s.seenTempNodes[node.ID] = name
	return name
}

func itoa(i int) string {
	// strconv.Itoa; small helper kept inline-free for clarity
	return fmtInt(i)
}

func (s *RenderState) pushListNode(n *luau.ListNode[luau.Statement]) {
	s.listNodesStack = append(s.listNodesStack, n)
}

func (s *RenderState) peekListNode() *luau.ListNode[luau.Statement] {
	if len(s.listNodesStack) == 0 {
		return nil
	}
	return s.listNodesStack[len(s.listNodesStack)-1]
}

func (s *RenderState) popListNode() {
	s.listNodesStack = s.listNodesStack[:len(s.listNodesStack)-1]
}

func (s *RenderState) Indented(text string) string { return s.indent + text }

func (s *RenderState) Line(text string) string { return s.Indented(text) + "\n" }

// LineWithEnd is upstream state.line(text, endNode): appends `;` when needed
// to avoid Luau ambiguous-syntax errors.
func (s *RenderState) LineWithEnd(text string, endNode luau.Statement) string {
	return s.Indented(text) + getEnding(s, endNode) + "\n"
}

func (s *RenderState) Block(callback func() string) string {
	s.pushIndent()
	result := callback()
	s.popIndent()
	return result
}
```

(replace `itoa`/`fmtInt` with `strconv.Itoa` directly — shown expanded here only to flag the import.)

- [ ] **Step 4: Implement parens.go, util.go, ending.go, visit.go**

`parens.go`:

```go
package render

import "rotor/internal/luau"

// Luau operator precedence — https://www.lua.org/manual/5.1/manual.html#2.5.6
const ifExpressionPrecedence = 1

var unaryOperatorPrecedence = map[luau.UnaryOperator]int{
	"not": 7, "#": 7, "-": 7,
}

var binaryOperatorPrecedence = map[luau.BinaryOperator]int{
	"or": 1, "and": 2,
	"<": 3, ">": 3, "<=": 3, ">=": 3, "~=": 3, "==": 3,
	"..": 4, "+": 5, "-": 5,
	"*": 6, "/": 6, "//": 6, "%": 6,
	"^": 8,
}

func getPrecedence(node luau.Node) int {
	switch n := node.(type) {
	case *luau.IfExpression:
		return ifExpressionPrecedence
	case *luau.BinaryExpression:
		return binaryOperatorPrecedence[n.Operator]
	case *luau.UnaryExpression:
		return unaryOperatorPrecedence[n.Operator]
	}
	panic("getPrecedence: not an expression with precedence")
}

func needsParentheses(node luau.Node) bool {
	parent := node.Parent()
	if parent != nil && luau.IsExpressionWithPrecedence(parent) {
		nodePrec, parentPrec := getPrecedence(node), getPrecedence(parent)
		if nodePrec < parentPrec {
			return true
		}
		if nodePrec == parentPrec {
			if bin, ok := parent.(*luau.BinaryExpression); ok {
				return luau.Node(node) == luau.Node(bin.Right) ||
					(bin.Right != nil && node == luau.Node(bin.Right))
			}
		}
	}
	return false
}
```

(Simplify the right-child comparison to a single `node == luau.Node(bin.Right)` — interface equality on the same pointer.)

`util.go`:

```go
package render

import (
	"strings"

	"rotor/internal/luau"
)

func getSafeBracketEquals(str string) string {
	amtEquals := 0
	for strings.Contains(str, "]"+strings.Repeat("=", amtEquals)+"]") ||
		strings.HasSuffix(str, "]"+strings.Repeat("=", amtEquals)) {
		amtEquals++
	}
	return strings.Repeat("=", amtEquals)
}

func renderArguments(s *RenderState, expressions *luau.List[luau.Expression]) string {
	parts := []string{}
	expressions.ForEach(func(v luau.Expression) { parts = append(parts, Render(s, v)) })
	return strings.Join(parts, ", ")
}

func renderParameters(s *RenderState, node luau.HasParameters) string {
	params, hasDotDotDot := node.ParamData()
	parts := []string{}
	params.ForEach(func(p luau.AnyIdentifier) { parts = append(parts, Render(s, p)) })
	if hasDotDotDot {
		parts = append(parts, "...")
	}
	return strings.Join(parts, ", ")
}

func renderStatements(s *RenderState, statements *luau.List[luau.Statement]) string {
	var b strings.Builder
	hasFinalStatement := false
	for listNode := statements.Head; listNode != nil; listNode = listNode.Next {
		if hasFinalStatement {
			if _, isComment := listNode.Value.(*luau.Comment); !isComment {
				panic("Cannot render statement after break, continue, or return!")
			}
		}
		hasFinalStatement = hasFinalStatement || luau.IsFinalStatement(listNode.Value)
		s.pushListNode(listNode)
		b.WriteString(Render(s, listNode.Value))
		s.popListNode()
	}
	return b.String()
}
```

`ending.go` — port `getEnding.ts` exactly:

```go
package render

import "rotor/internal/luau"

func endsWithIndexableExpressionInner(node luau.Expression) bool {
	if luau.IsIndexableExpression(node) {
		return true
	}
	switch n := node.(type) {
	case *luau.BinaryExpression:
		return endsWithIndexableExpressionInner(n.Right)
	case *luau.UnaryExpression:
		return endsWithIndexableExpressionInner(n.Expression)
	case *luau.IfExpression:
		return endsWithIndexableExpressionInner(n.Alternative)
	}
	return false
}

func lastExprOf(v luau.NodeOrList) (luau.Expression, bool) {
	switch x := v.(type) {
	case *luau.List[luau.Expression]:
		if x.Tail == nil {
			panic("empty expression list")
		}
		return x.Tail.Value, true
	case *luau.List[luau.WritableExpression]:
		if x.Tail == nil {
			panic("empty writable list")
		}
		return x.Tail.Value, true
	case *luau.List[luau.AnyIdentifier]:
		if x.Tail == nil {
			panic("empty identifier list")
		}
		return x.Tail.Value, true
	case luau.Expression:
		return x, true
	}
	return nil, false
}

func endsWithIndexableExpression(node luau.Statement) bool {
	switch n := node.(type) {
	case *luau.CallStatement:
		return true
	case *luau.VariableDeclaration:
		v := n.Right
		if v == nil {
			v = n.Left
		}
		if e, ok := lastExprOf(v); ok {
			return endsWithIndexableExpressionInner(e)
		}
	case *luau.Assignment:
		v := n.Right
		if v == nil {
			v = n.Left
		}
		if e, ok := lastExprOf(v); ok {
			return endsWithIndexableExpressionInner(e)
		}
	}
	return false
}

func startsWithParenthesisInner(node luau.Expression) bool {
	switch n := node.(type) {
	case *luau.ParenthesizedExpression:
		return true
	case *luau.CallExpression:
		return startsWithParenthesisInner(n.Expression)
	case *luau.MethodCallExpression:
		return startsWithParenthesisInner(n.Expression)
	case *luau.PropertyAccessExpression:
		return startsWithParenthesisInner(n.Expression)
	case *luau.ComputedIndexExpression:
		return startsWithParenthesisInner(n.Expression)
	}
	return false
}

func startsWithParenthesis(node luau.Statement) bool {
	switch n := node.(type) {
	case *luau.CallStatement:
		switch e := n.Expression.(type) {
		case *luau.CallExpression:
			return startsWithParenthesisInner(e.Expression)
		case *luau.MethodCallExpression:
			return startsWithParenthesisInner(e.Expression)
		}
	case *luau.Assignment:
		switch l := n.Left.(type) {
		case *luau.List[luau.WritableExpression]:
			if l.Head == nil {
				panic("empty assignment left list")
			}
			return startsWithParenthesisInner(l.Head.Value)
		case luau.Expression:
			return startsWithParenthesisInner(l)
		}
	}
	return false
}

func getNextNonComment(s *RenderState) luau.Statement {
	listNode := s.peekListNode()
	if listNode == nil {
		return nil
	}
	next := listNode.Next
	for next != nil {
		if _, isComment := next.Value.(*luau.Comment); !isComment {
			break
		}
		next = next.Next
	}
	if next == nil {
		return nil
	}
	return next.Value
}

func getEnding(s *RenderState, node luau.Statement) string {
	next := getNextNonComment(s)
	if next != nil && endsWithIndexableExpression(node) && startsWithParenthesis(next) {
		return ";"
	}
	return ""
}
```

`visit.go` — port `visit.ts` as a type switch (used by solveTempIds; full child coverage per the upstream KIND_TO_VISITOR table):

```go
package render

import "rotor/internal/luau"

type visitor struct {
	before func(luau.Node)
	after  func(luau.Node)
}

func visitNodeOrList(v luau.NodeOrList, vis *visitor) {
	switch x := v.(type) {
	case nil:
	case *luau.List[luau.Expression]:
		x.ForEach(func(n luau.Expression) { visitNode(n, vis) })
	case *luau.List[luau.Statement]:
		x.ForEach(func(n luau.Statement) { visitNode(n, vis) })
	case *luau.List[luau.AnyIdentifier]:
		x.ForEach(func(n luau.AnyIdentifier) { visitNode(n, vis) })
	case *luau.List[luau.WritableExpression]:
		x.ForEach(func(n luau.WritableExpression) { visitNode(n, vis) })
	case *luau.List[luau.Node]:
		x.ForEach(func(n luau.Node) { visitNode(n, vis) })
	case *luau.List[*luau.MapField]:
		x.ForEach(func(n *luau.MapField) { visitNode(n, vis) })
	case luau.Node:
		visitNode(x, vis)
	}
}

func visitNode(node luau.Node, vis *visitor) {
	if vis.before != nil {
		vis.before(node)
	}
	switch n := node.(type) {
	case *luau.ComputedIndexExpression:
		visitNode(n.Expression, vis)
		visitNode(n.Index, vis)
	case *luau.PropertyAccessExpression:
		visitNode(n.Expression, vis)
	case *luau.CallExpression:
		visitNode(n.Expression, vis)
		visitNodeOrList(n.Args, vis)
	case *luau.MethodCallExpression:
		visitNode(n.Expression, vis)
		visitNodeOrList(n.Args, vis)
	case *luau.ParenthesizedExpression:
		visitNode(n.Expression, vis)
	case *luau.FunctionExpression:
		visitNodeOrList(n.Parameters, vis)
		visitNodeOrList(n.Statements, vis)
	case *luau.BinaryExpression:
		visitNode(n.Left, vis)
		visitNode(n.Right, vis)
	case *luau.UnaryExpression:
		visitNode(n.Expression, vis)
	case *luau.IfExpression:
		visitNode(n.Condition, vis)
		visitNode(n.Expression, vis)
		visitNode(n.Alternative, vis)
	case *luau.InterpolatedString:
		visitNodeOrList(n.Parts, vis)
	case *luau.Array:
		visitNodeOrList(n.Members, vis)
	case *luau.Map:
		visitNodeOrList(n.Fields, vis)
	case *luau.Set:
		visitNodeOrList(n.Members, vis)
	case *luau.MixedTable:
		visitNodeOrList(n.Fields, vis)
	case *luau.Assignment:
		visitNodeOrList(n.Left, vis)
		visitNodeOrList(n.Right, vis)
	case *luau.CallStatement:
		visitNode(n.Expression, vis)
	case *luau.DoStatement:
		visitNodeOrList(n.Statements, vis)
	case *luau.WhileStatement:
		visitNode(n.Condition, vis)
		visitNodeOrList(n.Statements, vis)
	case *luau.RepeatStatement:
		visitNodeOrList(n.Statements, vis)
		visitNode(n.Condition, vis)
	case *luau.IfStatement:
		visitNode(n.Condition, vis)
		visitNodeOrList(n.Statements, vis)
		visitNodeOrList(n.ElseBody, vis)
	case *luau.NumericForStatement:
		visitNode(n.ID, vis)
		visitNode(n.Start, vis)
		visitNode(n.End, vis)
		if n.Step != nil {
			visitNode(n.Step, vis)
		}
		visitNodeOrList(n.Statements, vis)
	case *luau.ForStatement:
		visitNodeOrList(n.IDs, vis)
		visitNodeOrList(n.Statements, vis)
	case *luau.FunctionDeclaration:
		visitNode(n.Name, vis)
		visitNodeOrList(n.Parameters, vis)
		visitNodeOrList(n.Statements, vis)
	case *luau.MethodDeclaration:
		visitNode(n.Expression, vis)
		visitNodeOrList(n.Parameters, vis)
		visitNodeOrList(n.Statements, vis)
	case *luau.VariableDeclaration:
		visitNodeOrList(n.Left, vis)
		visitNodeOrList(n.Right, vis)
	case *luau.ReturnStatement:
		visitNodeOrList(n.Expression, vis)
	case *luau.MapField:
		visitNode(n.Index, vis)
		visitNode(n.Value, vis)
	}
	if vis.after != nil {
		vis.after(node)
	}
}
```

Also create `render.go` with just the exported entry points so the package compiles (bodies land in Tasks 13/14):

```go
package render

import "rotor/internal/luau"

// Render dispatches on node kind. Cases filled in by expression/statement tasks.
func Render(s *RenderState, node luau.Node) string {
	panic("render: no renderer for " + node.Kind().String())
}
```

- [ ] **Step 5: Run tests**

Run: `go test ./internal/luau/render/ -v`
Expected: PASS (`TestGetSafeBracketEquals`, `TestNeedsParentheses`, `TestRenderStateIndent`).

- [ ] **Step 6: Commit**

```powershell
git add internal\luau && git commit -m "luau/render: state, ending, precedence, visit utilities"
```

### Task 12: solveTempIds

**Files:**

- Create: `internal/luau/render/solvetempids.go`
- Test: `internal/luau/render/solvetempids_test.go`
- Reference: `LuauRenderer/solveTempIds.ts`
- [ ] **Step 1: Write the failing test**

```go
package render

import (
	"testing"

	"rotor/internal/luau"
)

func solveFor(stmts *luau.List[luau.Statement]) *RenderState {
	s := NewRenderState()
	solveTempIDs(s, stmts)
	return s
}

func TestTempIdsBasic(t *testing.T) {
	t1, t2 := luau.TempID(""), luau.TempID("")
	stmts := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(t1, luau.Num(1)),
		luau.NewVariableDeclaration(t2, luau.Num(2)),
	)
	s := solveFor(stmts)
	if got := s.seenTempNodes[t1.ID]; got != "_" {
		t.Errorf("first temp = %q, want %q", got, "_")
	}
	if got := s.seenTempNodes[t2.ID]; got != "_1" {
		t.Errorf("second temp = %q, want %q", got, "_1")
	}
}

func TestTempIdsNamed(t *testing.T) {
	t1, t2 := luau.TempID("foo"), luau.TempID("foo")
	stmts := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(t1, luau.Num(1)),
		luau.NewVariableDeclaration(t2, luau.Num(2)),
	)
	s := solveFor(stmts)
	if got := s.seenTempNodes[t1.ID]; got != "_foo" {
		t.Errorf("first = %q, want _foo", got)
	}
	if got := s.seenTempNodes[t2.ID]; got != "_foo_1" {
		t.Errorf("second = %q, want _foo_1", got)
	}
}

func TestTempIdsAvoidDeclaredLocals(t *testing.T) {
	tmp := luau.TempID("foo")
	stmts := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(luau.ID("_foo"), luau.Num(1)),
		luau.NewVariableDeclaration(tmp, luau.Num(2)),
	)
	s := solveFor(stmts)
	if got := s.seenTempNodes[tmp.ID]; got != "_foo_1" {
		t.Errorf("temp = %q, want _foo_1 (collides with declared _foo)", got)
	}
}

func TestTempIdsScopedFunctionsDontCollide(t *testing.T) {
	// two function expressions each with their own temp: both may use "_"
	mk := func() (*luau.TemporaryIdentifier, luau.Statement) {
		tmp := luau.TempID("")
		body := luau.NewList[luau.Statement](luau.NewVariableDeclaration(tmp, luau.Num(1)))
		fn := luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](), false, body)
		return tmp, luau.NewVariableDeclaration(luau.ID("f"), fn)
	}
	ta, sa := mk()
	tb, sb := mk()
	s := solveFor(luau.NewList[luau.Statement](sa, sb))
	if s.seenTempNodes[ta.ID] != "_" || s.seenTempNodes[tb.ID] != "_" {
		t.Errorf("separate function scopes should both get %q: got %q, %q",
			"_", s.seenTempNodes[ta.ID], s.seenTempNodes[tb.ID])
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/luau/render/ -v`
Expected: FAIL — `solveTempIDs` undefined.

- [ ] **Step 3: Implement solvetempids.go**

Port `solveTempIds.ts` exactly — scope edges, lastTry map, separator rule:

```go
package render

import (
	"strconv"

	"rotor/internal/luau"
)

type tempScope struct {
	ids     map[string]struct{}
	lastTry map[string]int
	parent  *tempScope
}

func newTempScope(parent *tempScope) *tempScope {
	return &tempScope{ids: map[string]struct{}{}, lastTry: map[string]int{}, parent: parent}
}

func (s *tempScope) has(id string) bool {
	if _, ok := s.ids[id]; ok {
		return true
	}
	if s.parent != nil {
		return s.parent.has(id)
	}
	return false
}

func isFullyScopedNode(node luau.Node) bool {
	switch node.Kind() {
	case luau.KindForStatement, luau.KindNumericForStatement:
		return true
	}
	return luau.IsFunctionLike(node)
}

func isScopeEdge(node luau.Node, head bool) bool {
	parent := node.Parent()
	if parent == nil {
		return false
	}
	edgeOf := func(l *luau.List[luau.Statement]) luau.Statement {
		if head {
			if l.Head != nil {
				return l.Head.Value
			}
		} else {
			if l.Tail != nil {
				return l.Tail.Value
			}
		}
		return nil
	}
	if luau.HasStatements(parent) {
		if stmts := luau.StatementsOf(parent); stmts != nil {
			if edge := edgeOf(stmts); edge != nil && luau.Node(edge) == node {
				return true
			}
		}
	}
	if ifStmt, ok := parent.(*luau.IfStatement); ok {
		if elseList, ok := ifStmt.ElseBody.(*luau.List[luau.Statement]); ok {
			if edge := edgeOf(elseList); edge != nil && luau.Node(edge) == node {
				return true
			}
		}
	}
	return false
}

func solveTempIDs(state *RenderState, ast luau.NodeOrList) {
	var tempIDsToProcess []*luau.TemporaryIdentifier
	nodesToScopes := map[*luau.TemporaryIdentifier]*tempScope{}

	scopeStack := []*tempScope{newTempScope(nil)}
	peek := func() *tempScope { return scopeStack[len(scopeStack)-1] }
	push := func() { scopeStack = append(scopeStack, newTempScope(peek())) }
	pop := func() { scopeStack = scopeStack[:len(scopeStack)-1] }
	registerID := func(name string) { peek().ids[name] = struct{}{} }

	vis := &visitor{
		before: func(node luau.Node) {
			if isFullyScopedNode(node) {
				push()
			}
			if isScopeEdge(node, true) {
				push()
			}
			switch n := node.(type) {
			case *luau.TemporaryIdentifier:
				nodesToScopes[n] = peek()
				tempIDsToProcess = append(tempIDsToProcess, n)
			case *luau.VariableDeclaration:
				switch l := n.Left.(type) {
				case *luau.List[luau.AnyIdentifier]:
					l.ForEach(func(id luau.AnyIdentifier) {
						if ident, ok := id.(*luau.Identifier); ok {
							registerID(ident.Name)
						}
					})
				case *luau.Identifier:
					registerID(l.Name)
				}
			default:
				if luau.IsFunctionLike(node) {
					params, _ := node.(luau.HasParameters).ParamData()
					params.ForEach(func(id luau.AnyIdentifier) {
						if ident, ok := id.(*luau.Identifier); ok {
							registerID(ident.Name)
						}
					})
				}
			}
		},
		after: func(node luau.Node) {
			if isFullyScopedNode(node) {
				pop()
			}
			if isScopeEdge(node, false) {
				pop()
			}
		},
	}
	visitNodeOrList(ast, vis)

	for _, tempID := range tempIDsToProcess {
		if _, done := state.seenTempNodes[tempID.ID]; done {
			continue
		}
		scope := nodesToScopes[tempID]
		separator := "_"
		if tempID.Name == "" {
			separator = ""
		}
		input := "_" + tempID.Name
		i, ok := scope.lastTry[input]
		if !ok {
			i = 1
		}
		for scope.has(input) {
			input = "_" + tempID.Name + separator + strconv.Itoa(i)
			i++
		}
		scope.lastTry[input] = i
		scope.ids[input] = struct{}{}
		state.seenTempNodes[tempID.ID] = input
	}
}
```

Note the subtle upstream behavior preserved: `lastTry` is keyed by the ORIGINAL `input` value before the loop mutates it (upstream `scope.lastTry.get(input)` reads pre-loop, `scope.lastTry.set(input, i)` writes post-loop key). Port this exactly as upstream does — re-read `solveTempIds.ts` lines 116-133 against this code during review; if they differ, upstream wins.

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/luau/render/ -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add internal\luau && git commit -m "luau/render: scope-aware temp identifier solver"
```

### Task 13: Expression renderers

**Files:**

- Create: `internal/luau/render/expressions.go`, `internal/luau/render/fields.go`
- Modify: `internal/luau/render/render.go` (dispatcher gains expression cases)
- Test: `internal/luau/render/expressions_test.go`
- Reference: `LuauRenderer/nodes/expressions/**/*.ts`, `LuauRenderer/nodes/fields/*.ts`
- [ ] **Step 1: Write the failing test**

`internal/luau/render/expressions_test.go`:

```go
package render

import (
	"testing"

	"rotor/internal/luau"
)

func renderExpr(t *testing.T, e luau.Expression) string {
	t.Helper()
	s := NewRenderState()
	solveTempIDs(s, e)
	return Render(s, e)
}

func exprList(es ...luau.Expression) *luau.List[luau.Expression] {
	return luau.NewList[luau.Expression](es...)
}

func TestRenderLiterals(t *testing.T) {
	cases := []struct {
		node luau.Expression
		want string
	}{
		{luau.Nil(), "nil"},
		{luau.Bool(true), "true"},
		{luau.Bool(false), "false"},
		{luau.NewVarArgs(), "..."},
		{luau.Num(5), "5"},
		{luau.NewNumberLiteral("1_000"), "1_000"},
		{luau.NewNumberLiteral("0xFF"), "0xFF"},
		{luau.Str("hi"), `"hi"`},
		{luau.Str(`say "hi"`), `'say "hi"'`},
		{luau.Str("both \" and '"), `[[both " and ']]`},
		{luau.Str("multi\nline"), "[[multi\nline]]"},
	}
	for _, c := range cases {
		if got := renderExpr(t, c.node); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestRenderIndexing(t *testing.T) {
	cases := []struct {
		node luau.Expression
		want string
	}{
		{luau.NewPropertyAccess(luau.ID("a"), "b"), "a.b"},
		{luau.NewPropertyAccess(luau.ID("a"), "not valid"), `a["not valid"]`},
		{luau.NewComputedIndex(luau.ID("a"), luau.Str("b")), "a.b"},
		{luau.NewComputedIndex(luau.ID("a"), luau.Num(1)), "a[1]"},
		{luau.NewCall(luau.ID("f"), exprList(luau.Num(1), luau.Num(2))), "f(1, 2)"},
		{luau.NewMethodCall("m", luau.ID("obj"), exprList()), "obj:m()"},
	}
	for _, c := range cases {
		if got := renderExpr(t, c.node); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestRenderOperators(t *testing.T) {
	cases := []struct {
		node luau.Expression
		want string
	}{
		{luau.NewBinary(luau.NewBinary(luau.ID("a"), "+", luau.ID("b")), "*", luau.ID("c")), "(a + b) * c"},
		{luau.NewBinary(luau.ID("a"), "-", luau.NewBinary(luau.ID("b"), "-", luau.ID("c"))), "a - (b - c)"},
		{luau.NewUnary("not", luau.ID("a")), "not a"},
		{luau.NewUnary("-", luau.NewUnary("-", luau.ID("a"))), "- -a"},
		{luau.NewUnary("#", luau.ID("t")), "#t"},
		{luau.NewIfExpression(luau.ID("c"), luau.Num(1), luau.Num(2)), "if c then 1 else 2"},
	}
	for _, c := range cases {
		if got := renderExpr(t, c.node); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestRenderParenthesized(t *testing.T) {
	// parens around simple expressions are dropped
	if got := renderExpr(t, luau.NewParenthesized(luau.ID("x"))); got != "x" {
		t.Errorf("got %q", got)
	}
	bin := luau.NewBinary(luau.ID("a"), "+", luau.ID("b"))
	if got := renderExpr(t, luau.NewParenthesized(bin)); got != "(a + b)" {
		t.Errorf("got %q", got)
	}
}

func TestRenderTables(t *testing.T) {
	if got := renderExpr(t, luau.NewArray(exprList())); got != "{}" {
		t.Errorf("empty array got %q", got)
	}
	if got := renderExpr(t, luau.NewArray(exprList(luau.Num(1), luau.Num(2)))); got != "{ 1, 2 }" {
		t.Errorf("array got %q", got)
	}
	if got := renderExpr(t, luau.NewSet(exprList(luau.Str("a")))); got != "{\n\t[\"a\"] = true,\n}" {
		t.Errorf("set got %q", got)
	}
	m := luau.NewMap(luau.NewList(
		luau.NewMapField(luau.Str("foo"), luau.Num(1)),
		luau.NewMapField(luau.Num(2), luau.Num(3)),
	))
	if got := renderExpr(t, m); got != "{\n\tfoo = 1,\n\t[2] = 3,\n}" {
		t.Errorf("map got %q", got)
	}
}

func TestRenderInterpolatedString(t *testing.T) {
	parts := luau.NewList[luau.Node](
		luau.NewInterpolatedStringPart("a"),
		luau.ID("b"),
		luau.NewInterpolatedStringPart(" {c} "),
	)
	want := "`a{b} \\{c\\} `"
	if got := renderExpr(t, luau.NewInterpolatedString(parts)); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRenderFunctionExpression(t *testing.T) {
	empty := luau.NewFunctionExpression(luau.NewList[luau.AnyIdentifier](), false, luau.NewList[luau.Statement]())
	if got := renderExpr(t, empty); got != "function() end" {
		t.Errorf("got %q", got)
	}
	body := luau.NewList[luau.Statement](luau.NewReturn(luau.ID("x")))
	params := luau.NewList[luau.AnyIdentifier](luau.ID("x"))
	fn := luau.NewFunctionExpression(params, true, body)
	want := "function(x, ...)\n\treturn x\nend"
	if got := renderExpr(t, fn); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/luau/render/ -v`
Expected: FAIL — Render panics "no renderer".

- [ ] **Step 3: Implement expressions.go + fields.go and wire the dispatcher**

Rewrite `render.go` as a full type switch (statement cases panic until Task 14):

```go
package render

import "rotor/internal/luau"

func Render(s *RenderState, node luau.Node) string {
	switch n := node.(type) {
	// indexable expressions
	case *luau.Identifier:
		return renderIdentifier(s, n)
	case *luau.TemporaryIdentifier:
		return renderTemporaryIdentifier(s, n)
	case *luau.ComputedIndexExpression:
		return renderComputedIndexExpression(s, n)
	case *luau.PropertyAccessExpression:
		return renderPropertyAccessExpression(s, n)
	case *luau.CallExpression:
		return renderCallExpression(s, n)
	case *luau.MethodCallExpression:
		return renderMethodCallExpression(s, n)
	case *luau.ParenthesizedExpression:
		return renderParenthesizedExpression(s, n)
	// expressions
	case *luau.None:
		panic("Cannot render None")
	case *luau.NilLiteral:
		return "nil"
	case *luau.FalseLiteral:
		return "false"
	case *luau.TrueLiteral:
		return "true"
	case *luau.NumberLiteral:
		return renderNumberLiteral(s, n)
	case *luau.StringLiteral:
		return renderStringLiteral(s, n)
	case *luau.VarArgsLiteral:
		return "..."
	case *luau.FunctionExpression:
		return renderFunctionExpression(s, n)
	case *luau.BinaryExpression:
		return renderBinaryExpression(s, n)
	case *luau.UnaryExpression:
		return renderUnaryExpression(s, n)
	case *luau.IfExpression:
		return renderIfExpression(s, n)
	case *luau.InterpolatedString:
		return renderInterpolatedString(s, n)
	case *luau.Array:
		return renderArray(s, n)
	case *luau.Map:
		return renderMap(s, n)
	case *luau.Set:
		return renderSet(s, n)
	case *luau.MixedTable:
		return renderMixedTable(s, n)
	// statements (Task 14)
	case *luau.Assignment:
		return renderAssignment(s, n)
	case *luau.BreakStatement:
		return s.Line("break")
	case *luau.CallStatement:
		return renderCallStatement(s, n)
	case *luau.ContinueStatement:
		return s.Line("continue")
	case *luau.DoStatement:
		return renderDoStatement(s, n)
	case *luau.WhileStatement:
		return renderWhileStatement(s, n)
	case *luau.RepeatStatement:
		return renderRepeatStatement(s, n)
	case *luau.IfStatement:
		return renderIfStatement(s, n)
	case *luau.NumericForStatement:
		return renderNumericForStatement(s, n)
	case *luau.ForStatement:
		return renderForStatement(s, n)
	case *luau.FunctionDeclaration:
		return renderFunctionDeclaration(s, n)
	case *luau.MethodDeclaration:
		return renderMethodDeclaration(s, n)
	case *luau.VariableDeclaration:
		return renderVariableDeclaration(s, n)
	case *luau.ReturnStatement:
		return renderReturnStatement(s, n)
	case *luau.Comment:
		return renderComment(s, n)
	// fields
	case *luau.MapField:
		return renderMapField(s, n)
	case *luau.InterpolatedStringPart:
		return renderInterpolatedStringPart(s, n)
	}
	panic("render: no renderer for " + node.Kind().String())
}
```

(In this task, implement the expression + field functions; declare the statement functions in `statements.go` as panicking stubs so the package compiles, e.g. `func renderAssignment(s *RenderState, n *luau.Assignment) string { panic("TODO Task 14") }` — Task 14 replaces every stub.)

`expressions.go` — each function is a direct port; the reference file is named in a comment:

```go
package render

import (
	"regexp"
	"strconv"
	"strings"

	"rotor/internal/luau"
)

// renderIdentifier.ts
func renderIdentifier(s *RenderState, node *luau.Identifier) string {
	if !luau.IsValidIdentifier(node.Name) {
		panic("Invalid Luau Identifier: \"" + node.Name + "\"")
	}
	return node.Name
}

// renderTemporaryIdentifier.ts
func renderTemporaryIdentifier(s *RenderState, node *luau.TemporaryIdentifier) string {
	name := s.getTempName(node)
	if !luau.IsValidIdentifier(name) {
		panic("Invalid Temporary Identifier: \"" + name + "\"")
	}
	return name
}

// renderComputedIndexExpression.ts
func renderComputedIndexExpression(s *RenderState, node *luau.ComputedIndexExpression) string {
	expStr := Render(s, node.Expression)
	if str, ok := node.Index.(*luau.StringLiteral); ok && luau.IsValidIdentifier(str.Value) {
		return expStr + "." + str.Value
	}
	return expStr + "[" + Render(s, node.Index) + "]"
}

// renderPropertyAccessExpression.ts
func renderPropertyAccessExpression(s *RenderState, node *luau.PropertyAccessExpression) string {
	expStr := Render(s, node.Expression)
	if luau.IsValidIdentifier(node.Name) {
		return expStr + "." + node.Name
	}
	return expStr + "[\"" + node.Name + "\"]"
}

// renderCallExpression.ts
func renderCallExpression(s *RenderState, node *luau.CallExpression) string {
	return Render(s, node.Expression) + "(" + renderArguments(s, node.Args) + ")"
}

// renderMethodCallExpression.ts
func renderMethodCallExpression(s *RenderState, node *luau.MethodCallExpression) string {
	if !luau.IsValidIdentifier(node.Name) {
		panic("invalid method name: " + node.Name)
	}
	return Render(s, node.Expression) + ":" + node.Name + "(" + renderArguments(s, node.Args) + ")"
}

// renderParenthesizedExpression.ts
func renderParenthesizedExpression(s *RenderState, node *luau.ParenthesizedExpression) string {
	expression := node.Expression
	for {
		if p, ok := expression.(*luau.ParenthesizedExpression); ok {
			expression = p.Expression
		} else {
			break
		}
	}
	if luau.IsSimple(expression) {
		return Render(s, node.Expression)
	}
	return "(" + Render(s, node.Expression) + ")"
}

// renderNumberLiteral.ts
var underscoreRegex = regexp.MustCompile(`_`)

func renderNumberLiteral(s *RenderState, node *luau.NumberLiteral) string {
	if luau.IsValidNumberLiteral(node.Value) {
		return node.Value
	}
	cleaned := underscoreRegex.ReplaceAllString(node.Value, "")
	f, err := strconv.ParseFloat(cleaned, 64)
	if err != nil {
		panic("invalid number literal: " + node.Value)
	}
	return strconv.FormatFloat(f, 'f', -1, 64)
}

// renderStringLiteral.ts
func needsBracketSpacing(node *luau.StringLiteral) bool {
	parent := node.Parent()
	if parent == nil {
		return false
	}
	switch p := parent.(type) {
	case *luau.MapField:
		return luau.Node(node) == luau.Node(p.Index)
	case *luau.ComputedIndexExpression:
		return luau.Node(node) == luau.Node(p.Index)
	case *luau.Set:
		return true
	}
	return false
}

func renderStringLiteral(s *RenderState, node *luau.StringLiteral) string {
	isMultiline := strings.Contains(node.Value, "\n")
	if !isMultiline && !strings.Contains(node.Value, "\"") {
		return "\"" + node.Value + "\""
	}
	if !isMultiline && !strings.Contains(node.Value, "'") {
		return "'" + node.Value + "'"
	}
	eqStr := getSafeBracketEquals(node.Value)
	spacing := ""
	if needsBracketSpacing(node) {
		spacing = " "
	}
	return spacing + "[" + eqStr + "[" + node.Value + "]" + eqStr + "]" + spacing
}

// renderFunctionExpression.ts
func renderFunctionExpression(s *RenderState, node *luau.FunctionExpression) string {
	if node.Statements.IsEmpty() {
		return "function(" + renderParameters(s, node) + ") end"
	}
	result := "function(" + renderParameters(s, node) + ")\n"
	result += s.Block(func() string { return renderStatements(s, node.Statements) })
	result += s.Indented("end")
	return result
}

// renderBinaryExpression.ts
func renderBinaryExpression(s *RenderState, node *luau.BinaryExpression) string {
	result := Render(s, node.Left) + " " + string(node.Operator) + " " + Render(s, node.Right)
	if needsParentheses(node) {
		result = "(" + result + ")"
	}
	return result
}

// renderUnaryExpression.ts
func unaryNeedsSpace(node *luau.UnaryExpression) bool {
	if node.Operator == "not" {
		return true
	}
	if inner, ok := node.Expression.(*luau.UnaryExpression); ok && inner.Operator == "-" {
		return true
	}
	return false
}

func renderUnaryExpression(s *RenderState, node *luau.UnaryExpression) string {
	opStr := string(node.Operator)
	if unaryNeedsSpace(node) {
		opStr += " "
	}
	result := opStr + Render(s, node.Expression)
	if needsParentheses(node) {
		result = "(" + result + ")"
	}
	return result
}

// renderIfExpression.ts
func renderIfExpression(s *RenderState, node *luau.IfExpression) string {
	result := "if " + Render(s, node.Condition) + " then " + Render(s, node.Expression) + " "
	var currentAlternative luau.Expression = node.Alternative
	for {
		ifExp, ok := currentAlternative.(*luau.IfExpression)
		if !ok {
			break
		}
		result += "elseif " + Render(s, ifExp.Condition) + " then " + Render(s, ifExp.Expression) + " "
		currentAlternative = ifExp.Alternative
	}
	result += "else " + Render(s, currentAlternative)
	if needsParentheses(node) {
		result = "(" + result + ")"
	}
	return result
}

// renderInterpolatedString.ts
func renderInterpolatedString(s *RenderState, node *luau.InterpolatedString) string {
	var b strings.Builder
	b.WriteString("`")
	node.Parts.ForEach(func(part luau.Node) {
		expressionStr := Render(s, part)
		if _, isPart := part.(*luau.InterpolatedStringPart); isPart {
			b.WriteString(expressionStr)
		} else {
			b.WriteString("{")
			if luau.IsTable(part) {
				expressionStr = "(" + expressionStr + ")"
			}
			b.WriteString(expressionStr)
			b.WriteString("}")
		}
	})
	b.WriteString("`")
	return b.String()
}

// renderArray.ts
func renderArray(s *RenderState, node *luau.Array) string {
	if node.Members.IsEmpty() {
		return "{}"
	}
	parts := []string{}
	node.Members.ForEach(func(m luau.Expression) { parts = append(parts, Render(s, m)) })
	return "{ " + strings.Join(parts, ", ") + " }"
}

// renderMap.ts
func renderMap(s *RenderState, node *luau.Map) string {
	if node.Fields.IsEmpty() {
		return "{}"
	}
	result := "{\n"
	s.Block(func() string {
		node.Fields.ForEach(func(f *luau.MapField) { result += s.Line(Render(s, f) + ",") })
		return ""
	})
	result += s.Indented("}")
	return result
}

// renderSet.ts
func renderSet(s *RenderState, node *luau.Set) string {
	if node.Members.IsEmpty() {
		return "{}"
	}
	result := "{\n"
	s.Block(func() string {
		node.Members.ForEach(func(m luau.Expression) {
			result += s.Line("[" + Render(s, m) + "] = true,")
		})
		return ""
	})
	result += s.Indented("}")
	return result
}

// renderMixedTable.ts
func renderMixedTable(s *RenderState, node *luau.MixedTable) string {
	if node.Fields.IsEmpty() {
		return "{}"
	}
	result := "{\n"
	s.Block(func() string {
		node.Fields.ForEach(func(f luau.Node) { result += s.Line(Render(s, f) + ",") })
		return ""
	})
	result += s.Indented("}")
	return result
}
```

`fields.go`:

```go
package render

import (
	"regexp"

	"rotor/internal/luau"
)

// renderMapField.ts
func renderMapField(s *RenderState, node *luau.MapField) string {
	valueStr := Render(s, node.Value)
	if str, ok := node.Index.(*luau.StringLiteral); ok && luau.IsValidIdentifier(str.Value) {
		return str.Value + " = " + valueStr
	}
	return "[" + Render(s, node.Index) + "] = " + valueStr
}

// renderInterpolatedStringPart.ts
var (
	bracePartRegex   = regexp.MustCompile(`(\\u\{[a-fA-F0-9]+\})|([{}])`)
	newlinePartRegex = regexp.MustCompile("(\r\n?|\n)")
)

func renderInterpolatedStringPart(s *RenderState, node *luau.InterpolatedStringPart) string {
	text := bracePartRegex.ReplaceAllStringFunc(node.Text, func(m string) string {
		if m == "{" || m == "}" {
			return "\\" + m
		}
		return m // unicode escape untouched
	})
	return newlinePartRegex.ReplaceAllString(text, "\\$1")
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./internal/luau/render/ -v`
Expected: PASS for all expression tests; statement stubs untouched.

- [ ] **Step 5: Commit**

```powershell
git add internal\luau && git commit -m "luau/render: expression and field renderers"
```

### Task 14: Statement renderers + RenderAST

**Files:**

- Create: `internal/luau/render/statements.go` (replace stubs)
- Modify: `internal/luau/render/render.go` (add `RenderAST`)
- Test: `internal/luau/render/statements_test.go`
- Reference: `LuauRenderer/nodes/statements/*.ts`, `LuauRenderer/render.ts` (renderAST)
- [ ] **Step 1: Write the failing test**

`internal/luau/render/statements_test.go`:

```go
package render

import (
	"testing"

	"rotor/internal/luau"
)

func stmts(ss ...luau.Statement) *luau.List[luau.Statement] {
	return luau.NewList[luau.Statement](ss...)
}

func TestRenderVariableDeclarationAndAssignment(t *testing.T) {
	cases := []struct {
		ast  *luau.List[luau.Statement]
		want string
	}{
		{
			stmts(luau.NewVariableDeclaration(luau.ID("x"), luau.Num(5))),
			"local x = 5\n",
		},
		{
			stmts(luau.NewVariableDeclaration(luau.ID("x"), nil)),
			"local x\n",
		},
		{
			stmts(luau.NewVariableDeclaration(
				luau.NewList[luau.AnyIdentifier](luau.ID("a"), luau.ID("b")),
				luau.NewList[luau.Expression](luau.Num(1), luau.Num(2)),
			)),
			"local a, b = 1, 2\n",
		},
		{
			stmts(luau.NewAssignment(luau.ID("x"), "+=", luau.Num(1))),
			"x += 1\n",
		},
	}
	for _, c := range cases {
		if got := RenderAST(c.ast); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestRenderIfStatement(t *testing.T) {
	inner := stmts(luau.NewCallStatement(luau.NewCall(luau.ID("print"), exprList(luau.Str("big")))))
	elseInner := stmts(luau.NewCallStatement(luau.NewCall(luau.ID("print"), exprList(luau.Str("small")))))
	ast := stmts(luau.NewIf(luau.NewBinary(luau.ID("x"), ">", luau.Num(3)), inner, elseInner))
	want := "if x > 3 then\n\tprint(\"big\")\nelse\n\tprint(\"small\")\nend\n"
	if got := RenderAST(ast); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRenderElseIfChain(t *testing.T) {
	elseif := luau.NewIf(luau.ID("b"), stmts(luau.NewCallStatement(luau.NewCall(luau.ID("g"), exprList()))), nil)
	ast := stmts(luau.NewIf(luau.ID("a"), stmts(luau.NewCallStatement(luau.NewCall(luau.ID("f"), exprList()))), elseif))
	want := "if a then\n\tf()\nelseif b then\n\tg()\nend\n"
	if got := RenderAST(ast); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestRenderLoops(t *testing.T) {
	body := stmts(luau.NewCallStatement(luau.NewCall(luau.ID("f"), exprList())))
	cases := []struct {
		ast  *luau.List[luau.Statement]
		want string
	}{
		{
			stmts(luau.NewNumericFor(luau.ID("i"), luau.Num(1), luau.Num(10), nil, body.Clone())),
			"for i = 1, 10 do\n\tf()\nend\n",
		},
		{
			stmts(luau.NewNumericFor(luau.ID("i"), luau.Num(1), luau.Num(10), luau.Num(1), body.Clone())),
			"for i = 1, 10 do\n\tf()\nend\n", // step of 1 omitted
		},
		{
			stmts(luau.NewNumericFor(luau.ID("i"), luau.Num(1), luau.Num(10), luau.Num(2), body.Clone())),
			"for i = 1, 10, 2 do\n\tf()\nend\n",
		},
		{
			stmts(luau.NewFor(
				luau.NewList[luau.AnyIdentifier](luau.ID("k"), luau.ID("v")),
				luau.NewCall(luau.ID("pairs"), exprList(luau.ID("t"))),
				body.Clone(),
			)),
			"for k, v in pairs(t) do\n\tf()\nend\n",
		},
		{
			stmts(luau.NewFor(luau.NewList[luau.AnyIdentifier](), luau.ID("it"), body.Clone())),
			"for _ in it do\n\tf()\nend\n", // empty ids render as _
		},
		{
			stmts(luau.NewWhile(luau.Bool(true), body.Clone())),
			"while true do\n\tf()\nend\n",
		},
		{
			stmts(luau.NewRepeat(luau.ID("done"), body.Clone())),
			"repeat\n\tf()\nuntil done\n",
		},
	}
	for _, c := range cases {
		if got := RenderAST(c.ast); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestRenderFunctions(t *testing.T) {
	body := stmts(luau.NewReturn(luau.ID("x")))
	params := luau.NewList[luau.AnyIdentifier](luau.ID("x"))
	ast := stmts(luau.NewFunctionDeclaration(true, luau.ID("f"), params, false, body))
	want := "local function f(x)\n\treturn x\nend\n"
	if got := RenderAST(ast); got != want {
		t.Errorf("got %q, want %q", got, want)
	}

	mBody := stmts(luau.NewReturn(luau.Nil()))
	m := luau.NewMethodDeclaration(luau.ID("Class"), "method", luau.NewList[luau.AnyIdentifier](), false, mBody)
	wantM := "function Class:method()\n\treturn nil\nend\n"
	if got := RenderAST(stmts(m)); got != wantM {
		t.Errorf("got %q, want %q", got, wantM)
	}
}

func TestRenderComment(t *testing.T) {
	if got := RenderAST(stmts(luau.NewComment("hello"))); got != "--hello\n" {
		t.Errorf("got %q", got)
	}
	want := "--[[\n\tline1\n\tline2\n]]\n"
	if got := RenderAST(stmts(luau.NewComment("line1\nline2"))); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestSemicolonAmbiguity(t *testing.T) {
	// local a = b followed by a statement starting with parenthesis needs `;`
	stmt1 := luau.NewVariableDeclaration(luau.ID("a"), luau.ID("b"))
	paren := luau.NewParenthesized(luau.NewBinary(luau.ID("c"), "+", luau.ID("d")))
	access := luau.NewPropertyAccess(paren, "e")
	stmt2 := luau.NewAssignment(access, "=", luau.ID("f"))
	want := "local a = b;\n(c + d).e = f\n"
	if got := RenderAST(stmts(stmt1, stmt2)); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestTempIdsEndToEnd(t *testing.T) {
	tmp := luau.TempID("result")
	ast := stmts(
		luau.NewVariableDeclaration(tmp, luau.Num(1)),
		luau.NewCallStatement(luau.NewCall(luau.ID("print"), exprList(tmp))),
	)
	want := "local _result = 1\nprint(_result)\n"
	if got := RenderAST(ast); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestStatementAfterFinalPanics(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Error("must panic on statement after return")
		}
	}()
	RenderAST(stmts(luau.NewReturn(luau.Nil()), luau.NewBreak()))
}
```

Note on `TestSemicolonAmbiguity`: the reused `tmp` identifier in `TestTempIdsEndToEnd` is intentionally adopted twice — the second use is cloned by `adopt`, but BOTH the original and clone carry the same `ID`, so solveTempIds assigns them the same rendered name. This mirrors upstream exactly (clones share `id`).

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/luau/render/ -v`
Expected: FAIL — statement stubs panic.

- [ ] **Step 3: Implement statements.go**

```go
package render

import (
	"strings"

	"rotor/internal/luau"
)

func renderExprOrList(s *RenderState, v luau.NodeOrList) string {
	switch x := v.(type) {
	case *luau.List[luau.Expression]:
		if x.IsEmpty() {
			panic("empty expression list")
		}
		parts := []string{}
		x.ForEach(func(e luau.Expression) { parts = append(parts, Render(s, e)) })
		return strings.Join(parts, ", ")
	case *luau.List[luau.WritableExpression]:
		if x.IsEmpty() {
			panic("empty writable expression list")
		}
		parts := []string{}
		x.ForEach(func(e luau.WritableExpression) { parts = append(parts, Render(s, e)) })
		return strings.Join(parts, ", ")
	case *luau.List[luau.AnyIdentifier]:
		if x.IsEmpty() {
			panic("empty identifier list")
		}
		parts := []string{}
		x.ForEach(func(e luau.AnyIdentifier) { parts = append(parts, Render(s, e)) })
		return strings.Join(parts, ", ")
	case luau.Node:
		return Render(s, x)
	}
	panic("renderExprOrList: unsupported value")
}

// renderAssignment.ts
func renderAssignment(s *RenderState, node *luau.Assignment) string {
	leftStr := renderExprOrList(s, node.Left)
	rightStr := renderExprOrList(s, node.Right)
	return s.LineWithEnd(leftStr+" "+string(node.Operator)+" "+rightStr, node)
}

// renderCallStatement.ts
func renderCallStatement(s *RenderState, node *luau.CallStatement) string {
	return s.LineWithEnd(Render(s, node.Expression), node)
}

// renderComment.ts
func renderComment(s *RenderState, node *luau.Comment) string {
	lines := strings.Split(node.Text, "\n")
	if len(lines) > 1 {
		eqStr := getSafeBracketEquals(node.Text)
		result := s.Line("--[" + eqStr + "[")
		result += s.Block(func() string {
			var b strings.Builder
			for _, line := range lines {
				b.WriteString(s.Line(line))
			}
			return b.String()
		})
		result += s.Line("]" + eqStr + "]")
		return result
	}
	return s.Line("--" + node.Text)
}

// renderDoStatement.ts
func renderDoStatement(s *RenderState, node *luau.DoStatement) string {
	result := s.Line("do")
	result += s.Block(func() string { return renderStatements(s, node.Statements) })
	result += s.Line("end")
	return result
}

// renderWhileStatement.ts
func renderWhileStatement(s *RenderState, node *luau.WhileStatement) string {
	result := s.Line("while " + Render(s, node.Condition) + " do")
	result += s.Block(func() string { return renderStatements(s, node.Statements) })
	result += s.Line("end")
	return result
}

// renderRepeatStatement.ts
func renderRepeatStatement(s *RenderState, node *luau.RepeatStatement) string {
	result := s.Line("repeat")
	result += s.Block(func() string { return renderStatements(s, node.Statements) })
	result += s.Line("until " + Render(s, node.Condition))
	return result
}

// renderIfStatement.ts
func renderIfStatement(s *RenderState, node *luau.IfStatement) string {
	result := s.Line("if " + Render(s, node.Condition) + " then")
	result += s.Block(func() string { return renderStatements(s, node.Statements) })

	currentElseBody := node.ElseBody
	for {
		ifStmt, ok := currentElseBody.(*luau.IfStatement)
		if !ok {
			break
		}
		statements := ifStmt.Statements
		result += s.Line("elseif " + Render(s, ifStmt.Condition) + " then")
		result += s.Block(func() string { return renderStatements(s, statements) })
		currentElseBody = ifStmt.ElseBody
	}

	if elseList, ok := currentElseBody.(*luau.List[luau.Statement]); ok && elseList.IsNonEmpty() {
		result += s.Line("else")
		result += s.Block(func() string { return renderStatements(s, elseList) })
	}

	result += s.Line("end")
	return result
}

// renderNumericForStatement.ts
func renderNumericForStatement(s *RenderState, node *luau.NumericForStatement) string {
	predicateStr := Render(s, node.ID) + " = " + Render(s, node.Start) + ", " + Render(s, node.End)
	if node.Step != nil {
		isOne := false
		if lit, ok := node.Step.(*luau.NumberLiteral); ok {
			if f, err := parseNumberValue(lit.Value); err == nil && f == 1 {
				isOne = true
			}
		}
		if !isOne {
			predicateStr += ", " + Render(s, node.Step)
		}
	}
	result := s.Line("for " + predicateStr + " do")
	result += s.Block(func() string { return renderStatements(s, node.Statements) })
	result += s.Line("end")
	return result
}

// renderForStatement.ts
func renderForStatement(s *RenderState, node *luau.ForStatement) string {
	parts := []string{}
	node.IDs.ForEach(func(id luau.AnyIdentifier) { parts = append(parts, Render(s, id)) })
	idsStr := strings.Join(parts, ", ")
	if idsStr == "" {
		idsStr = "_"
	}
	result := s.Line("for " + idsStr + " in " + Render(s, node.Expression) + " do")
	result += s.Block(func() string { return renderStatements(s, node.Statements) })
	result += s.Line("end")
	return result
}

// renderFunctionDeclaration.ts
func renderFunctionDeclaration(s *RenderState, node *luau.FunctionDeclaration) string {
	if node.Localize {
		if _, ok := node.Name.(luau.AnyIdentifier); !ok {
			panic("local function cannot be a property")
		}
	}
	nameStr := Render(s, node.Name)
	localStr := ""
	if node.Localize {
		localStr = "local "
	}
	result := s.Line(localStr + "function " + nameStr + "(" + renderParameters(s, node) + ")")
	result += s.Block(func() string { return renderStatements(s, node.Statements) })
	result += s.Line("end")
	return result
}

// renderMethodDeclaration.ts
func renderMethodDeclaration(s *RenderState, node *luau.MethodDeclaration) string {
	result := s.Line("function " + Render(s, node.Expression) + ":" + node.Name + "(" + renderParameters(s, node) + ")")
	result += s.Block(func() string { return renderStatements(s, node.Statements) })
	result += s.Line("end")
	return result
}

// renderVariableDeclaration.ts
func renderVariableDeclaration(s *RenderState, node *luau.VariableDeclaration) string {
	leftStr := renderExprOrList(s, node.Left)
	if node.Right != nil {
		rightStr := renderExprOrList(s, node.Right)
		return s.LineWithEnd("local "+leftStr+" = "+rightStr, node)
	}
	return s.LineWithEnd("local "+leftStr, node)
}

// renderReturnStatement.ts
func renderReturnStatement(s *RenderState, node *luau.ReturnStatement) string {
	return s.Line("return " + renderExprOrList(s, node.Expression))
}
```

Add `parseNumberValue` to `util.go` (mirrors JS `Number(value)` for literal forms the compiler emits):

```go
func parseNumberValue(text string) (float64, error) {
	cleaned := strings.ReplaceAll(text, "_", "")
	return strconv.ParseFloat(cleaned, 64)
}
```

Add `RenderAST` to `render.go`:

```go
// RenderAST mirrors upstream renderAST(): solve temp ids, then render.
func RenderAST(ast *luau.List[luau.Statement]) string {
	s := NewRenderState()
	solveTempIDs(s, ast)
	return renderStatements(s, ast)
}
```

- [ ] **Step 4: Run all tests**

Run: `go test ./internal/... -v`
Expected: PASS — every test in `internal/luau` and `internal/luau/render`.

- [ ] **Step 5: Commit**

```powershell
git add internal\luau && git commit -m "luau/render: statement renderers and RenderAST"
```

### Task 15: Golden integration test + benchmark

**Files:**

- Test: `internal/luau/render/golden_test.go`

A composite program exercising nesting, scoping, temp ids, semicolons, and precedence in one tree — the kind of structure the transformer will emit.

- [ ] **Step 1: Write the failing test**

```go
package render

import (
	"strings"
	"testing"

	"rotor/internal/luau"
)

// buildGoldenAST constructs a program shaped like typical transformer output.
func buildGoldenAST() *luau.List[luau.Statement] {
	// local function map(array, callback)
	// 	local result = {}
	// 	for i, v in ipairs(array) do
	// 		result[i] = callback(v)
	// 	end
	// 	return result
	// end
	arrayID, callbackID := luau.ID("array"), luau.ID("callback")
	resultID := luau.ID("result")
	iID, vID := luau.ID("i"), luau.ID("v")
	fnBody := luau.NewList[luau.Statement](
		luau.NewVariableDeclaration(resultID, luau.NewArray(luau.NewList[luau.Expression]())),
		luau.NewFor(
			luau.NewList[luau.AnyIdentifier](iID, vID),
			luau.NewCall(luau.ID("ipairs"), luau.NewList[luau.Expression](arrayID)),
			luau.NewList[luau.Statement](
				luau.NewAssignment(
					luau.NewComputedIndex(resultID, iID),
					"=",
					luau.NewCall(callbackID, luau.NewList[luau.Expression](vID)),
				),
			),
		),
		luau.NewReturn(resultID),
	)
	mapDecl := luau.NewFunctionDeclaration(
		true, luau.ID("map"),
		luau.NewList[luau.AnyIdentifier](arrayID, callbackID), false,
		fnBody,
	)

	// local doubled = map({ 1, 2, 3 }, function(n)
	// 	return n * 2
	// end)
	nID := luau.ID("n")
	callbackFn := luau.NewFunctionExpression(
		luau.NewList[luau.AnyIdentifier](nID), false,
		luau.NewList[luau.Statement](luau.NewReturn(luau.NewBinary(nID, "*", luau.Num(2)))),
	)
	call := luau.NewCall(luau.ID("map"), luau.NewList[luau.Expression](
		luau.NewArray(luau.NewList[luau.Expression](luau.Num(1), luau.Num(2), luau.Num(3))),
		callbackFn,
	))
	doubledDecl := luau.NewVariableDeclaration(luau.ID("doubled"), call)

	// if #doubled > 0 and doubled[1] == 2 then print("ok") else error("bad") end
	cond := luau.NewBinary(
		luau.NewBinary(luau.NewUnary("#", luau.ID("doubled")), ">", luau.Num(0)),
		"and",
		luau.NewBinary(luau.NewComputedIndex(luau.ID("doubled"), luau.Num(1)), "==", luau.Num(2)),
	)
	check := luau.NewIf(cond,
		luau.NewList[luau.Statement](luau.NewCallStatement(
			luau.NewCall(luau.ID("print"), luau.NewList[luau.Expression](luau.Str("ok"))))),
		luau.NewList[luau.Statement](luau.NewCallStatement(
			luau.NewCall(luau.ID("error"), luau.NewList[luau.Expression](luau.Str("bad"))))),
	)

	return luau.NewList[luau.Statement](mapDecl, doubledDecl, check)
}

const goldenWant = `local function map(array, callback)
	local result = {}
	for i, v in ipairs(array) do
		result[i] = callback(v)
	end
	return result
end
local doubled = map({ 1, 2, 3 }, function(n)
	return n * 2
end)
if #doubled > 0 and doubled[1] == 2 then
	print("ok")
else
	error("bad")
end
`

func TestGoldenProgram(t *testing.T) {
	got := RenderAST(buildGoldenAST())
	if got != goldenWant {
		t.Errorf("golden mismatch:\n--- got ---\n%s\n--- want ---\n%s", got, goldenWant)
		// first-diff helper
		g, w := got, goldenWant
		for i := 0; i < len(g) && i < len(w); i++ {
			if g[i] != w[i] {
				t.Errorf("first diff at byte %d: got %q, want %q (context: %q)", i, g[i], w[i], g[max(0, i-20):min(len(g), i+20)])
				break
			}
		}
	}
	if strings.Contains(got, "\r") {
		t.Error("output must never contain carriage returns")
	}
}

func BenchmarkRenderGolden(b *testing.B) {
	for b.Loop() {
		ast := buildGoldenAST()
		_ = RenderAST(ast)
	}
}
```

(Go ≥1.24 has `b.Loop()`; if on an older toolchain use the classic `for i := 0; i < b.N; i++` form. `max`/`min` builtins need Go ≥1.21.)

- [ ] **Step 2: Run the test**

Run: `go test ./internal/luau/render/ -run TestGoldenProgram -v`
Expected: PASS. If it fails, debug with the byte-diff output — the most likely culprits are `end` line endings after blocks (renderFunctionExpression ends with `s.Indented("end")` with NO newline — the enclosing statement adds it) and the parenthesized-call-argument spacing.

**Important check while debugging:** `local doubled = map(...)` — the multi-line function expression inside a variable declaration: `renderVariableDeclaration` calls `s.LineWithEnd(...)` which prefixes the CURRENT indent and appends `\n` — the function body inside already rendered with deeper indents because `renderFunctionExpression` emitted `\n` + block. This composition is exactly how upstream produces `end)` on the last line. If your output shows `end)` indented wrong, compare against upstream's behavior by reading `renderFunctionExpression.ts` again — `state.indented("end")` uses the CURRENT indent (depth of the statement), producing `end` at statement depth followed by `)` from the call wrapper.

- [ ] **Step 3: Run the benchmark**

Run: `go test ./internal/luau/render/ -bench BenchmarkRenderGolden -benchmem`
Expected: completes; record ns/op and allocs/op in the commit message. No optimization work now — this is the baseline that Phase 2's differential corpus will guard.

- [ ] **Step 4: Run everything + vet + format check**

Run: `go test ./internal/... && go vet ./internal/... ./tools/... && gofmt -l internal tools`
Expected: tests pass, vet clean, gofmt prints nothing.

- [ ] **Step 5: Commit**

```powershell
git add internal\luau && git commit -m "luau/render: golden program test and render benchmark"
```

---

## Done criteria for this plan

1. `go test ./internal/...` fully green.
2. The Phase 0 spike proves: parse + typecheck + `GetTypeAtLocation` + `TypeToString` from Go against the vendored tsgo mirror.
3. `internal/luau` + `internal/luau/render` provide the complete Luau AST/factory/renderer surface that Phase 2's `TransformState` will target, with upstream-faithful semantics (clone-on-reparent, temp-id solving, semicolon ambiguity, precedence parens).
4. Benchmarks exist as a baseline.

**Next plan (Phase 2):** TransformState + prereq stack, the differential harness against `rbxtsc` (which makes output fidelity continuously verified — the golden test here is a bootstrap, not the real conformance gate), and the first expression/statement transforms.
