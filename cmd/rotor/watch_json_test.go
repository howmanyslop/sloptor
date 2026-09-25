package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"rotor/internal/compile"
)

func TestBuildJSONResultSuccessCountsOutputs(t *testing.T) {
	result := &compile.BuildResult{Outputs: map[string]string{"a.luau": "", "b.luau": ""}}

	res := buildJSONResult("", result, nil, 142*time.Millisecond, nil)

	if !res.OK || res.Files != 2 || res.DurationMs != 142 || res.Version != version {
		t.Errorf("res = %+v, want ok, 2 files, 142 ms, version %q", res, version)
	}
	if res.Diagnostics == nil || len(res.Diagnostics) != 0 {
		t.Errorf("diagnostics = %#v, want empty non-nil", res.Diagnostics)
	}
}

func TestBuildJSONResultFailureWithoutDiagnosticsReportsError(t *testing.T) {
	res := buildJSONResult("", nil, nil, 0, errors.New("config broke"))

	if res.OK {
		t.Error("ok = true on failure")
	}
	if len(res.Diagnostics) != 1 || res.Diagnostics[0].Message != "config broke" || res.Diagnostics[0].Severity != "error" {
		t.Errorf("diagnostics = %+v, want one error from err", res.Diagnostics)
	}
}

func TestBuildJSONResultFailureMapsDiagnostics(t *testing.T) {
	diags := []compile.DiagnosticInfo{{Code: "TS2322", Message: "bad"}, {Message: "meh", Warning: true}}

	res := buildJSONResult("", nil, diags, 0, errors.New("build failed"))

	if len(res.Diagnostics) != 2 {
		t.Fatalf("diagnostics = %+v, want 2", res.Diagnostics)
	}
	if got := res.Diagnostics[0]; got.Code != "TS2322" || got.Severity != "error" || got.Message != "bad" {
		t.Errorf("diag[0] = %+v", got)
	}
	if got := res.Diagnostics[1]; got.Severity != "warning" {
		t.Errorf("diag[1] severity = %q, want warning", got.Severity)
	}
}

func TestWatchEventWriterEmitsOneObjectPerLine(t *testing.T) {
	var buf bytes.Buffer
	root := t.TempDir()
	at := time.Date(2026, 9, 25, 18, 0, 0, 142_000_000, time.UTC)
	w := newWatchEventWriter(&buf, root)
	w.now = func() time.Time { return at }

	w.buildStart(nil)
	w.buildStart([]string{filepath.Join(root, "src", "a.ts")})
	w.buildEnd(jsonResult{Version: "v", OK: true, Files: 3, DurationMs: 7, Diagnostics: []jsonDiagnostic{}})

	want := `{"event":"buildStart","at":"2026-09-25T18:00:00.142Z","changed":[]}
{"event":"buildStart","at":"2026-09-25T18:00:00.142Z","changed":["src/a.ts"]}
{"event":"buildEnd","at":"2026-09-25T18:00:00.142Z","version":"v","ok":true,"files":3,"durationMs":7,"diagnostics":[]}
`
	if got := buf.String(); got != want {
		t.Errorf("output:\n%s\nwant:\n%s", got, want)
	}
}

func TestWatchEventWriterWritesEachEventInOneWrite(t *testing.T) {
	var writes countingWriter
	w := newWatchEventWriter(&writes, t.TempDir())

	w.buildStart(nil)
	w.buildEnd(jsonResult{})

	if writes.n != 2 {
		t.Errorf("writes = %d, want 2 (one per NDJSON line, so pipes see whole lines)", writes.n)
	}
}

type countingWriter struct{ n int }

func (c *countingWriter) Write(p []byte) (int, error) {
	c.n++
	return len(p), nil
}

// watchEventStream runs a JSON watch loop in the background and yields its
// NDJSON lines as decoded maps. The loop stops when the test ends.
func watchEventStream(t *testing.T, loop func(ctx context.Context, out io.Writer)) <-chan map[string]any {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	r, w := io.Pipe()
	done := make(chan struct{})
	go func() {
		defer close(done)
		loop(ctx, w)
		_ = w.Close()
	}()
	events := make(chan map[string]any, 16)
	go func() {
		defer close(events)
		scanner := bufio.NewScanner(r)
		scanner.Buffer(nil, 1<<20)
		for scanner.Scan() {
			var event map[string]any
			if err := json.Unmarshal(scanner.Bytes(), &event); err != nil {
				t.Errorf("non-JSON line %q: %v", scanner.Text(), err)
				continue
			}
			events <- event
		}
	}()
	t.Cleanup(func() {
		cancel()
		go func() {
			for range events {
			}
		}()
		select {
		case <-done:
		case <-time.After(10 * time.Second):
			t.Error("watch loop did not stop after cancel")
		}
	})
	return events
}

func nextWatchEvent(t *testing.T, events <-chan map[string]any, want string) map[string]any {
	t.Helper()
	select {
	case event, ok := <-events:
		if !ok {
			t.Fatalf("stream closed, want %s", want)
		}
		if event["event"] != want {
			t.Fatalf("event = %v, want %s", event, want)
		}
		return event
	case <-time.After(30 * time.Second):
		t.Fatalf("timed out waiting for %s", want)
	}
	return nil
}

