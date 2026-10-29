package compile

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strconv"
	"strings"
	"testing"
	"time"

	"rotor/internal/config"
	"rotor/internal/logservice"
)

// These focused process tests are prepared for execution after source review.
// They use the existing Node sidecar and synthetic transformer fixture only.
func parallelFixtureIdentity(t *testing.T, sidecarDir, dir, entry string) (string, string) {
	t.Helper()
	cmd := exec.Command("node", "-e", `const path = require('node:path');
const [worker, project, entry] = process.argv.slice(1);
const ts = require(path.join(worker, 'index.js')).resolveTypeScript(project);
const digest = require(path.join(worker, 'lib/parallel.js')).packageDigest(entry);
process.stdout.write(JSON.stringify({ version: ts.version, digest }));`, sidecarDir, dir, entry)
	cmd.Dir, cmd.Env = dir, sidecarEnv(dir, sidecarDir)
	output, err := cmd.Output()
	if err != nil {
		t.Fatalf("fixture identity: %v", err)
	}
	var identity struct{ Version, Digest string }
	if err := json.Unmarshal(output, &identity); err != nil {
		t.Fatal(err)
	}
	return identity.Version, identity.Digest
}

func TestParallelRealNodeWholeResponseMatchesSerial(t *testing.T) {
	sidecarDir := repoSidecarDir(t)
	dir := writeProject(t, "@scope/parallel-fixture", "")
	// Use the ordinary sidecar TypeScript fallback; this synthetic fixture
	// does not require a separate testdata dependency installation.
	if err := os.MkdirAll(filepath.Join(dir, "plugins"), 0o755); err != nil {
		t.Fatal(err)
	}
	for name, text := range map[string]string{
		"src/main.ts": `export const a = "first";`, "src/second.ts": `export const b = "second";`,
		"plugins/package.json": `{"name":"reviewed-parallel-fixture"}`, "plugins/bad.js": `module.exports = {};`,
		"plugins/prefix-string.js": prefixStringPlugin,
		"tsconfig.json":            `{"compilerOptions":{"noLib":true,"target":"ESNext","module":"CommonJS"},"include":["src"]}`,
	} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(text), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	plugins := []json.RawMessage{json.RawMessage(`{"transform":"./plugins/prefix-string.js","prefix":"parallel"}`), json.RawMessage(`{"transform":"./plugins/bad.js"}`)}
	version, digest := parallelFixtureIdentity(t, sidecarDir, dir, filepath.Join(dir, "plugins/prefix-string.js"))
	profile := &config.ParallelTransformersConfig{Workers: 2, BatchIndependent: true, TypeScript: version}
	for _, plugin := range plugins {
		profile.Plugins = append(profile.Plugins, config.ParallelTransformerPlugin{Config: string(plugin), SHA256: digest})
	}
	request := sidecarRequest{
		Protocol: 1, Operation: "transform", ProjectDir: dir, TsConfigPath: filepath.Join(dir, "tsconfig.json"), Plugins: plugins,
		CompileFileNames: []string{filepath.Join(dir, "src/main.ts"), filepath.Join(dir, "src/missing-a.ts"), filepath.Join(dir, "src/second.ts"), filepath.Join(dir, "src/missing-b.ts")},
		ChangedFiles:     []sidecarChangedFile{{FileName: filepath.Join(dir, "src/second.ts"), Text: `export const b = "overlaid";`}},
	}
	serial, err := spawnSidecarSessionMode(dir, sidecarDir, true)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if serial.cmd.ProcessState == nil {
			serial.close()
		}
	})
	ctx, cancel := context.WithTimeout(t.Context(), 30*time.Second)
	defer cancel()
	baseline, err := parallelRoundTrip(ctx, serial, request, &sidecarCallStats{})
	if err != nil {
		t.Fatal(err)
	}
	if err := serial.closeAndWait(); err != nil {
		t.Fatal(err)
	}
	var forwarded bytes.Buffer
	previous, verbose := logservice.Output, logservice.Verbose
	logservice.Output, logservice.Verbose = &forwarded, false
	defer func() { logservice.Output, logservice.Verbose = previous, verbose }()
	serial.stderr.drainTo()
	baselineStderr := forwarded.String()
	forwarded.Reset()
	first, err := spawnSidecarSessionMode(dir, sidecarDir, true)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if first.cmd.ProcessState == nil {
			first.close()
		}
	})
	var second *sidecarSession
	t.Cleanup(func() {
		if second != nil && second.cmd.ProcessState == nil {
			second.close()
		}
	})
	spawn := func(dir, worker string, owned bool) (*sidecarSession, error) {
		var err error
		second, err = spawnSidecarSessionMode(dir, worker, owned)
		return second, err
	}
	response, handled, err := runParallelTransformerWithSpawner(ctx, 30*time.Second, sidecarDir, &sidecarSlot{session: first}, request, profile, &sidecarCallStats{}, spawn)
	if err != nil || !handled {
		t.Fatalf("parallel: handled=%v %v", handled, err)
	}
	if response == nil {
		t.Fatal("parallel transformer returned nil response")
	}
	baseline.Metrics, response.Metrics = nil, nil
	if len(baseline.Diagnostics) != 3 || len(baseline.Transformed) != 2 || !reflect.DeepEqual(baseline, response) || baselineStderr != forwarded.String() {
		t.Fatalf("whole serial/parallel response or forwarded stderr differs:\n%#v\n%#v", baseline, response)
	}
	for _, worker := range []*sidecarSession{first, second} {
		if worker == nil || worker.cmd.ProcessState == nil || !worker.cmd.ProcessState.Success() {
			t.Fatal("worker was not successfully waited/reaped")
		}
	}
}

