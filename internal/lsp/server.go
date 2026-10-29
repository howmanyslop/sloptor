// Package lsp is sloptor's Language Server: it publishes the diagnostics only
// sloptor can produce (transformer rejections, comment-directive refusals,
// internal compiler errors) into an editor over stdio.
//
// The editor's own TypeScript server already reports TypeScript errors, so by
// default this server stays quiet about those and adds nothing but what the
// editor cannot show. The compile step is injected through Options so the
// package owns the protocol, scheduling and position mapping and nothing else.
package lsp

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/url"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"rotor/internal/compile"
	"rotor/tsgo/json"
	"rotor/tsgo/jsonrpc"
	"rotor/tsgo/lsp/lsproto"
)

// serverName is what the server reports as serverInfo.name and as the source of
// every diagnostic it publishes.
const serverName = "sloptor"

// defaultDebounce is how long a burst of edits must go quiet before a census
// runs. A census is a full cold compile, so running one per keystroke would
// pin a core while the user types.
const defaultDebounce = 250 * time.Millisecond

// Options configures Serve. The compile dependencies are injected so this
// package never has to know how a project's options are resolved.
type Options struct {
	// FindTsConfig maps an absolute file path (OS separators) to the tsconfig that owns it.
	FindTsConfig func(fileName string) (string, error)
	// Census compiles the project at tsConfigPath with the given overlays (absolute path -> buffer text)
	// and returns the per-file census. Overlays naming files outside the program are ignored by the callee.
	// The returned *compile.ProjectDiagnostics may be non-nil alongside a non-nil error.
	Census func(ctx context.Context, tsConfigPath string, overlays map[string]string) (*compile.ProjectDiagnostics, error)
	// Debounce is the delay after didChange before re-running a census. Zero means 250ms.
	Debounce time.Duration
}

// server is the state of one Serve call.
//
// The message loop is a single goroutine, but censuses run on their own
// goroutines (one per tsconfig at a time), so everything they share with the
// loop is guarded by mu and every write to the client goes through send.
type server struct {
	opts   Options
	ctx    context.Context
	cancel context.CancelFunc
	sched  *scheduler

	outMu sync.Mutex
	out   *jsonrpc.Writer

	// failure is the first error that should end the session, such as the
	// client's pipe closing under a write.
	failure   error
	failureMu sync.Mutex

	// Set by initialize, before any census is scheduled.
	initialized bool
	shutdown    bool
	encoding    lsproto.PositionEncodingKind
	typeErrors  bool

	// mu guards the fields below.
	mu sync.Mutex
	// open holds the text of every open source buffer, keyed by absolute path
	// with OS separators (the form Census overlays use).
	open map[string]string
	// configs remembers which tsconfig owns each open buffer, resolved once at
	// didOpen so an edit never has to walk the file system.
	configs map[string]string
	// published is, per tsconfig, the URIs last published with a non-empty
	// list, so they can be cleared when they go clean.
	published map[string]map[lsproto.DocumentUri]struct{}
	// shown is, per tsconfig, the window messages currently on screen, so a
	// problem that persists across censuses is announced once.
	shown map[string]map[string]struct{}
}

// Serve runs the server until the client sends exit, the input stream ends, or
// ctx is done. It returns nil for exit and end of input, ctx's error when ctx
// ends the session, and the underlying error when the transport breaks.
//
// Nothing but LSP messages is ever written to out; the caller must keep its own
// logging off that stream. Serve does not return while a census it started is
// still running, but a goroutine reading in may stay blocked until the client
// closes its end of the stream.
func Serve(ctx context.Context, in io.Reader, out io.Writer, opts Options) error {
	if opts.FindTsConfig == nil || opts.Census == nil {
		return errors.New("lsp: Options.FindTsConfig and Options.Census are required")
	}
	if opts.Debounce <= 0 {
		opts.Debounce = defaultDebounce
	}

	ctx, cancel := context.WithCancel(ctx)
	s := &server{
		opts:      opts,
		ctx:       ctx,
		cancel:    cancel,
		out:       jsonrpc.NewWriter(out),
		encoding:  lsproto.PositionEncodingKindUTF16,
		open:      map[string]string{},
		configs:   map[string]string{},
		published: map[string]map[lsproto.DocumentUri]struct{}{},
		shown:     map[string]map[string]struct{}{},
	}
	s.sched = newScheduler(opts.Debounce, s.runCensus)

	err := s.loop(jsonrpc.NewReader(in))

	// Cancel first so an in-flight census returns promptly, then wait for it so
	// nothing writes to out after Serve has returned.
	cancel()
	s.sched.close()
	if failure := s.failed(); failure != nil {
		return failure
	}
	return err
}

