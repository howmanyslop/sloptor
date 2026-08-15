# rotor Phase 5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the vendored upstream roblox-ts test corpus into rotor's Phase 5 proof harness: diagnostics corpus parity, byte-diff conformance over upstream test sources, runtime behavioral execution under Rojo + Lune, and an acceptance runner for the real `randomness` project.

**Architecture:** Keep Phase 5 isolated behind `internal/conformance` and `tools/oracle`. The first slice is the diagnostics corpus because it is local, deterministic, and independent of Lune. The second slice extends the existing conformance corpus from an empty manifest into categorized, gradually enableable byte-diff tests. The third slice adds a runtime harness that shells out to `rojo` and `lune` only when the tools are present, with explicit skip messages when they are not.

**Tech Stack:** Go test harnesses, existing `internal/compile` APIs, vendored roblox-ts test corpus under `testdata/conformance`, PowerShell oracle scripts, `rojo`, `lune`, `npm`.

---

## Task 1: Diagnostics corpus harness

**Files:**

- Create: `internal/conformance/diagnostics_test.go`
- Create: `internal/conformance/diagnostics_manifest.go`
- Modify: `testdata/conformance/README.md`
- Test: `internal/conformance/diagnostics_test.go`
- [ ] **Step 1: Write the failing diagnostics harness test**

```go
func TestDiagnosticsCorpus(t *testing.T) {
	root := repoRoot(t)
	dir := filepath.Join(root, "testdata", "conformance", "excluded", "diagnostics")

	cases, err := loadDiagnosticFixtures(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(cases) == 0 {
		t.Fatal("no diagnostic fixtures found")
	}

	for _, tc := range cases {
		t.Run(tc.Name, func(t *testing.T) {
			got, err := compileDiagnosticFixture(root, tc.Path)
			if err != nil {
				t.Fatal(err)
			}
			if len(got) == 0 {
				t.Fatalf("expected diagnostic %q, got none", tc.ExpectedID)
			}
			for _, id := range got {
				if id != tc.ExpectedID {
					t.Fatalf("expected only %q, got %v", tc.ExpectedID, got)
				}
			}
		})
	}
}
```

- [ ] **Step 2: Run the new test and verify it fails**

Run:

```powershell
go test ./internal/conformance -run TestDiagnosticsCorpus -count=1
```

Expected: fail because `loadDiagnosticFixtures` / `compileDiagnosticFixture` do not exist yet.

- [ ] **Step 3: Add the manifest and fixture loader**

```go
type DiagnosticFixture struct {
	Name       string
	Path       string
	ExpectedID string
}

var DiagnosticIDByBaseName = map[string]string{
	"noAny": "noAny",
	"noArguments": "noArguments",
	// ...
}

func loadDiagnosticFixtures(dir string) ([]DiagnosticFixture, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	var out []DiagnosticFixture
	for _, entry := range entries {
		if entry.IsDir() {
			continue
		}
		name := entry.Name()
		if !strings.HasSuffix(name, ".ts") && !strings.HasSuffix(name, ".tsx") {
			continue
		}
		base := diagnosticBaseName(name)
		expected, ok := DiagnosticIDByBaseName[base]
		if !ok {
			return nil, fmt.Errorf("no manifest entry for %s", name)
		}
		out = append(out, DiagnosticFixture{
			Name:       name,
			Path:       filepath.Join(dir, name),
			ExpectedID: expected,
		})
	}
	slices.SortFunc(out, func(a, b DiagnosticFixture) int { return strings.Compare(a.Name, b.Name) })
	return out, nil
}
```

- [ ] **Step 4: Implement one-file diagnostic compilation**

```go
func compileDiagnosticFixture(root, fixturePath string) ([]string, error) {
	projectDir := filepath.Join(root, "testdata", "conformance", "project")
	rel, err := filepath.Rel(filepath.Join(projectDir, "src"), fixturePath)
	if err != nil {
		return nil, err
	}

	files := map[string]string{
		filepath.ToSlash(rel): mustReadFile(fixturePath),
		"services.d.ts":       mustReadFile(filepath.Join(projectDir, "src", "services.d.ts")),
	}

	_, diags, err := compile.CompileFile(files, filepath.ToSlash(rel))
	if err == nil && len(diags) == 0 {
		return nil, nil
	}
	return normalizeDiagnosticIDs(diags), nil
}
```

