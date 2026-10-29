package lsp_test

import (
	"context"
	"errors"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"rotor/internal/compile"
)

// Every test here drives the server through real Content-Length framed
// messages and asserts on what an editor would receive. The only fake is the
// compiler behind Options.Census.

// Catches an editor that cannot start the server: a handshake that does not
// advertise full-document sync, names the server wrongly, or ignores a
// client's offer of UTF-8 positions (which would shift every squiggle).
func TestInitializeHandshake(t *testing.T) {
	p := newProject(t, "a.ts")

	t.Run("defaults to utf-16", func(t *testing.T) {
		c := startServer(t, serveConfig{census: p.noAnyCensus, findCfg: p.findTsConfig})
		result := c.handshake(nil, nil)

		info := result["serverInfo"].(map[string]any)
		if info["name"] != "sloptor" {
			t.Errorf("serverInfo.name = %v, want sloptor", info["name"])
		}
		caps := result["capabilities"].(map[string]any)
		if got, ok := caps["positionEncoding"]; ok && got != "utf-16" {
			t.Errorf("positionEncoding = %v, want utf-16 when the client offers nothing", got)
		}
		sync := caps["textDocumentSync"].(map[string]any)
		if sync["change"] != float64(1) {
			t.Errorf("textDocumentSync.change = %v, want 1 (full)", sync["change"])
		}
		if sync["openClose"] != true {
			t.Errorf("textDocumentSync.openClose = %v, want true", sync["openClose"])
		}
		if sync["save"] == nil || sync["save"] == false {
			t.Errorf("textDocumentSync.save = %v, want enabled", sync["save"])
		}
	})

	t.Run("picks utf-8 when offered", func(t *testing.T) {
		c := startServer(t, serveConfig{census: p.noAnyCensus, findCfg: p.findTsConfig})
		result := c.handshake([]string{"utf-8", "utf-16"}, nil)
		caps := result["capabilities"].(map[string]any)
		if caps["positionEncoding"] != "utf-8" {
			t.Errorf("positionEncoding = %v, want utf-8", caps["positionEncoding"])
		}
	})
}

// Catches a squiggle drawn in the wrong place: a byte offset used as an LSP
// character, or UTF-16 units miscounted when an astral character (two units,
// four bytes) precedes the span on the same line.
func TestTransformerDiagnosticRange(t *testing.T) {
	// Line 1 is: const s = "<emoji>"; let x: any;
	// Counting characters by hand, `any` starts at column 23 in UTF-16 units
	// (the emoji is 2 units) and at column 25 in UTF-8 bytes (the emoji is 4).
	const text = "// héllo\nconst s = \"\U0001F600\"; let x: any;"

	tests := []struct {
		name      string
		encodings []string
		start     int
		end       int
	}{
		{"utf-16", nil, 23, 26},
		{"utf-8", []string{"utf-8"}, 25, 28},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p := newProject(t, "a.ts")
			c := startServer(t, serveConfig{census: p.noAnyCensus, findCfg: p.findTsConfig})
			c.handshake(tt.encodings, nil)
			c.didOpen(p.files[0], text)

			published := decodePublish(t, c.waitFor(1, publishFor(uriOf(p.files[0]))))
			if len(published.Diagnostics) != 1 {
				t.Fatalf("got %d diagnostics, want 1: %+v", len(published.Diagnostics), published.Diagnostics)
			}
			d := published.Diagnostics[0]
			if d.Range.Start.Line != 1 || d.Range.End.Line != 1 {
				t.Errorf("lines = %d..%d, want 1..1", d.Range.Start.Line, d.Range.End.Line)
			}
			if d.Range.Start.Character != tt.start || d.Range.End.Character != tt.end {
				t.Errorf("columns = %d..%d, want %d..%d", d.Range.Start.Character, d.Range.End.Character, tt.start, tt.end)
			}
			if d.Code != "noAny" {
				t.Errorf("code = %q, want noAny", d.Code)
			}
			if d.Source != "sloptor" {
				t.Errorf("source = %q, want sloptor", d.Source)
			}
			if d.Severity != 1 {
				t.Errorf("severity = %d, want 1 (error)", d.Severity)
			}
			if !strings.Contains(d.Message, "any") {
				t.Errorf("message = %q, want the transformer's message", d.Message)
			}
		})
	}
}

