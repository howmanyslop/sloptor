# Transformer Plugin Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring rotor's transformer-plugin support up to the design spec ("bundled Node helper", "plugin projects keep the JS program warm in watch mode", "Flamework-class plugins work unmodified") and prove it against the real `rbxts-transformer-flamework` and `rbxts-transform-env` packages.

**Architecture:** The Node sidecar JS stays in `tools/sidecar/` but becomes embedded in the rotor binary via `go:embed` and extracted to a content-addressed cache dir at runtime, so released binaries work outside this repo. The sidecar resolves `typescript` from the *project's* `node_modules` (the same instance plugins `require`), protects the stdout JSON protocol from plugin `console.log` chatter, and the Go side keeps one warm sidecar process per project across builds (sending `changedFiles` overlays computed from file stamps), which makes watch rebuilds reuse the JS program exactly like upstream's `data.transformerWatcher ??=` does.

**Tech Stack:** Go (`internal/compile`), Node CommonJS (`tools/sidecar`), `go:embed`, bun-installed npm fixtures, `node --test` for JS tests.

**Current state (verified 2026-06-10):**

- `internal/compile/sidecar.go` spawns `node tools/sidecar/main.js` per compile pass; paths baked via `runtime.Caller(0)` (`repoFile`), plus a `testdata/diff/project/node_modules` NODE_PATH hack. Broken for released binaries.
- `tools/sidecar/index.js` and `lib/diagnostics.js` `require("typescript")` at module load → resolves the sidecar's own pinned 5.5.3, a *different instance* than plugins get from the project. Upstream guarantees one shared instance.
- Plugin `console.log` output goes to stdout → corrupts the line-JSON protocol. Flamework logs with chalk.
- Watch mode respawns the sidecar each rebuild (roadmap "Post-v1 Follow-up").
- Tests only cover a synthetic prefix-string plugin. No real-package coverage. Note `@rbxts/transform-env` does not exist on npm — the real package is `rbxts-transform-env@3.0.0`; Flamework's transformer is `rbxts-transformer-flamework@1.3.2` (`@flamework/core@1.3.2`).

---

## Task 1: Sidecar JS — per-project TypeScript resolution

The sidecar must use the same `typescript` module instance that plugins resolve from the project. Remove all top-level `require("typescript")` (they would also crash an extracted sidecar that has no `node_modules`).

**Files:**

- Modify: `tools/sidecar/index.js`
- Modify: `tools/sidecar/lib/diagnostics.js`
- Modify: `tools/sidecar/lib/session.js`
- Test: `tools/sidecar/test/sidecar.test.js`
- [x] **Step 1: Write failing tests for `resolveTypeScript`**

Append to `tools/sidecar/test/sidecar.test.js` (it already requires `node:test`, `node:assert`, `node:path`, `node:fs`, `node:os` — add any missing requires at the top):

```js
test("resolveTypeScript prefers the project's typescript copy", () => {
  const projectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-ts-"));
  const stubDir = path.join(projectDir, "node_modules", "typescript");
  fs.mkdirSync(stubDir, { recursive: true });
  fs.writeFileSync(path.join(stubDir, "package.json"), JSON.stringify({ name: "typescript", version: "0.0.0-stub", main: "index.js" }));
  fs.writeFileSync(path.join(stubDir, "index.js"), "module.exports = { __rotorStub: true };\n");

  const { resolveTypeScript } = require("../index.js");
  const resolved = resolveTypeScript(projectDir);
  assert.strictEqual(resolved.__rotorStub, true);
});

test("resolveTypeScript falls back to the sidecar's own typescript", () => {
  const projectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-nots-"));
  const { resolveTypeScript } = require("../index.js");
  const resolved = resolveTypeScript(projectDir);
  assert.strictEqual(typeof resolved.transformNodes, "function");
});
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cd tools/sidecar && node --test test/sidecar.test.js`
Expected: FAIL — `resolveTypeScript` is not exported.

- [x] **Step 3: Implement per-project resolution**

`tools/sidecar/lib/diagnostics.js` — delete line 1 (`const ts = require("typescript");`) and thread `ts` through the two functions that need it:

```js
function categoryToString(ts, category) {
  switch (category) {
    case ts.DiagnosticCategory.Warning:
    case ts.DiagnosticCategory.Message:
    case ts.DiagnosticCategory.Suggestion:
      return "warning";
    case ts.DiagnosticCategory.Error:
    default:
      return "error";
  }
}

function toProtocolDiagnostic(ts, diagnostic) {
  return {
    category: categoryToString(ts, diagnostic.category),
    code: String(diagnostic.code),
    file: diagnostic.file ? diagnostic.file.fileName : undefined,
    start: diagnostic.start,
    length: diagnostic.length,
    message: ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
  };
}
```

`tools/sidecar/lib/session.js` — update the call sites (sessions hold `this.ts`):

- `parsed.errors.map(toProtocolDiagnostic)` → `parsed.errors.map((diagnostic) => toProtocolDiagnostic(this.ts, diagnostic))`
- `(result.diagnostics ?? []).map(toProtocolDiagnostic)` → `(result.diagnostics ?? []).map((diagnostic) => toProtocolDiagnostic(this.ts, diagnostic))`

`tools/sidecar/index.js` — remove `const ts = require("typescript");`, add and export:

```js
function resolveTypeScript(projectDir) {
  const paths = [];
  if (typeof projectDir === "string" && projectDir.length > 0) {
    paths.push(projectDir);
  }
  paths.push(__dirname);
  return require(require.resolve("typescript", { paths }));
}
```