func TestParallelRealProcessCleanupCannotBeGreen(t *testing.T) {
	for _, mode := range []string{"inherited-stderr", "abnormal-exit", "negative-preflight-inherited-stderr", "negative-preflight-abnormal-exit", "preflight-timeout", "preflight-timeout-inherited-stderr"} {
		t.Run(mode, func(t *testing.T) {
			dir, other, workerDir := t.TempDir(), t.TempDir(), t.TempDir()
			script := `const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const modeFile=path.join(process.cwd(),'cleanup-mode');
const mode=fs.existsSync(modeFile) ? fs.readFileSync(modeFile,'utf8') : '';
function holdStderr() {
 // On Windows, non-detached children die with libuv's parent-owned job.
 // Detached skips that nested job; the outer validation owner still owns it.
 const child=require('node:child_process').spawn(process.execPath,['-e','setTimeout(()=>{},60000)'],{detached:true,windowsHide:true,stdio:['ignore','ignore',2]});
 fs.writeFileSync(path.join(process.cwd(),'descendant.pid'),String(child.pid));
 child.unref();
}
const rl = readline.createInterface({input:process.stdin});
rl.on('line', line => {
 const request=JSON.parse(line);
 if (request.operation==='parallel-preflight') {
  if (mode.startsWith('preflight-timeout')) {
   if (mode.endsWith('inherited-stderr')) holdStderr();
   setInterval(()=>{},60000);
   return;
  }
  process.stdout.write(JSON.stringify({parallelReady:!mode.startsWith('negative-preflight'),diagnostics:[]})+'\n');
 } else {
  fs.appendFileSync(path.join(process.cwd(),'transform-dispatch'),'transform\n');
  process.stdout.write(JSON.stringify({diagnostics:[],transformed:[]})+'\n');
 }
});
rl.on('close', () => {
 if (!mode || mode.startsWith('preflight-timeout')) return;
 if (mode.endsWith('abnormal-exit')) process.exit(23);
 holdStderr();
});`
			if err := os.WriteFile(filepath.Join(workerDir, "main.js"), []byte(script), 0o600); err != nil {
				t.Fatal(err)
			}
			modeDir := dir
			negative := strings.HasPrefix(mode, "negative-preflight")
			timedOut := strings.HasPrefix(mode, "preflight-timeout")
			if negative || timedOut {
				modeDir = other
			}
			if err := os.WriteFile(filepath.Join(modeDir, "cleanup-mode"), []byte(mode), 0o600); err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() {
				if text, err := os.ReadFile(filepath.Join(modeDir, "descendant.pid")); err == nil {
					pid, err := strconv.Atoi(strings.TrimSpace(string(text)))
					if err != nil {
						t.Error(err)
						return
					}
					if process, err := os.FindProcess(pid); err == nil {
						_ = process.Kill()
						_ = process.Release()
					}
				}
			})
			first, err := spawnSidecarSessionMode(dir, workerDir, true)
			if err != nil {
				t.Fatal(err)
			}
			var second *sidecarSession
			t.Cleanup(func() {
				if first.cmd.ProcessState == nil {
					first.close()
				}
				if second != nil && second.cmd.ProcessState == nil {
					second.close()
				}
			})
			spawn := func(string, string, bool) (*sidecarSession, error) {
				var err error
				second, err = spawnSidecarSessionMode(other, workerDir, true)
				return second, err
			}
			slot := &sidecarSlot{session: first}
			timeout := 10 * time.Second
			if timedOut {
				timeout = 2 * time.Second
			}
			_, handled, err := runParallelTransformerWithSpawner(t.Context(), timeout, workerDir, slot, sidecarRequest{CompileFileNames: []string{"a", "b"}}, nil, &sidecarCallStats{}, spawn)
			if mode == "preflight-timeout" {
				if handled || err != nil || slot.session != first || first.cmd.ProcessState != nil || second == nil || second.cmd.ProcessState == nil {
					t.Fatalf("expected auxiliary kill lost serial fallback: handled=%v err=%v", handled, err)
				}
				if second.cmd.ProcessState.Success() {
					t.Fatal("timeout fixture did not exercise forced process termination")
				}
				ctx, cancel := context.WithTimeout(t.Context(), 10*time.Second)
				defer cancel()
				if _, err := parallelRoundTrip(ctx, first, sidecarRequest{Operation: "transform"}, &sidecarCallStats{}); err != nil {
					t.Fatalf("fresh serial request: %v", err)
				}
				if err := first.closeAndWait(); err != nil {
					t.Fatal(err)
				}
				if text, err := os.ReadFile(filepath.Join(dir, "transform-dispatch")); err != nil || string(text) != "transform\n" {
					t.Fatalf("expected exactly one serial dispatch: %q %v", text, err)
				}
				if _, err := os.Stat(filepath.Join(other, "transform-dispatch")); !errors.Is(err, os.ErrNotExist) {
					t.Fatalf("auxiliary transformed after timeout: %v", err)
				}
				return
			}
			if !handled || err == nil {
				t.Fatalf("cleanup was green: handled=%v err=%v", handled, err)
			}
			if strings.HasSuffix(mode, "inherited-stderr") && !errors.Is(err, exec.ErrWaitDelay) {
				t.Fatalf("incomplete drain not surfaced: %v", err)
			}
			if strings.HasSuffix(mode, "abnormal-exit") {
				var exitErr *exec.ExitError
				if !errors.As(err, &exitErr) || exitErr.ExitCode() != 23 {
					t.Fatalf("independent abnormal exit not surfaced: %v", err)
				}
			}
			if slot.session != nil {
				t.Fatal("failed cleanup retained primary session")
			}
			if timedOut && (second == nil || second.cmd.ProcessState == nil || second.cmd.ProcessState.Success()) {
				t.Fatal("combined fixture did not exercise forced process termination")
			}
			if negative || timedOut {
				for _, workerDir := range []string{dir, other} {
					if _, err := os.Stat(filepath.Join(workerDir, "transform-dispatch")); !errors.Is(err, os.ErrNotExist) {
						t.Fatalf("transform dispatched after preflight cleanup failure: %v", err)
					}
				}
			}
			for _, session := range []*sidecarSession{first, second} {
				if session == nil || session.cmd.ProcessState == nil {
					t.Fatal("direct worker not waited/reaped")
				}
			}
		})
	}
}