- [ ] **Step 5: Run the diagnostics test and make it pass**

Run:

```powershell
go test ./internal/conformance -run TestDiagnosticsCorpus -count=1
```

Expected: `PASS` with every fixture producing exactly one expected rotor diagnostic ID, or a small known-failure set that drives the next commit.

- [ ] **Step 6: Commit**

```powershell
git add internal/conformance/diagnostics_test.go internal/conformance/diagnostics_manifest.go testdata/conformance/README.md
git commit -m "conformance: add diagnostics corpus harness"
```

### Task 2: Upstream source byte-diff corpus enablement

**Files:**

- Modify: `internal/conformance/conformance_test.go`
- Modify: `internal/conformance/manifest.go`
- Modify: `testdata/conformance/README.md`
- Test: `internal/conformance/conformance_test.go`
- [ ] **Step 1: Write the failing categorized enablement test**

```go
func TestEnabledFixturesExist(t *testing.T) {
	for _, rel := range EnabledFixtures {
		if _, err := os.Stat(filepath.Join(goldenDir, filepath.FromSlash(rel))); err != nil {
			t.Fatalf("missing golden for %s: %v", rel, err)
		}
	}
}
```

- [ ] **Step 2: Run the test and verify it fails after seeding one manifest entry**

Run:

```powershell
go test ./internal/conformance -run TestEnabledFixturesExist -count=1
```

Expected: fail until `EnabledFixtures` contains a real entry and the helper variables are wired correctly.

- [ ] **Step 3: Split the manifest into explicit slices**

```go
var CoreFixtures = []string{
	"helpers/util/ClassWithInstanceFoo.luau",
	"helpers/util/ClassWithStaticFoo.luau",
	"main.server.luau",
}

var SpecFixtures = []string{
	"tests/literal.spec.luau",
	"tests/binary.spec.luau",
}

var EnabledFixtures = append(append([]string{}, CoreFixtures...), SpecFixtures...)
```

- [ ] **Step 4: Keep the compile-once diff test, but improve failure reporting**

```go
got, ok := out["out/"+rel]
if !ok {
	t.Fatalf("out/%s missing from CompileProject output; available keys: %v", rel, sortedKeys(out))
}
if got != string(want) {
	t.Errorf("output differs from rbxtsc golden")
	reportFirstDiff(t, got, string(want))
}
```

- [ ] **Step 5: Run the conformance diff suite**

Run:

```powershell
go test ./internal/conformance -run TestConformance -count=1 -v
```

Expected: pass for only the entries already known to be transformer-clean on the current branch; skip the rest explicitly.

- [ ] **Step 6: Commit**

```powershell
git add internal/conformance/conformance_test.go internal/conformance/manifest.go testdata/conformance/README.md
git commit -m "conformance: enable upstream byte-diff corpus"
```

### Task 3: Runtime behavioral suite runner

**Files:**

- Create: `internal/conformance/runtime_test.go`
- Create: `internal/conformance/runtime.go`
- Modify: `testdata/conformance/README.md`
- Test: `internal/conformance/runtime_test.go`
- [ ] **Step 1: Write the failing runtime tool-detection test**

```go
func TestRuntimeToolAvailability(t *testing.T) {
	tools := detectRuntimeTools()
	if tools.Rojo == "" {
		t.Fatal("rojo not detected")
	}
}
```

- [ ] **Step 2: Run the test and verify its current status**

Run:

```powershell
go test ./internal/conformance -run TestRuntimeToolAvailability -count=1
```

Expected: `PASS` for `rojo`; `lune` may be absent and should be represented as an empty path, not a hard failure.

- [ ] **Step 3: Implement tool detection and skip-aware runtime execution**