func TestBuildWatchJSONEmitsPairedEventsPerBuild(t *testing.T) {
	dir := writeBuildableProject(t, "")
	tsConfigPath := filepath.Join(dir, "tsconfig.json")
	events := watchEventStream(t, func(ctx context.Context, out io.Writer) {
		runBuildWatchLoop(ctx, dir, tsConfigPath, defaultProjectOptions, newBuildWatchJSONReporter(out, dir))
	})

	// Initial build: a pair with no changed files.
	start := nextWatchEvent(t, events, "buildStart")
	if changed, _ := start["changed"].([]any); changed == nil || len(changed) != 0 {
		t.Errorf("initial changed = %v, want []", start["changed"])
	}
	end := nextWatchEvent(t, events, "buildEnd")
	if end["ok"] != true || end["files"].(float64) <= 0 {
		t.Errorf("initial buildEnd = %v, want ok with files", end)
	}

	// A type error: the next pair names the file and carries the diagnostic.
	mustWrite(t, filepath.Join(dir, "src", "main.ts"), "export const s: string = 5;\n")
	start = nextWatchEvent(t, events, "buildStart")
	if changed, _ := start["changed"].([]any); len(changed) != 1 || changed[0] != "src/main.ts" {
		t.Errorf("changed = %v, want [src/main.ts]", start["changed"])
	}
	end = nextWatchEvent(t, events, "buildEnd")
	diags, _ := end["diagnostics"].([]any)
	if end["ok"] != false || len(diags) == 0 {
		t.Fatalf("buildEnd = %v, want failure with diagnostics", end)
	}
	if diag := diags[0].(map[string]any); diag["code"] != "TS2322" || diag["severity"] != "error" || diag["file"] != "src/main.ts" {
		t.Errorf("diagnostic = %v, want TS2322 error in src/main.ts", diag)
	}

	// A save with no content change still yields exactly one pair.
	later := time.Now().Add(2 * time.Second)
	if err := os.Chtimes(filepath.Join(dir, "src", "main.ts"), later, later); err != nil {
		t.Fatal(err)
	}
	nextWatchEvent(t, events, "buildStart")
	nextWatchEvent(t, events, "buildEnd")
}

func TestCheckWatchJSONEmitsPairedEventsPerCheck(t *testing.T) {
	dir := writeBuildableProject(t, "")
	events := watchEventStream(t, func(ctx context.Context, out io.Writer) {
		runCheckWatchLoop(ctx, dir, nil, newCheckWatchJSONReporter(out, dir))
	})

	start := nextWatchEvent(t, events, "buildStart")
	if changed, _ := start["changed"].([]any); changed == nil || len(changed) != 0 {
		t.Errorf("initial changed = %v, want []", start["changed"])
	}
	end := nextWatchEvent(t, events, "buildEnd")
	if end["ok"] != true || end["files"].(float64) <= 0 {
		t.Errorf("initial buildEnd = %v, want ok with files", end)
	}

	mustWrite(t, filepath.Join(dir, "src", "main.ts"), "export const s: string = 5;\n")
	start = nextWatchEvent(t, events, "buildStart")
	if changed, _ := start["changed"].([]any); len(changed) != 1 || changed[0] != "src/main.ts" {
		t.Errorf("changed = %v, want [src/main.ts]", start["changed"])
	}
	end = nextWatchEvent(t, events, "buildEnd")
	diags, _ := end["diagnostics"].([]any)
	if end["ok"] != false || len(diags) == 0 {
		t.Fatalf("buildEnd = %v, want failure with diagnostics", end)
	}
	if diag := diags[0].(map[string]any); diag["code"] != "TS2322" || diag["file"] != "src/main.ts" {
		t.Errorf("diagnostic = %v, want TS2322 in src/main.ts", diag)
	}
}

func TestBuildSolutionWatchJSONRejected(t *testing.T) {
	dir := writeBuildableProject(t, "")

	_, stderr, code := captureBuildOutput(t, []string{"--build", "--watch", "--json", dir})

	if code != 1 || !strings.Contains(stderr, "--json cannot be used with --build --watch") {
		t.Errorf("exit = %d, stderr = %q, want the --build --watch --json rejection", code, stderr)
	}
}

func TestCmdBuildJSONDiagnosticPathIsProjectRelative(t *testing.T) {
	dir := writeBuildableProject(t, "export const s: string = 5;\n")

	output, _ := captureStdout(t, func() int { return cmdBuild([]string{"--json", dir}) })

	var res jsonResult
	if err := json.Unmarshal([]byte(strings.TrimSpace(output)), &res); err != nil {
		t.Fatalf("output is not valid JSON: %v\n%s", err, output)
	}
	if len(res.Diagnostics) == 0 || res.Diagnostics[0].File != "src/main.ts" {
		t.Errorf("diagnostics = %+v, want file src/main.ts (relative to the project, like check)", res.Diagnostics)
	}
}
