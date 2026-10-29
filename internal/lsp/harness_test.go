package lsp_test

import (
	"context"
	"encoding/json"
	"io"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"rotor/internal/compile"
	"rotor/internal/lsp"
	"rotor/tsgo/jsonrpc"
	"rotor/tsgo/ls/lsconv"
)

const (
	waitTimeout = 10 * time.Second
	// debounceForTests keeps edit-driven runs fast without changing which
	// runs coalesce: tests that need a run to overlap an edit hold the
	// census open instead of racing the clock.
	debounceForTests = 20 * time.Millisecond
)

// message is one server-to-client JSON-RPC message, decoded just far enough to
// match on. The wire form is the contract under test, so the client side of
// these tests deliberately does not use the server's own protocol types.
type message struct {
	ID     json.RawMessage `json:"id"`
	Method string          `json:"method"`
	Params json.RawMessage `json:"params"`
	Result json.RawMessage `json:"result"`
	Error  *struct {
		Code    int    `json:"code"`
		Message string `json:"message"`
	} `json:"error"`
}

type publishedDiagnostic struct {
	Range struct {
		Start struct{ Line, Character int }
		End   struct{ Line, Character int }
	}
	Severity int
	Code     string
	Source   string
	Message  string
}

type publishParams struct {
	URI         string
	Diagnostics []publishedDiagnostic
}

// client is a scripted editor on the other end of the server's streams.
type client struct {
	t       *testing.T
	toSrv   *io.PipeWriter
	done    chan error
	changed chan struct{}

	mu       sync.Mutex
	received []message
}

type serveConfig struct {
	census   func(ctx context.Context, tsConfigPath string, overlays map[string]string) (*compile.ProjectDiagnostics, error)
	findCfg  func(fileName string) (string, error)
	debounce time.Duration
}

// startServer runs Serve over in-memory pipes and returns the editor side.
func startServer(t *testing.T, cfg serveConfig) *client {
	t.Helper()
	srvIn, toSrv := io.Pipe()
	fromSrv, srvOut := io.Pipe()
	c := &client{t: t, toSrv: toSrv, done: make(chan error, 1), changed: make(chan struct{}, 1)}

	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		err := lsp.Serve(ctx, srvIn, srvOut, lsp.Options{
			FindTsConfig: cfg.findCfg,
			Census:       cfg.census,
			Debounce:     cfg.debounce,
		})
		_ = srvOut.Close()
		c.done <- err
	}()

	go func() {
		reader := jsonrpc.NewReader(fromSrv)
		for {
			data, err := reader.Read()
			if err != nil {
				return
			}
			var m message
			if err := json.Unmarshal(data, &m); err != nil {
				t.Errorf("server wrote a frame that is not JSON-RPC: %q", data)
				return
			}
			c.mu.Lock()
			c.received = append(c.received, m)
			c.mu.Unlock()
			select {
			case c.changed <- struct{}{}:
			default:
			}
		}
	}()

	t.Cleanup(func() {
		_ = toSrv.Close()
		select {
		case err := <-c.done:
			if err != nil {
				t.Errorf("Serve returned %v after the client closed its end", err)
			}
		case <-time.After(waitTimeout):
			cancel()
			t.Error("Serve did not return after its input closed")
		}
		cancel()
	})
	return c
}

func (c *client) write(body any) {
	c.t.Helper()
	body.(map[string]any)["jsonrpc"] = "2.0"
	data, err := json.Marshal(body)
	if err != nil {
		c.t.Fatal(err)
	}
	if err := jsonrpc.NewWriter(c.toSrv).Write(data); err != nil {
		c.t.Fatalf("write to server: %v", err)
	}
}

func (c *client) request(id int, method string, params any) {
	c.t.Helper()
	c.write(map[string]any{"id": id, "method": method, "params": params})
}

func (c *client) notify(method string, params any) {
	c.t.Helper()
	c.write(map[string]any{"method": method, "params": params})
}

// responseTo waits for the response to request id.
func (c *client) responseTo(id int) message {
	c.t.Helper()
	want := strconv.Itoa(id)
	return c.waitFor(1, func(m message) bool {
		return m.Method == "" && string(m.ID) == want
	})
}

// waitFor blocks until at least n received messages match, and returns the nth.
func (c *client) waitFor(n int, match func(message) bool) message {
	c.t.Helper()
	deadline := time.After(waitTimeout)
	for {
		c.mu.Lock()
		seen := 0
		for _, m := range c.received {
			if match(m) {
				seen++
				if seen == n {
					c.mu.Unlock()
					return m
				}
			}
		}
		c.mu.Unlock()
		select {
		case <-c.changed:
		case <-deadline:
			c.t.Fatalf("timed out waiting for message %d matching the predicate", n)
		}
	}
}