// Catches a stale squiggle: a diagnostic that stays on screen after the user
// fixes the code, because the server never tells the editor the file is clean.
func TestFixedFileIsCleared(t *testing.T) {
	p := newProject(t, "a.ts")
	c := startServer(t, serveConfig{census: p.noAnyCensus, findCfg: p.findTsConfig, debounce: debounceForTests})
	c.handshake(nil, nil)

	c.didOpen(p.files[0], "let x: any;")
	first := decodePublish(t, c.waitFor(1, publishFor(uriOf(p.files[0]))))
	if len(first.Diagnostics) != 1 {
		t.Fatalf("expected a diagnostic before the fix, got %+v", first.Diagnostics)
	}

	c.didChange(p.files[0], 2, "let x: unknown;")
	cleared := decodePublish(t, c.waitFor(2, publishFor(uriOf(p.files[0]))))
	if len(cleared.Diagnostics) != 0 {
		t.Errorf("expected the diagnostics to be cleared, got %+v", cleared.Diagnostics)
	}
	if c.count(publishFor(uriOf(p.files[0]))) != 2 {
		t.Errorf("a clean file must be cleared once, not republished")
	}
}

// Catches duplicated and unwanted noise: the editor's TypeScript server already
// reports type errors, so sloptor must not repeat them by default; but an
// editor without one opts in with typeErrors.
func TestTypeScriptErrorsAreOptIn(t *testing.T) {
	typeErrorCensus := func(p project) func(context.Context, string, map[string]string) (*compile.ProjectDiagnostics, error) {
		typeOnlyPath := filepath.ToSlash(p.files[0])
		bothPath := filepath.ToSlash(p.files[1])
		return func(context.Context, string, map[string]string) (*compile.ProjectDiagnostics, error) {
			typeOnly := compile.FileDiagnostics{
				FileName: typeOnlyPath, Outcome: compile.FileOutcomeTypeError, Transformed: true,
				Diagnostics: []compile.DiagnosticInfo{{Code: "TS2322", Message: "Type 'string' is not assignable to type 'number'.", FileName: typeOnlyPath, Offset: 4, Len: 1}},
			}
			both := compile.FileDiagnostics{
				FileName: bothPath, Outcome: compile.FileOutcomeTransformerDiagnostic, Transformed: true,
				Diagnostics: []compile.DiagnosticInfo{
					{Code: "TS2304", Message: "Cannot find name 'q'.", FileName: bothPath, Offset: 0, Len: 1},
					{Code: "noAny", Message: "no any", FileName: bothPath, Offset: 0, Len: 1},
				},
			}
			// Warnings must stay warnings.
			both.Diagnostics = append(both.Diagnostics, compile.DiagnosticInfo{Code: "someWarning", Message: "careful", Warning: true, FileName: bothPath, Offset: 0, Len: 1})
			return &compile.ProjectDiagnostics{Files: []compile.FileDiagnostics{typeOnly, both}}, nil
		}
	}

	t.Run("hidden by default", func(t *testing.T) {
		p := newProject(t, "types.ts", "both.ts")
		c := startServer(t, serveConfig{census: typeErrorCensus(p), findCfg: p.findTsConfig})
		c.handshake(nil, nil)
		c.didOpen(p.files[1], "q")

		// Files are published in program order, so once the second file's
		// publication has arrived the first file's would have too.
		published := decodePublish(t, c.waitFor(1, publishFor(uriOf(p.files[1]))))
		codes := []string{}
		for _, d := range published.Diagnostics {
			codes = append(codes, d.Code)
		}
		if strings.Join(codes, ",") != "noAny,someWarning" {
			t.Errorf("codes = %v, want only sloptor's own [noAny someWarning]", codes)
		}
		if published.Diagnostics[1].Severity != 2 {
			t.Errorf("warning severity = %d, want 2", published.Diagnostics[1].Severity)
		}
		if n := c.count(publishFor(uriOf(p.files[0]))); n != 0 {
			t.Errorf("a file with only type errors got %d publications, want none", n)
		}
	})

	t.Run("shown with typeErrors", func(t *testing.T) {
		p := newProject(t, "types.ts", "both.ts")
		c := startServer(t, serveConfig{census: typeErrorCensus(p), findCfg: p.findTsConfig})
		c.handshake(nil, map[string]any{"typeErrors": true})
		c.didOpen(p.files[1], "q")

		typeOnly := decodePublish(t, c.waitFor(1, publishFor(uriOf(p.files[0]))))
		if len(typeOnly.Diagnostics) != 1 || typeOnly.Diagnostics[0].Code != "TS2322" {
			t.Errorf("type-error file diagnostics = %+v, want one TS2322", typeOnly.Diagnostics)
		}
		both := decodePublish(t, c.waitFor(1, publishFor(uriOf(p.files[1]))))
		if len(both.Diagnostics) != 3 {
			t.Errorf("mixed file has %d diagnostics, want 3", len(both.Diagnostics))
		}
	})
}

