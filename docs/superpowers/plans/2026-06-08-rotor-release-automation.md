# rotor Release Automation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automate tag-driven GitHub Releases for rotor with cross-platform binaries, checksums, linker-injected versions, and README install instructions for GitHub-release-based tool managers.

**Architecture:** Use GitHub Actions to trigger on `v*` tags and invoke GoReleaser. Keep version injection inside `cmd/rotor`, keep packaging declarative in `.goreleaser.yml`, and keep user-facing install guidance in `README.md`.

**Tech Stack:** Go, Go tests, GoReleaser, GitHub Actions, Markdown docs.

---

## Task 1: Make the CLI release version linker-injectable

**Files:**

- Modify: `cmd/rotor/main.go`
- Create: `cmd/rotor/main_test.go`
- Test: `cmd/rotor/main_test.go`
- [ ] **Step 1: Write the failing version output test**

```go
func TestVersionCommandPrintsInjectedVersion(t *testing.T) {
	old := version
	version = "9.9.9-test"
	t.Cleanup(func() { version = old })

	output := captureStdout(t, func() {
		if code := run([]string{"--version"}); code != 0 {
			t.Fatalf("run exit = %d, want 0", code)
		}
	})

	if strings.TrimSpace(output) != "9.9.9-test" {
		t.Fatalf("version output = %q", output)
	}
}
```

- [ ] **Step 2: Run the test to verify it fails before the code change**

Run:

```powershell
go test ./cmd/rotor -run TestVersionCommandPrintsInjectedVersion -count=1
```

Expected: fail because `version` is currently a constant.

- [ ] **Step 3: Change `version` from a constant to a package variable**

```go
var version = "dev"
```

- [ ] **Step 4: Add the stdout-capture helper test and make it pass**

```go
func captureStdout(t *testing.T, fn func()) string {
	t.Helper()
	old := os.Stdout
	r, w, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	os.Stdout = w
	t.Cleanup(func() { os.Stdout = old })

	fn()

	_ = w.Close()
	data, err := io.ReadAll(r)
	if err != nil {
		t.Fatal(err)
	}
	return string(data)
}
```

- [ ] **Step 5: Run the CLI tests**

Run:

```powershell
go test ./cmd/rotor -count=1
```

Expected: `PASS`

### Task 2: Add GoReleaser configuration

**Files:**

- Create: `.goreleaser.yml`

- [ ] **Step 1: Add the release build matrix**

```yaml
builds:
  - id: rotor
    main: ./cmd/rotor
    binary: rotor
    env:
      - CGO_ENABLED=0
    goos: [windows, linux, darwin]
    goarch: [amd64, arm64]
    ldflags:
      - -s -w -X main.version={{ .Version }}
```

- [ ] **Step 2: Add archive naming and checksum output**

```yaml
archives:
  - id: rotor
    builds: [rotor]
    name_template: "rotor-v{{ .Version }}-{{ .Os }}-{{ .Arch }}"
    files:
      - README.md
      - LICENSE
    format_overrides:
      - goos: windows
        format: zip

checksum:
  name_template: "rotor-v{{ .Version }}-checksums.txt"
```

- [ ] **Step 3: Add release metadata**

```yaml
release:
  draft: false
  prerelease: auto
```

### Task 3: Add the tag-triggered GitHub Actions workflow

**Files:**

- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Add the tag trigger and checkout/setup steps**

```yaml
on:
  push:
    tags:
      - "v*"
```

- [ ] **Step 2: Run tests before publishing**

```yaml
- name: Run CLI tests
  run: go test ./cmd/rotor -count=1
```

- [ ] **Step 3: Invoke GoReleaser**

```yaml
- name: Run GoReleaser
  uses: goreleaser/goreleaser-action@v6
  with:
    distribution: goreleaser
    version: "~> v2"
    args: release --clean
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### Task 4: Update the README release and install docs

**Files:**

- Modify: `README.md`

- [ ] **Step 1: Replace the local-build-first installation section with release-first guidance**

```md
### Install

Download a release archive from GitHub Releases, or install rotor with a
GitHub-release-aware tool manager.
```

- [ ] **Step 2: Add tool-manager snippets**

```md
mise use -g github:uproot/rotor@v1.0.1
```

```toml
[tools]
rotor = "uproot/rotor@1.0.1"
```

```bash
rokit add uproot/rotor@1.0.1
```

- [ ] **Step 3: Add release instructions for maintainers**

```md
1. Update anything user-visible that should mention the new version.
2. Push a tag like `v1.0.1`.
3. Wait for the `release` GitHub Actions workflow to publish the archives and checksums.
```

### Task 5: Verify the release flow locally

**Files:**

- Test: `cmd/rotor/main_test.go`

- [ ] **Step 1: Run CLI tests**

Run:

```powershell
go test ./cmd/rotor -count=1
```

Expected: `PASS`

- [ ] **Step 2: Verify linker version injection with a local build**

Run:

```powershell
go build -ldflags "-X main.version=9.9.9" -o .\\tmp\\rotor-version-test.exe .\\cmd\\rotor
.\tmp\rotor-version-test.exe --version
```

Expected:

```text
9.9.9
```

- [ ] **Step 3: Smoke-check repo tests that cover touched code**

Run:

```powershell
go test ./cmd/rotor -count=1
```

Expected: `PASS`