`SidecarServer` (in `lib/session.js`) currently takes the `ts` module. Make it accept either a module or a loader, resolving per session, and surface load failures as a protocol diagnostic:

```js
class SidecarServer {
  constructor(tsOrLoader) {
    this.loadTypeScript = typeof tsOrLoader === "function" ? tsOrLoader : () => tsOrLoader;
    this.session = undefined;
    this.sessionKey = "";
  }

  handleRequest(request) {
    const validationError = validateRequest(request);
    if (validationError) {
      return { diagnostics: [validationError], transformed: [] };
    }

    const sessionKey = `${normalizePath(request.projectDir)}\u0000${normalizePath(request.tsConfigPath)}`;
    if (!this.session || this.sessionKey !== sessionKey) {
      let ts;
      try {
        ts = this.loadTypeScript(request.projectDir);
      } catch (error) {
        return {
          diagnostics: [
            createProtocolDiagnostic(
              "error",
              "typescript-not-found",
              `Could not resolve the \`typescript\` package from ${request.projectDir}.\n` +
                `Transformer plugins require typescript in the project's node_modules (roblox-ts projects pin ~5.5.3).\n` +
                `More info: ${error instanceof Error ? error.message : String(error)}`,
            ),
          ],
          transformed: [],
        };
      }
      this.session = new SidecarProjectSession(ts, request.projectDir, request.tsConfigPath);
      this.sessionKey = sessionKey;
    }

    return this.session.handleRequest(request);
  }
}
```

`serveStdio` in `index.js`: replace `new SidecarServer(options.ts ?? ts)` with `new SidecarServer(options.ts ?? resolveTypeScript)`. Export `resolveTypeScript` from `module.exports`. `transformSourceFiles` already takes `tsApi` as a parameter — unchanged.

- [x] **Step 4: Run the sidecar test suite**

Run: `cd tools/sidecar && node --test test/*.test.js`
Expected: PASS (including the pre-existing tests that pass a `ts` module directly to `SidecarServer`).

- [x] **Step 5: Commit**

```bash
git add tools/sidecar
git commit -m "feat(sidecar): resolve typescript from the project, matching the instance plugins require"
```

---

### Task 2: Sidecar JS — protect the stdout protocol from plugin chatter

Flamework (and any plugin using `console.log`) writes to stdout, which corrupts the line-JSON protocol. Capture the real stdout writer for protocol responses and redirect everything else to stderr.

**Files:**

- Modify: `tools/sidecar/main.js`
- Test: `tools/sidecar/test/sidecar.test.js`
- [x] **Step 1: Write the failing test**

This test must spawn the real `main.js` (the redirect lives there, not in `serveStdio`). Append to `tools/sidecar/test/sidecar.test.js` (add `const { spawnSync } = require("node:child_process");` to the requires):

```js
test("main.js keeps plugin console.log off the protocol stream", () => {
  const projectDir = fs.mkdtempSync(path.join(os.tmpdir(), "rotor-sidecar-stdout-"));
  fs.mkdirSync(path.join(projectDir, "src"), { recursive: true });
  fs.writeFileSync(
    path.join(projectDir, "noisy-plugin.js"),
    `module.exports = function (program, config) {
      console.log("plugin chatter on stdout");
      return (context) => (sourceFile) => sourceFile;
    };\n`,
  );
  fs.writeFileSync(
    path.join(projectDir, "tsconfig.json"),
    JSON.stringify({
      compilerOptions: {
        module: "CommonJS",
        moduleResolution: "Node",
        noLib: true,
        moduleDetection: "force",
        target: "ESNext",
        types: [],
        rootDir: "src",
        outDir: "out",
        plugins: [{ transform: "./noisy-plugin.js" }],
      },
      include: ["src"],
    }),
  );
  const mainFile = path.join(projectDir, "src", "main.ts");
  fs.writeFileSync(mainFile, "export const phase = \"start\";\n");

  const request = JSON.stringify({
    protocol: 1,
    tsConfigPath: path.join(projectDir, "tsconfig.json"),
    projectDir,
    compileFileNames: [mainFile],
    changedFiles: [],
  });

  const result = spawnSync(process.execPath, [path.join(__dirname, "..", "main.js")], {
    input: `${request}\n`,
    encoding: "utf8",
    cwd: projectDir,
  });

  assert.strictEqual(result.status, 0, result.stderr);
  const lines = result.stdout.split("\n").filter((line) => line.trim().length > 0);
  assert.strictEqual(lines.length, 1, `stdout must carry exactly one JSON response, got:\n${result.stdout}`);
  const response = JSON.parse(lines[0]);
  assert.deepStrictEqual(response.diagnostics, []);
  assert.match(result.stderr, /plugin chatter on stdout/);
});
```

Note: the noisy plugin requires nothing, so this works without `typescript` in the temp project (`resolveTypeScript` falls back to `__dirname`).

- [x] **Step 2: Run test to verify it fails**

Run: `cd tools/sidecar && node --test test/sidecar.test.js`
Expected: FAIL — two stdout lines (the chatter and the JSON response), or a JSON parse error.

- [x] **Step 3: Implement the redirect in `main.js`**

```js
#!/usr/bin/env node

// The line-JSON protocol owns stdout. Plugins are arbitrary user code that
// console.log()s freely (Flamework does), so reserve the real stdout writer
// for protocol responses and reroute every other stdout write to stderr.
const protocolWrite = process.stdout.write.bind(process.stdout);
process.stdout.write = (chunk, encoding, callback) => process.stderr.write(chunk, encoding, callback);

const { serveStdio } = require("./index.js");

serveStdio({ output: { write: (text) => protocolWrite(text) } });
```

- [x] **Step 4: Run the sidecar test suite**

Run: `cd tools/sidecar && node --test test/*.test.js`
Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add tools/sidecar
git commit -m "fix(sidecar): keep plugin console.log output off the stdout protocol stream"
```

---

### Task 3: Go — embed the sidecar; stop depending on the repo checkout

`go:embed` patterns must live in the embedded files' own directory, so the embed declaration goes in a new `tools/sidecar/embed.go` (package `sidecar`, import path `rotor/tools/sidecar`). `internal/compile` extracts it to a content-addressed cache dir unless `ROTOR_SIDECAR_PATH` points elsewhere. Delete `repoFile`/`runtime.Caller` and the `testdata/diff/project/node_modules` NODE_PATH entry from production code.

**Files:**

- Create: `tools/sidecar/embed.go`
- Create: `internal/compile/sidecar_install.go`
- Create: `internal/compile/sidecar_install_test.go`
- Modify: `internal/compile/sidecar.go`
- Modify: `internal/compile/sidecar_test.go`
- [x] **Step 1: Create the embed package**

`tools/sidecar/embed.go`:

```go
// Package sidecar embeds the Node transformer-plugin worker so released
// rotor binaries can run transformer plugins without a checkout of this
// repository. node_modules is intentionally not embedded: the worker
// resolves `typescript` from the target project at runtime.
package sidecar

import "embed"

//go:embed main.js index.js lib/*.js package.json
var FS embed.FS
```

- [x] **Step 2: Write the failing extraction test**

`internal/compile/sidecar_install_test.go`:

```go
package compile

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

// redirectUserCacheDir points os.UserCacheDir at a temp dir for the test.
func redirectUserCacheDir(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	if runtime.GOOS == "windows" {
		t.Setenv("LocalAppData", dir)
	} else if runtime.GOOS == "darwin" {
		t.Setenv("HOME", dir)
	} else {
		t.Setenv("XDG_CACHE_HOME", dir)
	}
	return dir
}

func TestResolveSidecarDirExtractsEmbeddedWorker(t *testing.T) {
	t.Setenv("ROTOR_SIDECAR_PATH", "")
	redirectUserCacheDir(t)

	dir, err := resolveSidecarDir()
	if err != nil {
		t.Fatalf("resolveSidecarDir: %v", err)
	}
	for _, name := range []string{"main.js", "index.js", filepath.Join("lib", "session.js"), filepath.Join("lib", "plugins.js"), filepath.Join("lib", "diagnostics.js"), "package.json"} {
		if _, err := os.Stat(filepath.Join(dir, name)); err != nil {
			t.Fatalf("extracted sidecar missing %s: %v", name, err)
		}
	}

	// Idempotent: a second call returns the same completed dir.
	again, err := resolveSidecarDir()
	if err != nil {
		t.Fatalf("second resolveSidecarDir: %v", err)
	}
	if again != dir {
		t.Fatalf("resolveSidecarDir not stable: %q != %q", again, dir)
	}
}

func TestResolveSidecarDirHonorsOverride(t *testing.T) {
	override := repoSidecarDir(t)
	t.Setenv("ROTOR_SIDECAR_PATH", override)
	dir, err := resolveSidecarDir()
	if err != nil {
		t.Fatalf("resolveSidecarDir: %v", err)
	}
	if dir != override {
		t.Fatalf("dir = %q, want override %q", dir, override)
	}
}
```

- [x] **Step 3: Run test to verify it fails**

Run: `go test ./internal/compile -run TestResolveSidecarDir -count=1`
Expected: FAIL — `resolveSidecarDir`/`repoSidecarDir` undefined.

- [x] **Step 4: Implement extraction**

`internal/compile/sidecar_install.go`:

```go
package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"

	sidecarfs "rotor/tools/sidecar"
)

// resolveSidecarDir locates the Node transformer worker. ROTOR_SIDECAR_PATH
// overrides (repo-dev and tests); otherwise the embedded worker is extracted
// once into a content-addressed cache dir so released binaries work without
// a repo checkout.
func resolveSidecarDir() (string, error) {
	if dir := os.Getenv("ROTOR_SIDECAR_PATH"); dir != "" {
		return dir, nil
	}

	names, hash, err := embeddedSidecarManifest()
	if err != nil {
		return "", err
	}
	cacheRoot, err := os.UserCacheDir()
	if err != nil {
		return "", fmt.Errorf("compile: cannot locate user cache dir for sidecar extraction: %w", err)
	}
	dir := filepath.Join(cacheRoot, "rotor", "sidecar-"+hash)
	marker := filepath.Join(dir, ".complete")
	if _, err := os.Stat(marker); err == nil {
		return dir, nil
	}

	tmp := dir + fmt.Sprintf(".tmp-%d", os.Getpid())
	if err := os.RemoveAll(tmp); err != nil {
		return "", err
	}
	for _, name := range names {
		data, err := sidecarfs.FS.ReadFile(name)
		if err != nil {
			return "", err
		}
		target := filepath.Join(tmp, filepath.FromSlash(name))
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return "", err
		}
		if err := os.WriteFile(target, data, 0o644); err != nil {
			return "", err
		}
	}
	if err := os.WriteFile(filepath.Join(tmp, ".complete"), nil, 0o644); err != nil {
		return "", err
	}
	if err := os.Rename(tmp, dir); err != nil {
		// Lost a race with a concurrent extraction; accept the winner.
		if _, statErr := os.Stat(marker); statErr == nil {
			_ = os.RemoveAll(tmp)
			return dir, nil
		}
		return "", err
	}
	return dir, nil
}

func embeddedSidecarManifest() ([]string, string, error) {
	var names []string
	err := fs.WalkDir(sidecarfs.FS, ".", func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		names = append(names, path)
		return nil
	})
	if err != nil {
		return nil, "", err
	}
	sort.Strings(names)

	hasher := sha256.New()
	for _, name := range names {
		data, err := sidecarfs.FS.ReadFile(name)
		if err != nil {
			return nil, "", err
		}
		hasher.Write([]byte(name))
		hasher.Write([]byte{0})
		hasher.Write(data)
		hasher.Write([]byte{0})
	}
	return names, hex.EncodeToString(hasher.Sum(nil))[:16], nil
}
```

In `internal/compile/sidecar.go`:

- Delete the `var sidecarMainPath/sidecarNodePath` block and the `repoFile` function.
- `runTransformerSidecar` resolves the dir per call: `sidecarDir, err := resolveSidecarDir()` (propagate error), spawns `filepath.Join(sidecarDir, "main.js")`.
- `sidecarEnv(projectDir)` becomes `sidecarEnv(projectDir, sidecarDir string)` with `nodePaths := []string{filepath.Join(filepath.FromSlash(projectDir), "node_modules"), filepath.Join(sidecarDir, "node_modules")}` — the testdata entry is gone. (The sidecar-dir entry keeps repo-dev working: synthetic test plugins `require("typescript")` and find `tools/sidecar/node_modules` through NODE_PATH.)

In `internal/compile/sidecar_test.go`, add the helper (production code no longer knows the repo layout; tests still may):

```go
// repoSidecarDir returns tools/sidecar in this repo checkout. Synthetic
// plugin fixtures have no typescript of their own, so tests point
// ROTOR_SIDECAR_PATH here and the worker falls back to the sidecar's
// pinned typescript install.
func repoSidecarDir(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	dir := filepath.Join(filepath.Dir(file), "..", "..", "tools", "sidecar")
	if _, err := os.Stat(filepath.Join(dir, "main.js")); err != nil {
		t.Fatalf("repo sidecar missing: %v", err)
	}
	return filepath.Clean(dir)
}

func setRepoSidecarPath(t *testing.T) {
	t.Helper()
	t.Setenv("ROTOR_SIDECAR_PATH", repoSidecarDir(t))
}
```

Add `setRepoSidecarPath(t)` at the top of `TestBuildProjectTransformerPluginSidecar` and `TestBuildProjectMissingTransformerWarnsAndContinues` (the two that actually run the worker; `TestBuildProjectTransformerPluginRequiresNode` fails before spawning but add it there too for consistency).

- [x] **Step 5: Run the compile tests**

Run: `go test ./internal/compile -run 'TestResolveSidecarDir|TestBuildProject' -count=1`
Expected: PASS.

- [x] **Step 6: Verify the whole module still builds and vets**

Run: `go build ./... && go vet ./internal/... ./cmd/... ./tools/...`
Expected: clean.

- [x] **Step 7: Commit**

```bash
git add tools/sidecar/embed.go internal/compile/sidecar_install.go internal/compile/sidecar_install_test.go internal/compile/sidecar.go internal/compile/sidecar_test.go
git commit -m "feat: embed the transformer sidecar so released binaries run plugins without the repo"
```

---

### Task 4: Go — warm sidecar sessions with changedFiles overlays

Keep one Node worker alive per `(projectDir, tsConfigPath)` for the life of the rotor process. Subsequent requests reuse the warm JS program; file edits are communicated via the protocol's `changedFiles` (stamp-diff over the program's project source files), matching upstream's persistent `transformerWatcher`. Watch mode gets warmth for free because the registry outlives each `runBuildOnce` call.

**Files:**

- Modify: `internal/compile/sidecar.go`
- Test: `internal/compile/sidecar_test.go`
- [x] **Step 1: Write the failing warm-session test**

The plugin below proves process reuse (a module-level counter survives only if the worker process and its require cache survive) and overlay correctness (the second build's output must reflect the edited source, which a stale LanguageService snapshot would miss). Append to `internal/compile/sidecar_test.go`:

```go
const countingPlugin = `let buildCount = 0;

module.exports = function (program, config, helpers) {
	const ts = helpers.ts;
	buildCount += 1;
	return (context) => (sourceFile) => {
		const visit = (node) => {
			if (ts.isStringLiteral(node) && node.text === "BUILD_COUNT") {
				return ts.factory.createStringLiteral("build:" + buildCount);
			}
			return ts.visitEachChild(node, visit, context);
		};
		return ts.visitNode(sourceFile, visit);
	};
};
`

func TestBuildProjectTransformerSidecarStaysWarmAcrossBuilds(t *testing.T) {
	setRepoSidecarPath(t)
	t.Cleanup(closeSidecarSessions)
	closeSidecarSessions()

	dir := writeProject(t, "@scope/plugin-warm-fixture", "")
	if err := os.MkdirAll(filepath.Join(dir, "plugins"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "plugins", "counting.js"), []byte(countingPlugin), 0o644); err != nil {
		t.Fatal(err)
	}
	tsconfig := `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out",
		"plugins": [{ "transform": "./plugins/counting.js" }]
	},
	"include": ["src"]
}`
	if err := os.WriteFile(filepath.Join(dir, "tsconfig.json"), []byte(tsconfig), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const tag = \"BUILD_COUNT\";\nexport const phase = \"first\";\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil {
		t.Fatalf("first build: %v (diags: %v)", err, diags)
	}
	got := result.Outputs["out/main.luau"]
	if !strings.Contains(got, `local tag = "build:1"`) || !strings.Contains(got, `local phase = "first"`) {
		t.Fatalf("first build output unexpected:\n%s", got)
	}

	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const tag = \"BUILD_COUNT\";\nexport const phase = \"second\";\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	result, diags, err = BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil {
		t.Fatalf("second build: %v (diags: %v)", err, diags)
	}
	got = result.Outputs["out/main.luau"]
	if !strings.Contains(got, `local tag = "build:2"`) {
		t.Fatalf("second build did not reuse a warm sidecar process:\n%s", got)
	}
	if !strings.Contains(got, `local phase = "second"`) {
		t.Fatalf("warm sidecar served a stale snapshot (changedFiles overlay broken):\n%s", got)
	}
}
```

Also add `t.Cleanup(closeSidecarSessions)` plus a leading `closeSidecarSessions()` to the existing `TestBuildProjectTransformerPluginSidecar` and `TestBuildProjectMissingTransformerWarnsAndContinues` so tests don't share sessions.

- [x] **Step 2: Run test to verify it fails**

Run: `go test ./internal/compile -run TestBuildProjectTransformerSidecarStaysWarm -count=1`
Expected: FAIL — `closeSidecarSessions` undefined (compile error). After implementing only the registry stub, the assertion failure would be `build:1` appearing twice (cold respawn).

- [x] **Step 3: Implement warm sessions in `sidecar.go`**

Replace the per-call spawn with a session registry. Shape:

```go
type sidecarFileStamp struct {
	modTime time.Time
	size    int64
}

type sidecarSession struct {
	cmd    *exec.Cmd
	stdin  io.WriteCloser
	stdout *bufio.Reader
	stderr *sidecarStderrTail
	stamps map[string]sidecarFileStamp
	dead   bool
}

var (
	sidecarMu       sync.Mutex
	sidecarSessions = map[string]*sidecarSession{}
)
```

`sidecarStderrTail` is a goroutine-fed tail buffer that also forwards plugin chatter (now rerouted to stderr by Task 2) to the user:

```go
// sidecarStderrTail streams the worker's stderr to the compiler log (plugins
// print progress there, e.g. Flamework) while keeping a tail for error
// reporting.
type sidecarStderrTail struct {
	mu   sync.Mutex
	tail []string
}

func newSidecarStderrTail(pipe io.Reader) *sidecarStderrTail {
	t := &sidecarStderrTail{}
	go func() {
		scanner := bufio.NewScanner(pipe)
		scanner.Buffer(make([]byte, 64*1024), 1024*1024)
		for scanner.Scan() {
			line := scanner.Text()
			t.mu.Lock()
			t.tail = append(t.tail, line)
			if len(t.tail) > 50 {
				t.tail = t.tail[len(t.tail)-50:]
			}
			t.mu.Unlock()
			logservice.WriteLine(line)
		}
	}()
	return t
}

func (t *sidecarStderrTail) String() string {
	t.mu.Lock()
	defer t.mu.Unlock()
	return strings.Join(t.tail, "\n")
}
```

Spawning (factored out of the old `runTransformerSidecar`):

```go
func spawnSidecarSession(dir, sidecarDir string) (*sidecarSession, error) {
	nodeCommand := os.Getenv("ROTOR_NODE_PATH")
	if nodeCommand != "" {
		if _, err := os.Stat(nodeCommand); err != nil {
			return nil, errors.New("node executable not found; rotor transformer plugins require Node.js on PATH")
		}
	} else {
		nodeCommand = "node"
	}
	nodePath, err := exec.LookPath(nodeCommand)
	if err != nil {
		return nil, errors.New("node executable not found; rotor transformer plugins require Node.js on PATH")
	}

	cmd := exec.Command(nodePath, filepath.Join(sidecarDir, "main.js"))
	cmd.Dir = filepath.FromSlash(dir)
	cmd.Env = sidecarEnv(dir, sidecarDir)

	stderrPipe, err := cmd.StderrPipe()
	if err != nil {
		return nil, err
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, err
	}
	if err := cmd.Start(); err != nil {
		return nil, err
	}
	return &sidecarSession{
		cmd:    cmd,
		stdin:  stdin,
		stdout: bufio.NewReader(stdout),
		stderr: newSidecarStderrTail(stderrPipe),
		stamps: map[string]sidecarFileStamp{},
	}, nil
}
```

Round trip and changed-file detection:

```go
func (s *sidecarSession) roundTrip(request sidecarRequest) (*sidecarResponse, error) {
	payload, err := json.Marshal(request)
	if err != nil {
		return nil, err
	}
	if _, err := s.stdin.Write(append(payload, '\n')); err != nil {
		return nil, s.fail(err)
	}
	line, err := s.stdout.ReadBytes('\n')
	if err != nil {
		return nil, s.fail(err)
	}
	var response sidecarResponse
	if err := json.Unmarshal(bytes.TrimSpace(line), &response); err != nil {
		return nil, s.fail(err)
	}
	return &response, nil
}

func (s *sidecarSession) fail(err error) error {
	s.dead = true
	if tail := s.stderr.String(); tail != "" {
		return fmt.Errorf("transformer sidecar failed: %s", strings.TrimSpace(tail))
	}
	return err
}

func (s *sidecarSession) close() {
	_ = s.stdin.Close()
	if s.cmd.Process != nil {
		done := make(chan struct{})
		go func() { _ = s.cmd.Wait(); close(done) }()
		select {
		case <-done:
		case <-time.After(2 * time.Second):
			_ = s.cmd.Process.Kill()
			<-done
		}
	}
}

// changedFilesFor stat-diffs the program's project files against the
// session's last-seen stamps. Fresh sessions only record stamps (the worker
// reads from disk); warm sessions ship new text so the LanguageService
// snapshot versions advance (upstream updateFile semantics).
func (s *sidecarSession) changedFilesFor(fileNames []string) ([]sidecarChangedFile, error) {
	fresh := len(s.stamps) == 0
	changed := []sidecarChangedFile{}
	for _, fileName := range fileNames {
		path := filepath.FromSlash(fileName)
		info, err := os.Stat(path)
		if err != nil {
			continue
		}
		stamp := sidecarFileStamp{modTime: info.ModTime(), size: info.Size()}
		if prev, ok := s.stamps[path]; !fresh && (!ok || prev != stamp) {
			text, err := os.ReadFile(path)
			if err != nil {
				return nil, err
			}
			changed = append(changed, sidecarChangedFile{FileName: path, Text: string(text)})
		}
		s.stamps[path] = stamp
	}
	return changed, nil
}
```

`runTransformerSidecar` becomes acquire → request → retry-once-on-death. It needs the full project file list for stamping, so change `applyTransformerSidecar` to pass it (`projectSourceFiles(program)` — the same set `remapProgramSourceFiles` uses):

```go
func runTransformerSidecar(dir, configPath string, compileFiles, stampFiles []*ast.SourceFile) (*sidecarResponse, error) {
	sidecarDir, err := resolveSidecarDir()
	if err != nil {
		return nil, err
	}

	key := normalizeSourceFilePath(dir) + "\x00" + normalizeSourceFilePath(configPath)
	sidecarMu.Lock()
	defer sidecarMu.Unlock()

	for attempt := 0; ; attempt++ {
		session := sidecarSessions[key]
		if session == nil || session.dead {
			if session != nil {
				session.close()
			}
			session, err = spawnSidecarSession(dir, sidecarDir)
			if err != nil {
				return nil, err
			}
			sidecarSessions[key] = session
		}

		stampNames := make([]string, 0, len(stampFiles))
		for _, sourceFile := range stampFiles {
			stampNames = append(stampNames, sourceFile.FileName())
		}
		changedFiles, err := session.changedFilesFor(stampNames)
		if err != nil {
			return nil, err
		}

		request := sidecarRequest{
			Protocol:         1,
			TsConfigPath:     filepath.FromSlash(configPath),
			ProjectDir:       filepath.FromSlash(dir),
			CompileFileNames: make([]string, 0, len(compileFiles)),
			ChangedFiles:     changedFiles,
		}
		for _, sourceFile := range compileFiles {
			request.CompileFileNames = append(request.CompileFileNames, filepath.FromSlash(sourceFile.FileName()))
		}

		response, err := session.roundTrip(request)
		if err != nil {
			delete(sidecarSessions, key)
			session.close()
			if attempt == 0 {
				continue
			}
			return nil, err
		}
		return response, nil
	}
}

func closeSidecarSessions() {
	sidecarMu.Lock()
	defer sidecarMu.Unlock()
	for key, session := range sidecarSessions {
		session.close()
		delete(sidecarSessions, key)
	}
}
```

`applyTransformerSidecar` call site: `runTransformerSidecar(dir, configPath, sourceFiles, projectSourceFiles(program))`.

Notes:

- The old single-shot code path (write → close stdin → wait) is replaced entirely; the worker now exits when rotor's process exits and the pipes close.
- Error reporting keeps surfacing the stderr tail (`session.fail`), preserving the existing `transformer sidecar failed: ...` message shape.
- Stamps cover `.ts`/`.tsx` project files only (same scope `projectSourceFiles` gives the rest of the pipeline). A warm worker can hold a stale view of an edited ambient `.d.ts`; document this in Task 6 — it only affects what *plugins* see mid-watch, never rotor's own typecheck.
- [x] **Step 4: Run the sidecar tests**

Run: `go test ./internal/compile -run 'TestBuildProject|TestResolveSidecarDir' -count=1`
Expected: PASS, including the new warm test.

- [x] **Step 5: Run the full Go suite (sidecar touches the shared compile path)**

Run: `go test ./... -count=1`
Expected: PASS (conformance suites skip where tools are absent, as usual).

- [x] **Step 6: Commit**

```bash
git add internal/compile/sidecar.go internal/compile/sidecar_test.go
git commit -m "feat: keep one warm transformer sidecar per project across builds and watch rebuilds"
```

---

### Task 5: Real-package fixture — Flamework + rbxts-transform-env integration tests

A real fixture project under `testdata/transformers/project` with `rbxts-transformer-flamework@1.3.2`, `@flamework/core@1.3.2`, and `rbxts-transform-env@3.0.0`, installed by bun (same pattern as the diff fixture), built by rotor through the full production sidecar path (embedded extraction, project-resolved typescript, warm session).

**Files:**

- Create: `testdata/transformers/project/package.json`
- Create: `testdata/transformers/project/tsconfig.json`
- Create: `testdata/transformers/project/default.project.json`
- Create: `testdata/transformers/project/src/shared/env.ts`
- Create: `testdata/transformers/project/src/server/services/test.service.ts`
- Create: `testdata/transformers/project/src/server/main.server.ts`
- Create: `internal/compile/transformers_integration_test.go`
- Modify: `.gitignore` (or create entries; check existing patterns first)
- Modify: `.github/actions/setup/action.yml`
- Modify: `.github/workflows/ci.yml` (add a sidecar JS test step)
- [x] **Step 1: Create the fixture project**

`testdata/transformers/project/package.json` (versions mirror `testdata/diff/project` plus the transformer packages):

```json
{
	"name": "rotor-transformers-fixture",
	"version": "1.0.0",
	"devDependencies": {
		"roblox-ts": "3.0.0",
		"typescript": "5.5.3",
		"@rbxts/compiler-types": "3.0.0-types.0",
		"@rbxts/types": "^1.0.800",
		"@flamework/core": "1.3.2",
		"rbxts-transformer-flamework": "1.3.2",
		"rbxts-transform-env": "3.0.0"
	}
}
```

`testdata/transformers/project/tsconfig.json` (Flamework requires `experimentalDecorators` and the `@flamework` typeRoot):

```json
{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"downlevelIteration": true,
		"module": "commonjs",
		"moduleResolution": "Node",
		"noLib": true,
		"resolveJsonModule": true,
		"forceConsistentCasingInFileNames": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"experimentalDecorators": true,
		"typeRoots": ["node_modules/@rbxts", "node_modules/@flamework"],
		"rootDir": "src",
		"outDir": "out",
		"incremental": true,
		"tsBuildInfoFile": "out/tsconfig.tsbuildinfo",
		"plugins": [
			{ "transform": "rbxts-transformer-flamework" },
			{ "transform": "rbxts-transform-env" }
		]
	},
	"include": ["src"]
}
```

`testdata/transformers/project/default.project.json` (game-type tree — the representative Flamework setup; `Flamework.addPaths` resolves instance paths through it):

```json
{
	"name": "transformers-fixture",
	"tree": {
		"$className": "DataModel",
		"ServerScriptService": {
			"$className": "ServerScriptService",
			"TS": { "$path": "out/server" }
		},
		"ReplicatedStorage": {
			"$className": "ReplicatedStorage",
			"rbxts_include": {
				"$path": "include",
				"node_modules": {
					"$className": "Folder",
					"@rbxts": { "$path": "node_modules/@rbxts" },
					"@flamework": { "$path": "node_modules/@flamework" }
				}
			},
			"TS": { "$path": "out/shared" }
		}
	}
}
```

`testdata/transformers/project/src/shared/env.ts`:

```ts
import { $env } from "rbxts-transform-env";

export const apiUrl = $env.string("ROTOR_FIXTURE_API_URL", "https://fallback.example");
export const flagEnabled = $env.boolean("ROTOR_FIXTURE_FLAG");
```

`testdata/transformers/project/src/server/services/test.service.ts`:

```ts
import { OnStart, Service } from "@flamework/core";

@Service()
export class TestService implements OnStart {
	public onStart(): void {
		print("TestService started");
	}
}
```

`testdata/transformers/project/src/server/main.server.ts`:

```ts
import { Flamework } from "@flamework/core";

Flamework.addPaths("src/server/services");
Flamework.ignite();
```

Note: exact emitted shapes (identifier strings, addPaths rewriting) are asserted loosely in Step 4; if the real packages typecheck differently than expected (e.g. flamework needs an `include` folder mapping tweak), adjust the fixture, not the assertions' intent. Verify against `rbxts-transform-env`'s published README (`$env.string(variable, default)`, `$env.boolean(variable)`) and Flamework's install docs if anything fails.

- [x] **Step 2: Install fixture dependencies and add ignore entries**

Run: `cd testdata/transformers/project && bun install --no-save`

Check the repo `.gitignore` style first (`Read .gitignore`), then add:

```text
testdata/transformers/project/node_modules/
testdata/transformers/project/out/
testdata/transformers/project/flamework.build
```

- [x] **Step 3: Write the failing integration test**

`internal/compile/transformers_integration_test.go`:

```go
package compile

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func transformersFixtureDir(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller failed")
	}
	dir := filepath.Clean(filepath.Join(filepath.Dir(file), "..", "..", "testdata", "transformers", "project"))
	if _, err := os.Stat(filepath.Join(dir, "node_modules", "rbxts-transformer-flamework", "package.json")); err != nil {
		t.Skipf("transformers fixture dependencies not installed (run `bun install --no-save` in testdata/transformers/project): %v", err)
	}
	if _, err := exec.LookPath("node"); err != nil {
		t.Skipf("node not on PATH: %v", err)
	}
	return dir
}

// TestTransformersFixtureFlameworkAndEnv runs the full production plugin
// path: embedded sidecar extraction (no ROTOR_SIDECAR_PATH), typescript
// resolved from the fixture's node_modules, warm session, real packages.
func TestTransformersFixtureFlameworkAndEnv(t *testing.T) {
	dir := transformersFixtureDir(t)
	t.Cleanup(closeSidecarSessions)
	closeSidecarSessions()
	t.Setenv("ROTOR_SIDECAR_PATH", "")
	redirectUserCacheDir(t)
	t.Setenv("ROTOR_FIXTURE_API_URL", "https://env.example")

	if err := os.RemoveAll(filepath.Join(dir, "out")); err != nil {
		t.Fatal(err)
	}
	_ = os.Remove(filepath.Join(dir, "flamework.build"))

	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil {
		t.Fatalf("BuildProjectWithOptions: %v (diags: %v)", err, diags)
	}
	if len(diags) > 0 {
		t.Fatalf("diagnostics: %v", diags)
	}

	envOut := result.Outputs["out/shared/env.luau"]
	if !strings.Contains(envOut, "https://env.example") {
		t.Fatalf("rbxts-transform-env did not inline ROTOR_FIXTURE_API_URL:\n%s", envOut)
	}
	if strings.Contains(envOut, "$env") {
		t.Fatalf("rbxts-transform-env left $env macros in output:\n%s", envOut)
	}

	serviceOut := result.Outputs["out/server/services/test.service.luau"]
	if !strings.Contains(serviceOut, "identifier") || !strings.Contains(serviceOut, "defineMetadata") {
		t.Fatalf("rbxts-transformer-flamework did not inject identifier metadata:\n%s", serviceOut)
	}

	mainOut := result.Outputs["out/server/main.server.luau"]
	if strings.Contains(mainOut, `"src/server/services"`) {
		t.Fatalf("Flamework.addPaths was not rewritten:\n%s", mainOut)
	}
}
```

- [x] **Step 4: Run the test, iterate the fixture until green**

Run: `go test ./internal/compile -run TestTransformersFixtureFlameworkAndEnv -count=1 -v`
Expected first run: may FAIL on fixture details (typecheck of real packages, rojo mapping, assertion text). Iterate on the *fixture* and, only if the real emitted shape legitimately differs, the assertion strings — capture the actual output in the failure message before changing an assertion. The non-negotiable assertions: env value inlined, no `$env` left, flamework metadata present, addPaths rewritten.

- [x] **Step 5: Wire CI**

`.github/actions/setup/action.yml` — extend the fixture install loop:

```yaml
        for dir in tools/sidecar testdata/diff/project testdata/conformance/project testdata/transformers/project; do
```

`.github/workflows/ci.yml` — after the `Build` step, add a sidecar JS test step:

```yaml
      - name: Sidecar tests
        run: node --test test/*.test.js
        working-directory: tools/sidecar
```

- [x] **Step 6: Run the full Go suite once more**

Run: `go test ./... -count=1`
Expected: PASS.

- [x] **Step 7: Commit**

```bash
git add testdata/transformers/project internal/compile/transformers_integration_test.go .gitignore .github
git commit -m "test: prove transformer plugins against real rbxts-transformer-flamework and rbxts-transform-env"
```

---

### Task 6: Documentation, roadmap, and push

**Files:**

- Modify: `docs/sidecar-protocol.md`
- Modify: `roadmap.md`
- Modify: `C:\Users\user\.claude\projects\C--Users-user-Source-roblox-rotor\memory\rotor-project-status.md`
- [x] **Step 1: Update `docs/sidecar-protocol.md`**

Rewrite the stale sections to document:

- Deployment: the sidecar is embedded in the rotor binary and extracted to `<user-cache>/rotor/sidecar-<hash>/`; `ROTOR_SIDECAR_PATH` overrides (repo dev/tests). `tools/sidecar` remains the source of truth.
- TypeScript resolution: resolved from the project's `node_modules` first (the same instance plugins `require`), falling back to the sidecar dir; `typescript-not-found` diagnostic when absent.
- Stdout rule: plugin `console.log` is rerouted to stderr; rotor streams it to the compiler log. Protocol responses are the only stdout traffic.
- Warm sessions: rotor keeps one worker per `(projectDir, tsConfigPath)` for the process lifetime, including across watch rebuilds; `changedFiles` carries stamp-diffed overlay text. Known limitation: a warm worker's plugin view of an edited ambient `.d.ts` can be stale until restart (rotor's own typecheck is unaffected).
- Real-package coverage: `testdata/transformers/project` (Flamework + rbxts-transform-env) and how to run it.
- [x] **Step 2: Update `roadmap.md`**
- Phase 4 transformer line: note the sidecar is now embedded (released binaries work), typescript is project-resolved, stdout-protected, and watch sessions are warm.
- Remove/strike the "Post-v1 Follow-up" bullet "Keep one warm Node sidecar session across `build -w` rebuilds" (done); add the `.d.ts`-staleness note as a known limitation if a follow-up list remains.
- Verification section: add `go test ./internal/compile -run TestTransformersFixture` with the bun-install prerequisite, and note CI now runs the sidecar JS tests and installs `testdata/transformers/project`.
- [x] **Step 3: Update the project-status memory file**

Add to `rotor-project-status.md`: transformer plugin support hardened (embedded sidecar, shared typescript instance, stdout protection, warm watch sessions) and validated against `rbxts-transformer-flamework@1.3.2` + `rbxts-transform-env@3.0.0`; `@rbxts/transform-env` does not exist on npm.

- [x] **Step 4: Final verification**

Run: `gofmt -l internal cmd tools` (expect empty), `go vet ./internal/... ./cmd/... ./tools/...`, `go test ./... -count=1`, `cd tools/sidecar && node --test test/*.test.js`
Expected: all clean/PASS.

- [x] **Step 5: Commit and push**

```bash
git add docs/sidecar-protocol.md roadmap.md docs/superpowers/plans/2026-06-10-rotor-transformer-plugins-hardening.md
git commit -m "docs: document embedded sidecar, warm sessions, and real-transformer coverage"
git push
```

---

## Self-review notes

- **Spec coverage:** "bundled Node helper" → Task 3; "full TypeChecker access / flamework-class plugins work unmodified" → Tasks 1, 2, 5; "keep the JS program warm in watch mode" → Task 4; "projects without plugins never spawn Node" → already true, guarded by existing `TestBuildProjectWithoutPluginsDoesNotRequireNode`.
- **Out of scope (per spec "Out v1"):** `--writeTransformedFiles`. Builtin `afterDeclarations` transformers (`transformPaths`, `transformTypeReferenceDirectives`) only affect `.d.ts` emit upstream, not the plugin pass — not part of this plan.
- **Type consistency check:** `resolveSidecarDir`, `repoSidecarDir`, `setRepoSidecarPath`, `closeSidecarSessions`, `redirectUserCacheDir`, `sidecarEnv(projectDir, sidecarDir)` used consistently across tasks; `runTransformerSidecar(dir, configPath, compileFiles, stampFiles)` signature matches its call site in `applyTransformerSidecar`.