// Catches a compiler crash going unreported: the user would see a file with no
// squiggles and conclude it compiled.
func TestInternalCompilerErrorIsPublished(t *testing.T) {
	p := newProject(t, "a.ts")
	fileName := filepath.ToSlash(p.files[0])
	census := func(context.Context, string, map[string]string) (*compile.ProjectDiagnostics, error) {
		return &compile.ProjectDiagnostics{Files: []compile.FileDiagnostics{{
			FileName: fileName, Outcome: compile.FileOutcomeInternalCompilerError,
			InternalError: &compile.InternalCompilerError{FileName: fileName, Value: "assertion failed"},
		}}}, nil
	}
	c := startServer(t, serveConfig{census: census, findCfg: p.findTsConfig})
	c.handshake(nil, nil)
	c.didOpen(p.files[0], "let a = 1;\nlet b = 2;")

	published := decodePublish(t, c.waitFor(1, publishFor(uriOf(p.files[0]))))
	if len(published.Diagnostics) != 1 {
		t.Fatalf("got %d diagnostics, want exactly 1: %+v", len(published.Diagnostics), published.Diagnostics)
	}
	d := published.Diagnostics[0]
	if d.Severity != 1 {
		t.Errorf("severity = %d, want error", d.Severity)
	}
	if d.Range.Start.Line != 0 || d.Range.Start.Character != 0 {
		t.Errorf("range starts at %+v, want 0:0", d.Range.Start)
	}
	if !strings.Contains(d.Message, "assertion failed") {
		t.Errorf("message = %q, want the panic value", d.Message)
	}
}

// Catches a popup storm: a project that cannot compile fails on every
// keystroke, and raising the same window message each time would bury the
// editor. It must also come back if the problem goes away and returns.
func TestCensusErrorIsShownOncePerOccurrence(t *testing.T) {
	p := newProject(t, "a.ts")
	started := make(chan struct{}, 16)
	// A buffer containing "fail" stands for a project that cannot be set up.
	census := func(ctx context.Context, config string, overlays map[string]string) (*compile.ProjectDiagnostics, error) {
		started <- struct{}{}
		if strings.Contains(overlayText(overlays, p.files[0]), "fail") {
			return &compile.ProjectDiagnostics{}, errors.New("tsconfig.json is unreadable")
		}
		return p.noAnyCensus(ctx, config, overlays)
	}
	c := startServer(t, serveConfig{census: census, findCfg: p.findTsConfig, debounce: debounceForTests})
	c.handshake(nil, nil)

	c.didOpen(p.files[0], "fail")
	<-started
	c.waitFor(1, isShowMessage)
	c.didSave(p.files[0]) // the same failure again
	<-started
	c.didChange(p.files[0], 2, "let x: any;") // the problem goes away
	<-started                                 // a run starting means the one before it has finished publishing
	c.waitFor(1, publishFor(uriOf(p.files[0])))
	if n := c.count(isShowMessage); n != 1 {
		t.Fatalf("got %d window messages for one persistent failure, want 1", n)
	}

	c.didChange(p.files[0], 3, "fail") // and it comes back
	second := c.waitFor(2, isShowMessage)
	if !strings.Contains(string(second.Params), "tsconfig.json is unreadable") {
		t.Errorf("message params = %s, want the census error text", second.Params)
	}
	if !strings.Contains(string(second.Params), `"type":1`) {
		t.Errorf("message params = %s, want type 1 (error)", second.Params)
	}
}

