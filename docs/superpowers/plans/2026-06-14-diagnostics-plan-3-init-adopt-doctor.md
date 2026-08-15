# Diagnostics Plan 3 — `init` adopt-existing + `doctor` ↔ `init` synergy

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `rotor init` adopt an existing project — when run where a project already exists but no `rotor.toml` does, write only the missing rotor config files (no clobber, template auto-detected) — and have `rotor doctor` report on `rotor.toml` and point users at `rotor init`.

**Architecture:** Replace `init`'s blanket "refuse existing project" guard with a three-way decision: greenfield scaffold (empty dir) / adopt mode (existing project, no rotor.toml) / already-configured (rotor.toml present → idempotent no-op). Adopt mode reuses the existing config-file producers and a no-clobber writer. `doctor` gains a `rotor.toml` check via `config.Load`.

**Tech Stack:** Go, `internal/config` (`Load`, `ErrNotFound`, `ConfigFileName`, `SchemaFileName`, `Schema`), `internal/compile` (env decl file), `cmd/rotor` (`init.go`, `doctor.go`). Independent of Plans 1–2 (may run in parallel).

**Spec:** `docs/superpowers/specs/2026-06-14-unified-code-frame-diagnostics-design.md` (§6, §7).

---

## File structure

- `cmd/rotor/init.go` — adopt-mode decision, `--config` flag, template detection, no-clobber writer, adopt next-steps.
- `cmd/rotor/init_test.go` — adopt-mode tests.
- `cmd/rotor/doctor.go` — `rotor.toml` check row.
- `cmd/rotor/doctor_test.go` — config-check tests.

---

## Task 1: template detection from an existing project

**Files:**

- Modify: `cmd/rotor/init.go`
- Test: `cmd/rotor/init_test.go`
- [ ] **Step 1: Write the failing test**

```go
func TestDetectTemplate(t *testing.T) {
	cases := []struct {
		name  string
		files map[string]string
		want  string
	}{
		{"plain", map[string]string{"default.project.json": `{"name":"x","tree":{"$path":"src"}}`}, "plain"},
		{"package", map[string]string{
			"tsconfig.json":        `{"compilerOptions":{"declaration":true}}`,
			"default.project.json": `{"name":"x","tree":{"$path":"out"}}`,
		}, "package"},
		{"game", map[string]string{
			"tsconfig.json":        `{"compilerOptions":{}}`,
			"default.project.json": `{"name":"x","tree":{"$className":"DataModel"}}`,
		}, "game"},
	}
	for _, c := range cases {
		dir := t.TempDir()
		for name, content := range c.files {
			if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
				t.Fatal(err)
			}
		}
		if got := detectTemplate(dir); got != c.want {
			t.Errorf("%s: detectTemplate = %q, want %q", c.name, got, c.want)
		}
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./cmd/rotor/ -run TestDetectTemplate -v`
Expected: FAIL — `undefined: detectTemplate`.

- [ ] **Step 3: Implement detection**

Add to `cmd/rotor/init.go`:

```go
// detectTemplate guesses which rotor.toml skeleton fits an existing project:
//   - "plain"   — a default.project.json but no tsconfig.json (Luau-only)
//   - "package" — tsconfig has "declaration": true (a model/package project)
//   - "game"    — otherwise (the common rbxts game)
// It only chooses the commented skeleton; it never reads/writes source files.
func detectTemplate(dir string) string {
	tsconfig := filepath.Join(dir, "tsconfig.json")
	hasTS := fileExists(tsconfig)
	if !hasTS && fileExists(filepath.Join(dir, "default.project.json")) {
		return "plain"
	}
	if hasTS {
		if data, err := os.ReadFile(tsconfig); err == nil && declarationEnabled(data) {
			return "package"
		}
	}
	return "game"
}

// declarationEnabled reports whether a tsconfig's compilerOptions.declaration is
// true. tsconfig.json is JSONC; a tolerant scan avoids a full JSONC parser for
// this one boolean (a commented-out "declaration" line stays false).
func declarationEnabled(data []byte) bool {
	for _, line := range strings.Split(string(data), "\n") {
		t := strings.TrimSpace(line)
		if strings.HasPrefix(t, "//") {
			continue
		}
		if strings.Contains(t, "\"declaration\"") && strings.Contains(t, "true") {
			return true
		}
	}
	return false
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./cmd/rotor/ -run TestDetectTemplate -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add cmd/rotor/init.go cmd/rotor/init_test.go
git commit -m "feat(init): detect rotor.toml template from an existing project"
```