type frame struct {
	data []byte
	err  error
}

// loop reads and dispatches messages until the session ends. Reading happens on
// a helper goroutine so a cancelled context ends the session even while the
// client is silent.
func (s *server) loop(reader *jsonrpc.Reader) error {
	frames := make(chan frame)
	go func() {
		for {
			data, err := reader.Read()
			select {
			case frames <- frame{data, err}:
			case <-s.ctx.Done():
				return
			}
			if err != nil {
				return
			}
		}
	}()

	for {
		select {
		case <-s.ctx.Done():
			return s.ctx.Err()
		case f := <-frames:
			if f.err != nil {
				if errors.Is(f.err, io.EOF) {
					return nil
				}
				return f.err
			}
			if exit := s.handle(f.data); exit {
				return nil
			}
		}
	}
}

// handle processes one message and reports whether it was exit.
func (s *server) handle(data []byte) (exit bool) {
	var raw jsonrpc.Message
	if err := json.Unmarshal(data, &raw); err != nil {
		s.respondError(nil, jsonrpc.CodeInvalidRequest, "invalid request: "+err.Error())
		return false
	}
	if raw.IsResponse() {
		// The server sends no requests, so there is nothing to correlate.
		return false
	}

	if raw.IsRequest() && !s.initialized && raw.Method != string(lsproto.MethodInitialize) {
		s.respondError(raw.ID, int32(lsproto.ErrorCodeServerNotInitialized), "server not initialized")
		return false
	}

	var msg lsproto.Message
	if decodeErr := json.Unmarshal(data, &msg); decodeErr != nil {
		switch {
		case raw.IsRequest() && (raw.Method == string(lsproto.MethodInitialize) || raw.Method == string(lsproto.MethodShutdown)):
			// Initialize carries a capability block that may hold shapes the
			// generated types reject, and some clients send `params: null`
			// with shutdown. Refusing either would lose the whole session, so
			// both proceed without typed params.
			s.handleRequest(&lsproto.RequestMessage{ID: raw.ID, Method: lsproto.Method(raw.Method)}, raw.Params)
			return false
		case raw.IsRequest():
			s.respondError(raw.ID, jsonrpc.CodeInvalidParams, decodeErr.Error())
		}
		return raw.Method == string(lsproto.MethodExit)
	}
	request := msg.AsRequest()

	if raw.IsNotification() {
		return s.handleNotification(request)
	}
	s.handleRequest(request, raw.Params)
	return false
}

func (s *server) handleRequest(request *lsproto.RequestMessage, rawParams json.Value) {
	switch {
	case request.Method == lsproto.MethodInitialize:
		s.handleInitialize(request, rawParams)
	case !s.initialized:
		s.respondError(request.ID, int32(lsproto.ErrorCodeServerNotInitialized), "server not initialized")
	case s.shutdown:
		s.respondError(request.ID, jsonrpc.CodeInvalidRequest, "server is shutting down")
	case request.Method == lsproto.MethodShutdown:
		s.shutdown = true
		s.respond(request.ID, json.Value("null"))
	default:
		s.respondError(request.ID, jsonrpc.CodeMethodNotFound, "method not found: "+string(request.Method))
	}
}

func (s *server) handleNotification(request *lsproto.RequestMessage) (exit bool) {
	if request.Method == lsproto.MethodExit {
		return true
	}
	if !s.initialized {
		return false
	}
	switch request.Method {
	case lsproto.MethodTextDocumentDidOpen:
		if params, ok := request.Params.(*lsproto.DidOpenTextDocumentParams); ok && params.TextDocument != nil {
			s.didOpen(params.TextDocument)
		}
	case lsproto.MethodTextDocumentDidChange:
		if params, ok := request.Params.(*lsproto.DidChangeTextDocumentParams); ok {
			s.didChange(params)
		}
	case lsproto.MethodTextDocumentDidSave:
		if params, ok := request.Params.(*lsproto.DidSaveTextDocumentParams); ok {
			s.didSave(params)
		}
	case lsproto.MethodTextDocumentDidClose:
		if params, ok := request.Params.(*lsproto.DidCloseTextDocumentParams); ok {
			s.didClose(params.TextDocument.Uri)
		}
	case lsproto.MethodCancelRequest:
		// Every request is answered inline, so by the time a cancel arrives
		// there is nothing left to cancel.
	default:
		// Unknown notifications are ignored, as the protocol requires.
	}
	return false
}