// Catches project-level problems (not tied to a file) being swallowed.
func TestProjectDiagnosticsAreShown(t *testing.T) {
	p := newProject(t, "a.ts")
	census := func(context.Context, string, map[string]string) (*compile.ProjectDiagnostics, error) {
		return &compile.ProjectDiagnostics{Diagnostics: []compile.DiagnosticInfo{
			{Message: "Rojo project not found"},
			{Code: "TS5083", Message: "Cannot read file tsconfig.base.json."},
		}}, nil
	}
	c := startServer(t, serveConfig{census: census, findCfg: p.findTsConfig})
	c.handshake(nil, nil)
	c.didOpen(p.files[0], "x")

	m := c.waitFor(1, isShowMessage)
	if !strings.Contains(string(m.Params), "Rojo project not found") {
		t.Errorf("params = %s, want the project diagnostic", m.Params)
	}
	c.ping(10)
	if n := c.count(isShowMessage); n != 1 {
		t.Errorf("got %d window messages, want 1 (the TypeScript one is hidden by default)", n)
	}
}

// Catches a burst of edits queueing one compile per keystroke (or, worse,
// dropping the last edit): edits during a run must cost exactly one more run,
// and that run must see the newest text.
func TestEditsDuringARunCoalesceIntoOneMoreRun(t *testing.T) {
	p := newProject(t, "a.ts")
	var calls atomic.Int32
	started := make(chan struct{}, 8)
	release := make(chan struct{})
	census := func(ctx context.Context, config string, overlays map[string]string) (*compile.ProjectDiagnostics, error) {
		if calls.Add(1) == 1 {
			started <- struct{}{}
			select {
			case <-release:
			case <-ctx.Done():
			}
		}
		return p.noAnyCensus(ctx, config, overlays)
	}
	c := startServer(t, serveConfig{census: census, findCfg: p.findTsConfig, debounce: debounceForTests})
	c.handshake(nil, nil)

	c.didOpen(p.files[0], "let x = 1;")
	<-started // the first census is now in flight and blocked
	for version := 2; version < 52; version++ {
		c.didChange(p.files[0], version, "let x: unknown;")
	}
	c.didChange(p.files[0], 52, "let last: any;")
	c.ping(10) // every change above has been handled by the server
	close(release)

	published := decodePublish(t, c.waitFor(1, publishFor(uriOf(p.files[0]))))
	if len(published.Diagnostics) != 1 {
		t.Fatalf("the follow-up run did not see the newest text: %+v", published.Diagnostics)
	}
	c.ping(11)
	if got := calls.Load(); got != 2 {
		t.Errorf("census ran %d times for 52 edits behind one in-flight run, want 2", got)
	}
}

// Catches a debounce that never fires: after the user stops typing, the edit
// must show up without any further action.
func TestTypingIsEventuallyApplied(t *testing.T) {
	p := newProject(t, "a.ts")
	var calls atomic.Int32
	started := make(chan struct{}, 8)
	census := func(ctx context.Context, config string, overlays map[string]string) (*compile.ProjectDiagnostics, error) {
		calls.Add(1)
		started <- struct{}{}
		return p.noAnyCensus(ctx, config, overlays)
	}
	c := startServer(t, serveConfig{census: census, findCfg: p.findTsConfig, debounce: debounceForTests})
	c.handshake(nil, nil)

	c.didOpen(p.files[0], "let x: unknown;")
	<-started
	c.didChange(p.files[0], 2, "let x: any;")
	c.waitFor(1, publishFor(uriOf(p.files[0])))
	c.ping(10)
	if got := calls.Load(); got != 2 {
		t.Errorf("an open and one edit caused %d census runs, want 2", got)
	}
}