---

## Task 2: adopt-mode files + no-clobber writer

**Files:**

- Modify: `cmd/rotor/init.go`
- Test: `cmd/rotor/init_test.go`
- [ ] **Step 1: Write the failing test**

```go
func TestAdoptFilesAndWrite(t *testing.T) {
	dir := t.TempDir()
	// existing project + a pre-existing env decl we must NOT clobber
	must(os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(`{"compilerOptions":{}}`), 0o644))
	must(os.WriteFile(filepath.Join(dir, compile.EnvDeclFileName), []byte("// mine\n"), 0o644))

	var out bytes.Buffer
	code := writeAdoptFiles(&out, dir, "game")
	if code != 0 {
		t.Fatalf("writeAdoptFiles = %d", code)
	}
	// rotor.toml + schema were created
	if !fileExists(filepath.Join(dir, config.ConfigFileName)) {
		t.Error("rotor.toml not created")
	}
	if !fileExists(filepath.Join(dir, config.SchemaFileName)) {
		t.Error("schema not created")
	}
	// env decl was kept verbatim
	got, _ := os.ReadFile(filepath.Join(dir, compile.EnvDeclFileName))
	if string(got) != "// mine\n" {
		t.Errorf("env decl clobbered: %q", got)
	}
	if !strings.Contains(out.String(), "exists, kept") {
		t.Errorf("expected a kept-file note:\n%s", out.String())
	}
}
```

(`must` is a one-line test helper: `func must(err error){ if err!=nil { panic(err) } }` — add if the test file lacks one.)

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./cmd/rotor/ -run TestAdoptFilesAndWrite -v`
Expected: FAIL — `undefined: writeAdoptFiles`.

- [ ] **Step 3: Implement adopt files + writer**

Add to `cmd/rotor/init.go`:

```go
// adoptFiles returns just the rotor config files for adopt mode: rotor.toml
// (commented skeleton for the detected template), the JSON schema, and the env
// type declaration. Source/project files are never part of adopt mode.
func adoptFiles() []initFile {
	files := []initFile{
		{config.ConfigFileName, rotorTOML(nil, nil)},
	}
	if configSchema != "" {
		files = append(files, initFile{config.SchemaFileName, configSchema})
	}
	files = append(files, initFile{compile.EnvDeclFileName, compile.EnvDeclFileText})
	return files
}

// writeAdoptFiles writes adoptFiles() into an existing project, never
// overwriting: a pre-existing target is reported as "(exists, kept)". template
// is the detected skeleton (currently only affects future rotor.toml content;
// the skeleton is shared today but the parameter keeps the call site honest).
func writeAdoptFiles(out io.Writer, dir, template string) int {
	u := newUI(out)
	u.banner("init  " + filepath.Base(mustAbs(dir)) + "  (adopt)")
	wrote := 0
	for _, f := range adoptFiles() {
		path := filepath.Join(dir, filepath.FromSlash(f.path))
		if fileExists(path) {
			fmt.Fprintf(out, "  %s %s %s\n", u.s.Muted(u.s.Glyphs().Dot), f.path, u.s.Muted("(exists, kept)"))
			continue
		}
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			newUI(os.Stderr).failLine(fmt.Sprintf("rotor init: %v", err))
			return 1
		}
		if err := os.WriteFile(path, []byte(f.content), 0o644); err != nil {
			newUI(os.Stderr).failLine(fmt.Sprintf("rotor init: cannot write %q: %v", path, err))
			return 1
		}
		fmt.Fprintf(out, "  %s %s\n", u.s.Green("+"), f.path)
		wrote++
	}
	printAdoptNextSteps(u, dir, wrote)
	return 0
}

func mustAbs(dir string) string {
	abs, err := filepath.Abs(dir)
	if err != nil {
		return dir
	}
	return abs
}