// handleInitialize negotiates the position encoding and reads the one
// sloptor-specific option. The generated InitializationOptions type does not
// know about typeErrors, so that field is read from the raw params.
func (s *server) handleInitialize(request *lsproto.RequestMessage, rawParams json.Value) {
	if s.initialized {
		s.respondError(request.ID, jsonrpc.CodeInvalidRequest, "server already initialized")
		return
	}
	params, _ := request.Params.(*lsproto.InitializeParams)
	if params != nil && params.Capabilities != nil && params.Capabilities.General != nil &&
		params.Capabilities.General.PositionEncodings != nil {
		for _, encoding := range *params.Capabilities.General.PositionEncodings {
			if encoding == lsproto.PositionEncodingKindUTF8 {
				s.encoding = lsproto.PositionEncodingKindUTF8
			}
		}
	}
	var extra struct {
		InitializationOptions struct {
			TypeErrors bool `json:"typeErrors"`
		} `json:"initializationOptions"`
	}
	// Options of the wrong shape leave the default (off) in place.
	if err := json.Unmarshal(rawParams, &extra); err == nil {
		s.typeErrors = extra.InitializationOptions.TypeErrors
	}
	s.initialized = true

	openClose, changeKind := true, lsproto.TextDocumentSyncKindFull
	save := true
	encoding := s.encoding
	result := &lsproto.InitializeResult{
		Capabilities: &lsproto.ServerCapabilities{
			PositionEncoding: &encoding,
			TextDocumentSync: &lsproto.TextDocumentSyncOptionsOrKind{
				Options: &lsproto.TextDocumentSyncOptions{
					OpenClose: &openClose,
					Change:    &changeKind,
					Save:      &lsproto.BooleanOrSaveOptions{Boolean: &save},
				},
			},
		},
		ServerInfo: &lsproto.ServerInfo{Name: serverName},
	}
	s.respond(request.ID, result)
}

// didOpen records the buffer and runs a census right away: opening a file is
// the moment the user is looking at it and expects its diagnostics.
func (s *server) didOpen(item *lsproto.TextDocumentItem) {
	path, ok := sourcePath(item.Uri)
	if !ok {
		return
	}
	config, err := s.opts.FindTsConfig(path)
	if err != nil {
		// A file outside any project has nothing for sloptor to say.
		return
	}
	s.mu.Lock()
	s.open[path] = item.Text
	s.configs[path] = config
	s.mu.Unlock()
	s.sched.schedule(config, true)
}

// didChange replaces the buffer and schedules a debounced census. The server
// advertises full sync, so the last whole-document change is the new text.
func (s *server) didChange(params *lsproto.DidChangeTextDocumentParams) {
	path, ok := sourcePath(params.TextDocument.Uri)
	if !ok {
		return
	}
	text, found := "", false
	for _, change := range params.ContentChanges {
		if change.WholeDocument != nil {
			text, found = change.WholeDocument.Text, true
		}
	}
	if !found {
		return
	}
	s.mu.Lock()
	config, known := s.configs[path]
	if known {
		s.open[path] = text
	}
	s.mu.Unlock()
	if known {
		s.sched.schedule(config, false)
	}
}

// didSave runs a census right away. Editors that include the text on save
// refresh the buffer from it.
func (s *server) didSave(params *lsproto.DidSaveTextDocumentParams) {
	path, ok := sourcePath(params.TextDocument.Uri)
	if !ok {
		return
	}
	s.mu.Lock()
	config, known := s.configs[path]
	if known && params.Text != nil {
		s.open[path] = *params.Text
	}
	s.mu.Unlock()
	if known {
		s.sched.schedule(config, true)
	}
}