// Catches a file outside every project taking the server down or raising an
// error: an editor opens scratch files all the time.
func TestFileOutsideAnyProjectIsIgnored(t *testing.T) {
	var calls atomic.Int32
	census := func(context.Context, string, map[string]string) (*compile.ProjectDiagnostics, error) {
		calls.Add(1)
		return &compile.ProjectDiagnostics{}, nil
	}
	findNone := func(string) (string, error) { return "", errors.New("no tsconfig.json found") }
	c := startServer(t, serveConfig{census: census, findCfg: findNone, debounce: debounceForTests})
	c.handshake(nil, nil)

	p := newProject(t, "scratch.ts")
	c.didOpen(p.files[0], "let x: any;")
	c.didChange(p.files[0], 2, "let y: any;")
	c.didSave(p.files[0])
	c.ping(10) // the server is still alive and answering

	if calls.Load() != 0 {
		t.Errorf("census ran %d times for a file with no project, want 0", calls.Load())
	}
	if n := c.count(isShowMessage); n != 0 {
		t.Errorf("got %d window messages for a file with no project, want 0", n)
	}
}

// Catches an editor being told "unsupported" in the wrong way: an unknown
// request must be answered with MethodNotFound (or the client waits forever),
// and an unknown notification must be silently dropped.
func TestUnknownMessages(t *testing.T) {
	p := newProject(t, "a.ts")
	c := startServer(t, serveConfig{census: p.noAnyCensus, findCfg: p.findTsConfig})
	c.handshake(nil, nil)

	c.notify("workspace/didChangeConfiguration", map[string]any{"settings": nil})
	c.notify("$/cancelRequest", map[string]any{"id": 99})
	c.notify("custom/unheardOf", map[string]any{})
	c.request(7, "textDocument/hover", map[string]any{
		"textDocument": map[string]any{"uri": uriOf(p.files[0])},
		"position":     map[string]any{"line": 0, "character": 0},
	})

	response := c.responseTo(7)
	if response.Error == nil || response.Error.Code != -32601 {
		t.Errorf("response to an unsupported request = %+v, want error -32601", response.Error)
	}
	if got := c.count(func(m message) bool { return m.Method == "" && m.Error != nil }); got != 1 {
		t.Errorf("got %d error responses, want only the one for the request", got)
	}
}

// Catches a server that never lets the editor stop it: shutdown must answer
// with a null result (not omit it) and exit must end Serve cleanly.
func TestShutdownAndExit(t *testing.T) {
	p := newProject(t, "a.ts")
	c := startServer(t, serveConfig{census: p.noAnyCensus, findCfg: p.findTsConfig})
	c.handshake(nil, nil)

	c.request(5, "shutdown", nil)
	response := c.responseTo(5)
	if response.Error != nil {
		t.Fatalf("shutdown failed: %v", response.Error)
	}
	if string(response.Result) != "null" {
		t.Errorf("shutdown result = %q, want null", response.Result)
	}

	c.notify("exit", nil)
	select {
	case err := <-c.done:
		if err != nil {
			t.Errorf("Serve returned %v after exit, want nil", err)
		}
		c.done <- nil // the cleanup waits on this too
	case <-time.After(waitTimeout):
		t.Fatal("Serve did not return after exit")
	}
}

// Catches requests before initialize being served as if the handshake had
// happened.
func TestRequestBeforeInitialize(t *testing.T) {
	p := newProject(t, "a.ts")
	c := startServer(t, serveConfig{census: p.noAnyCensus, findCfg: p.findTsConfig})

	c.request(3, "textDocument/hover", map[string]any{})
	response := c.responseTo(3)
	if response.Error == nil || response.Error.Code != -32002 {
		t.Errorf("response = %+v, want error -32002 (server not initialized)", response.Error)
	}
}