func printAdoptNextSteps(u *ui, dir string, wrote int) {
	fmt.Fprintln(u.w)
	if wrote == 0 {
		u.okLine("already had rotor config", "nothing to add")
	} else {
		u.okLine("added rotor config to existing project", plural(wrote, "file"))
	}
	fmt.Fprintln(u.w)
	fmt.Fprintf(u.w, "  %s\n", u.s.Bold("next steps"))
	fmt.Fprintf(u.w, "    %s %s\n", u.s.Muted(u.s.Glyphs().Arrow), u.s.Info("rotor doctor"))
}
```

Add imports to `init.go` if missing: `"io"` is already used by `writeInitFiles`; `compile`/`config` already imported.

- [ ] **Step 4: Run test to verify it passes**

Run: `go test ./cmd/rotor/ -run TestAdoptFilesAndWrite -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add cmd/rotor/init.go cmd/rotor/init_test.go
git commit -m "feat(init): adopt-mode config files + no-clobber writer"
```

---

## Task 3: route `cmdInit` into adopt mode

**Files:**

- Modify: `cmd/rotor/init.go` (`cmdInit` decision + `--config` flag)
- Test: `cmd/rotor/init_test.go`
- [ ] **Step 1: Write the failing tests**

```go
func TestInit_AdoptsExistingProjectInsteadOfRefusing(t *testing.T) {
	dir := t.TempDir()
	must(os.WriteFile(filepath.Join(dir, "package.json"), []byte(`{"name":"x"}`), 0o644))
	must(os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(`{"compilerOptions":{}}`), 0o644))

	code := cmdInit([]string{dir, "--yes"})
	if code != 0 {
		t.Fatalf("cmdInit = %d (expected adopt, not refuse)", code)
	}
	if !fileExists(filepath.Join(dir, config.ConfigFileName)) {
		t.Error("adopt mode did not create rotor.toml")
	}
	// existing files untouched
	pj, _ := os.ReadFile(filepath.Join(dir, "package.json"))
	if string(pj) != `{"name":"x"}` {
		t.Error("package.json was modified")
	}
}

func TestInit_AlreadyConfiguredIsIdempotent(t *testing.T) {
	dir := t.TempDir()
	must(os.WriteFile(filepath.Join(dir, "package.json"), []byte(`{"name":"x"}`), 0o644))
	must(os.WriteFile(filepath.Join(dir, config.ConfigFileName), []byte("# mine\n"), 0o644))

	code := cmdInit([]string{dir, "--yes"})
	if code != 0 {
		t.Fatalf("cmdInit = %d", code)
	}
	got, _ := os.ReadFile(filepath.Join(dir, config.ConfigFileName))
	if string(got) != "# mine\n" {
		t.Error("existing rotor.toml was overwritten")
	}
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `go test ./cmd/rotor/ -run 'TestInit_Adopts|TestInit_AlreadyConfigured' -v`
Expected: FAIL — current `cmdInit` refuses (exit 1) when `package.json` exists.

- [ ] **Step 3: Replace the refuse-guard with the three-way decision**

In `cmd/rotor/init.go` `cmdInit`, add a `--config` flag in the arg loop:

```go
		case a == "--config":
			configOnly = true
```

(declare `configOnly := false` near the other flag vars).

Replace the refuse block (init.go:89-97) with:

```go
	// Three-way: adopt an existing project (config-only), no-op if already
	// configured, else fall through to greenfield scaffolding.
	existing := false
	for _, marker := range []string{"package.json", "tsconfig.json", "default.project.json"} {
		if fileExists(filepath.Join(dir, marker)) {
			existing = true
			break
		}
	}
	if configOnly || existing {
		if fileExists(filepath.Join(dir, config.ConfigFileName)) {
			u := newUI(os.Stdout)
			u.banner("init  " + name)
			fmt.Fprintln(u.w)
			u.okLine("already configured", config.ConfigFileName+" exists")
			fmt.Fprintf(u.w, "    %s %s\n", u.s.Muted(u.s.Glyphs().Arrow), u.s.Info("rotor doctor"))
			return 0
		}
		return writeAdoptFiles(os.Stdout, dir, detectTemplate(dir))
	}
```

Keep the wizard gate and greenfield scaffold below unchanged. Document `--config` in the `-h` help block:

```go
		fmt.Println("  --config                            add only rotor config to an existing project")
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `go test ./cmd/rotor/ -run 'TestInit' -v`
Expected: PASS (including pre-existing greenfield init tests — verify they still scaffold in empty temp dirs).

- [ ] **Step 5: Commit**

```bash
git add cmd/rotor/init.go cmd/rotor/init_test.go
git commit -m "feat(init): adopt existing projects (config-only) + --config; idempotent"
```

---

## Task 4: `doctor` ↔ `init` synergy

**Files:**

- Modify: `cmd/rotor/doctor.go` (`runDoctor`)
- Test: `cmd/rotor/doctor_test.go`
- [ ] **Step 1: Write the failing tests**

```go
func TestDoctor_MissingRotorToml(t *testing.T) {
	dir := t.TempDir()
	must(os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(`{"compilerOptions":{}}`), 0o644))
	checks, _ := runDoctor(dir)
	if !hasCheck(checks, "rotor.toml", doctorWarn, "rotor init") {
		t.Errorf("expected a warn row suggesting rotor init:\n%+v", checks)
	}
}