func (c *client) count(match func(message) bool) int {
	c.mu.Lock()
	defer c.mu.Unlock()
	n := 0
	for _, m := range c.received {
		if match(m) {
			n++
		}
	}
	return n
}

func publishFor(uri string) func(message) bool {
	return func(m message) bool {
		if m.Method != "textDocument/publishDiagnostics" {
			return false
		}
		var p publishParams
		return json.Unmarshal(m.Params, &p) == nil && p.URI == uri
	}
}

func decodePublish(t *testing.T, m message) publishParams {
	t.Helper()
	var p publishParams
	if err := json.Unmarshal(m.Params, &p); err != nil {
		t.Fatal(err)
	}
	return p
}

func isShowMessage(m message) bool { return m.Method == "window/showMessage" }

// handshake performs initialize/initialized and returns the initialize result.
func (c *client) handshake(positionEncodings []string, initOptions map[string]any) map[string]any {
	c.t.Helper()
	general := map[string]any{}
	if positionEncodings != nil {
		general["positionEncodings"] = positionEncodings
	}
	params := map[string]any{
		"processId":    nil,
		"rootUri":      nil,
		"capabilities": map[string]any{"general": general},
	}
	if initOptions != nil {
		params["initializationOptions"] = initOptions
	}
	c.request(1, "initialize", params)
	response := c.responseTo(1)
	if response.Error != nil {
		c.t.Fatalf("initialize failed: %s", response.Error.Message)
	}
	c.notify("initialized", map[string]any{})
	var result map[string]any
	if err := json.Unmarshal(response.Result, &result); err != nil {
		c.t.Fatal(err)
	}
	return result
}

func (c *client) didOpen(path, text string) {
	c.t.Helper()
	c.notify("textDocument/didOpen", map[string]any{"textDocument": map[string]any{
		"uri": uriOf(path), "languageId": "typescript", "version": 1, "text": text,
	}})
}

func (c *client) didChange(path string, version int, text string) {
	c.t.Helper()
	c.notify("textDocument/didChange", map[string]any{
		"textDocument":   map[string]any{"uri": uriOf(path), "version": version},
		"contentChanges": []map[string]any{{"text": text}},
	})
}

func (c *client) didSave(path string) {
	c.t.Helper()
	c.notify("textDocument/didSave", map[string]any{"textDocument": map[string]any{"uri": uriOf(path)}})
}

// ping round-trips an unknown request. Because the server handles messages in
// order, its answer proves every earlier message has been processed.
func (c *client) ping(id int) {
	c.t.Helper()
	c.request(id, "sloptor/ping", map[string]any{})
	c.responseTo(id)
}

func uriOf(path string) string { return string(lsconv.FileNameToDocumentURI(filepath.ToSlash(path))) }

// project is a throwaway project layout. The files need not exist on disk: the
// editor's buffers are what get compiled.
type project struct {
	config string
	files  []string
}

func newProject(t *testing.T, names ...string) project {
	t.Helper()
	root := t.TempDir()
	p := project{config: filepath.Join(root, "tsconfig.json")}
	for _, name := range names {
		p.files = append(p.files, filepath.Join(root, "src", name))
	}
	return p
}

func (p project) findTsConfig(string) (string, error) { return p.config, nil }

// noAnyCensus stands in for the compiler: it rejects the word "any" in each
// project file's buffer the way the real transformer rejects the type, at that
// word's byte offset. It reads the buffers from the overlays, so a result only
// reflects what the editor actually sent.
func (p project) noAnyCensus(_ context.Context, _ string, overlays map[string]string) (*compile.ProjectDiagnostics, error) {
	result := &compile.ProjectDiagnostics{ConfigPath: filepath.ToSlash(p.config)}
	for _, file := range p.files {
		entry := compile.FileDiagnostics{FileName: filepath.ToSlash(file), Outcome: compile.FileOutcomeOK, Transformed: true}
		text := overlays[file]
		if offset := strings.Index(text, "any"); offset >= 0 {
			entry.Outcome = compile.FileOutcomeTransformerDiagnostic
			entry.Diagnostics = []compile.DiagnosticInfo{{
				Code:     "noAny",
				Message:  "Using values of type `any` is not supported!",
				FileName: entry.FileName,
				Offset:   offset,
				Len:      len("any"),
			}}
		}
		result.Files = append(result.Files, entry)
	}
	return result, nil
}