// didClose drops the buffer and re-runs the census so the project reflects what
// is on disk again, in case the user closed the file without saving.
func (s *server) didClose(uri lsproto.DocumentUri) {
	path, ok := sourcePath(uri)
	if !ok {
		return
	}
	s.mu.Lock()
	config, known := s.configs[path]
	delete(s.open, path)
	delete(s.configs, path)
	s.mu.Unlock()
	if known {
		s.sched.schedule(config, true)
	}
}

// snapshotOverlays copies every open buffer. The census needs a stable view
// while the client keeps editing, and publishing maps offsets through the same
// copy so ranges always match the text the offsets were computed against.
func (s *server) snapshotOverlays() map[string]string {
	s.mu.Lock()
	defer s.mu.Unlock()
	overlays := make(map[string]string, len(s.open))
	for path, text := range s.open {
		overlays[path] = text
	}
	return overlays
}

// runCensus is the scheduler's unit of work: one census of one tsconfig, then
// its publication.
func (s *server) runCensus(config string) {
	overlays := s.snapshotOverlays()
	result, err := s.census(config, overlays)
	if s.ctx.Err() != nil {
		return
	}
	s.publish(config, overlays, result, err)
}

// census calls the injected compile step. A panic there must not take down a
// long-lived editor process, so it becomes an ordinary census error.
func (s *server) census(config string, overlays map[string]string) (result *compile.ProjectDiagnostics, err error) {
	defer func() {
		if recovered := recover(); recovered != nil {
			err = fmt.Errorf("sloptor: census panicked: %v", recovered)
		}
	}()
	return s.opts.Census(s.ctx, config, overlays)
}

// sourcePath turns a document URI into the absolute path used for overlays and
// project lookup. Only file URIs naming TypeScript sources qualify; anything
// else (untitled buffers, package.json, Luau) is not sloptor's concern.
func sourcePath(uri lsproto.DocumentUri) (path string, ok bool) {
	if !strings.HasPrefix(string(uri), "file://") {
		return "", false
	}
	if _, err := url.Parse(string(uri)); err != nil {
		return "", false
	}
	switch strings.ToLower(filepath.Ext(string(uri))) {
	case ".ts", ".tsx", ".mts", ".cts":
	default:
		return "", false
	}
	return filepath.FromSlash(uri.FileName()), true
}

// response is the wire form of a JSON-RPC response. Result is raw JSON so that
// a null result (shutdown) is still written; the generated response type omits
// zero results, which would produce a response with neither result nor error.
type response struct {
	JSONRPC jsonrpc.JSONRPCVersion `json:"jsonrpc"`
	ID      *jsonrpc.ID            `json:"id"`
	Result  json.Value             `json:"result,omitzero"`
	Error   *jsonrpc.ResponseError `json:"error,omitzero"`
}

func (s *server) respond(id *jsonrpc.ID, result any) {
	raw, ok := result.(json.Value)
	if !ok {
		var err error
		raw, err = json.Marshal(result)
		if err != nil {
			s.respondError(id, jsonrpc.CodeInternalError, err.Error())
			return
		}
	}
	s.send(&response{ID: id, Result: raw})
}

func (s *server) respondError(id *jsonrpc.ID, code int32, message string) {
	s.send(&response{ID: id, Error: &jsonrpc.ResponseError{Code: code, Message: message}})
}

func (s *server) notify(message *lsproto.RequestMessage) {
	s.send(message)
}

// send writes one framed message. Writes are serialized because censuses
// publish from their own goroutines. A failed write means the client is gone,
// so it ends the session.
func (s *server) send(message any) {
	data, err := json.Marshal(message)
	if err != nil {
		s.fail(fmt.Errorf("lsp: encode message: %w", err))
		return
	}
	s.outMu.Lock()
	err = s.out.Write(data)
	s.outMu.Unlock()
	if err != nil {
		s.fail(fmt.Errorf("lsp: write message: %w", err))
	}
}

func (s *server) fail(err error) {
	s.failureMu.Lock()
	if s.failure == nil {
		s.failure = err
	}
	s.failureMu.Unlock()
	s.cancel()
}

func (s *server) failed() error {
	s.failureMu.Lock()
	defer s.failureMu.Unlock()
	return s.failure
}