func TestDoctor_ValidRotorToml(t *testing.T) {
	dir := t.TempDir()
	must(os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(`{"compilerOptions":{}}`), 0o644))
	must(os.WriteFile(filepath.Join(dir, config.ConfigFileName), []byte("# ok\n"), 0o644))
	checks, _ := runDoctor(dir)
	if !hasCheck(checks, "rotor.toml", doctorOK, "") {
		t.Errorf("expected an OK row for rotor.toml:\n%+v", checks)
	}
}

// hasCheck reports whether checks contains a row with the given label/status
// whose hint or detail contains substr (substr "" matches any).
func hasCheck(checks []doctorCheck, label string, status doctorStatus, substr string) bool {
	for _, c := range checks {
		if c.label == label && c.status == status &&
			(substr == "" || strings.Contains(c.hint, substr) || strings.Contains(c.detail, substr)) {
			return true
		}
	}
	return false
}
```

(Adapt `doctorWarn`/`doctorOK`/`doctorStatus`/`doctorCheck` field names to the actual definitions in `doctor.go`.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `go test ./cmd/rotor/ -run 'TestDoctor_.*RotorToml' -v`
Expected: FAIL — no `rotor.toml` row exists.

- [ ] **Step 3: Add the check**

In `cmd/rotor/doctor.go` `runDoctor`, after the tsconfig/node_modules checks (use the resolved project `dir`), append:

```go
	switch cfg, err := config.Load(dir); {
	case errors.Is(err, config.ErrNotFound):
		checks = append(checks, doctorCheck{
			status: doctorWarn,
			label:  "rotor.toml",
			detail: "not found",
			hint:   "run `rotor init` to add rotor config (needed for asset sync / deploy)",
		})
	case err != nil:
		checks = append(checks, doctorCheck{
			status: doctorFail,
			label:  "rotor.toml",
			detail: err.Error(),
		})
	default:
		_ = cfg
		checks = append(checks, doctorCheck{
			status: doctorOK,
			label:  "rotor.toml",
			detail: filepath.Join(dir, config.ConfigFileName),
		})
	}
```

Add imports to `doctor.go` if missing: `"errors"`, `"path/filepath"` (config already imported).

- [ ] **Step 4: Run tests to verify they pass**

Run: `go test ./cmd/rotor/ -run 'TestDoctor' -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add cmd/rotor/doctor.go cmd/rotor/doctor_test.go
git commit -m "feat(doctor): rotor.toml check; suggest rotor init when missing"
```

---

## Task 5: full regression + roadmap

- [ ] **Step 1: Whole suite**

Run: `go test ./...`
Expected: PASS. Validate via the golang Docker container per the project's CI note.

- [ ] **Step 2: Manual smoke**

```bash
mkdir -p /tmp/adopt && printf '{"compilerOptions":{}}' > /tmp/adopt/tsconfig.json
go run ./cmd/rotor init /tmp/adopt --yes   # should ADD rotor.toml, not refuse
go run ./cmd/rotor doctor /tmp/adopt       # should show an OK rotor.toml row
```

Expected: adopt writes `rotor.toml` + schema + env decl; doctor reports the config OK.

- [ ] **Step 3: Roadmap**

Tick the init-adopt / doctor items in `roadmap.md`; record Plan 3 complete.

```bash
git add roadmap.md
git commit -m "docs(roadmap): init adopt-existing + doctor synergy complete (plan 3)"
```

---

## Self-review notes (author)

- Spec §6 (init adopt) → Tasks 1–3; §7 (doctor synergy) → Task 4.
- Type/name consistency: `detectTemplate(dir) string`, `declarationEnabled([]byte) bool`, `adoptFiles() []initFile`, `writeAdoptFiles(out, dir, template) int`, `configOnly` flag, reuse of `rotorTOML(nil,nil)`/`configSchema`/`compile.EnvDeclFileName`/`compile.EnvDeclFileText` from existing `init.go`.
- No clobber: `writeAdoptFiles` checks `fileExists` before every write; idempotent path returns 0 without touching `rotor.toml`.
- Greenfield init is untouched (the wizard gate and `scaffold`/`writeInitFiles` still run for empty dirs); existing init tests must keep passing (Task 3 Step 4).
- "adapt to actual definitions" note in Task 4 depends on the unseen exact `doctorCheck`/`doctorStatus` field names in `doctor.go`.