```go
type RuntimeTools struct {
	Rojo string
	Lune string
}

func detectRuntimeTools() RuntimeTools {
	return RuntimeTools{
		Rojo: lookPath("rojo"),
		Lune: lookPath("lune"),
	}
}

func runBehavioralSuite(t *testing.T, root string) {
	t.Helper()
	tools := detectRuntimeTools()
	if tools.Rojo == "" || tools.Lune == "" {
		t.Skipf("runtime suite requires rojo and lune; rojo=%q lune=%q", tools.Rojo, tools.Lune)
	}
	// build with rotor, rojo build the place, lune execute runTestsWithLune.lua
}
```

- [ ] **Step 4: Add the behavioral suite test**

```go
func TestBehavioralSuite(t *testing.T) {
	runBehavioralSuite(t, repoRoot(t))
}
```

- [ ] **Step 5: Run the runtime suite**

Run:

```powershell
go test ./internal/conformance -run TestBehavioralSuite -count=1 -v
```

Expected: `SKIP` until `lune` is installed; once installed, either `PASS` or actionable rotor/Phase 4 failures with command output surfaced in the test log.

- [ ] **Step 6: Commit**

```powershell
git add internal/conformance/runtime.go internal/conformance/runtime_test.go testdata/conformance/README.md
git commit -m "conformance: add runtime behavioral suite runner"
```

### Task 4: Acceptance runner for `randomness`

**Files:**

- Create: `internal/conformance/acceptance_test.go`
- Modify: `testdata/conformance/README.md`
- Test: `internal/conformance/acceptance_test.go`
- [ ] **Step 1: Write the failing acceptance-path resolution test**

```go
func TestAcceptanceProjectDiscovery(t *testing.T) {
	path := os.Getenv("ROTOR_RANDOMNESS_PATH")
	if path == "" {
		t.Skip("ROTOR_RANDOMNESS_PATH not set")
	}
	if _, err := os.Stat(filepath.Join(path, "tsconfig.json")); err != nil {
		t.Fatalf("randomness tsconfig missing: %v", err)
	}
}
```

- [ ] **Step 2: Run the test and verify it skips without local configuration**

Run:

```powershell
go test ./internal/conformance -run TestAcceptanceProjectDiscovery -count=1 -v
```

Expected: `SKIP` unless `ROTOR_RANDOMNESS_PATH` is configured.

- [ ] **Step 3: Add build-and-diff acceptance plumbing**

```go
func TestRandomnessAcceptance(t *testing.T) {
	path := os.Getenv("ROTOR_RANDOMNESS_PATH")
	if path == "" {
		t.Skip("ROTOR_RANDOMNESS_PATH not set")
	}
	out, diags, err := compile.CompileProject(path)
	if err != nil {
		t.Fatalf("CompileProject: %v (diags: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("unexpected diagnostics: %v", diags)
	}
	if len(out) == 0 {
		t.Fatal("no emitted files")
	}
}
```

- [ ] **Step 4: Run the acceptance test**

Run:

```powershell
go test ./internal/conformance -run TestRandomnessAcceptance -count=1 -v
```

Expected: `SKIP` by default, or a concrete emitted-file count / first failing diagnostic when the project is configured locally.

- [ ] **Step 5: Commit**

```powershell
git add internal/conformance/acceptance_test.go testdata/conformance/README.md
git commit -m "conformance: add randomness acceptance runner"
```

---

## Done criteria

1. `go test ./internal/conformance -run TestDiagnosticsCorpus -count=1` passes.
2. `go test ./internal/conformance -run TestConformance -count=1 -v` runs a non-empty enabled fixture set and reports skipped fixtures explicitly.
3. `go test ./internal/conformance -run TestBehavioralSuite -count=1 -v` skips cleanly when `lune` is absent and executes when it is present.
4. `go test ./internal/conformance -run TestRandomnessAcceptance -count=1 -v` skips cleanly without `ROTOR_RANDOMNESS_PATH` and produces actionable output when configured.
5. `testdata/conformance/README.md` documents the corpus split, required tools, and the exact commands above.
