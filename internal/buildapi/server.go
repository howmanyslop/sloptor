// Package buildapi owns the versioned build protocol and native session
// lifecycle. Callers provide one build function; the package hides framing,
// handshake state, request correlation, cancellation, and serialization.
package buildapi

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"path/filepath"
	"sort"
	"sync"
	"time"

	"rotor/internal/compile"
)

const ProtocolVersion = 1

var capabilities = []string{"build", "shutdown", "terminal-cancel"}

// Diagnostic is the stable wire representation of one compiler diagnostic.
type Diagnostic struct {
	File     string `json:"file"`
	Line     int    `json:"line"`
	Col      int    `json:"col"`
	Code     string `json:"code,omitempty"`
	Severity string `json:"severity"`
	Message  string `json:"message"`
}

// BuildResult is the complete single-project response at the protocol seam.
type BuildResult struct {
	OK          bool         `json:"ok"`
	Files       int          `json:"files"`
	DurationMS  int64        `json:"durationMs"`
	Diagnostics []Diagnostic `json:"diagnostics"`
	Outputs     []string     `json:"outputs"`
}

// BuildFunc performs one build. Build failures belong in BuildResult;
// returned errors are reserved for cancellation or server failures.
type BuildFunc func(ctx context.Context, project string) (BuildResult, error)

// Server owns one initialized connection and at most one active build.
type Server struct {
	version string
	build   BuildFunc

	out          io.Writer
	writeMu      sync.Mutex
	writeErr     error
	activeMu     sync.Mutex
	activeID     int64
	activeCancel context.CancelFunc
	builds       sync.WaitGroup
	initialized  bool
}

// NewServer creates a native build server behind a small build function seam.
func NewServer(version string, build BuildFunc) *Server {
	return &Server{version: version, build: build}
}

// Run serves one newline-framed JSON-RPC connection until shutdown or EOF.
func (s *Server) Run(in io.Reader, out io.Writer) error {
	s.out = out

	scanner := bufio.NewScanner(in)
	scanner.Buffer(make([]byte, 64*1024), 16*1024*1024)
	for scanner.Scan() {
		var request request
		if err := json.Unmarshal(scanner.Bytes(), &request); err != nil {
			return fmt.Errorf("build API: malformed request: %w", err)
		}
		if request.ID == nil {
			return errors.New("build API: request is missing an id")
		}

		switch request.Method {
		case "initialize":
			s.initialize(*request.ID, request.Params)
		case "build":
			s.startBuild(*request.ID, request.Params)
		case "shutdown":
			s.stopActiveBuild()
			s.builds.Wait()
			s.respond(response{ID: *request.ID, Result: struct{}{}})
			return s.responseError()
		default:
			s.respondError(*request.ID, "METHOD_NOT_FOUND", fmt.Sprintf("unknown method %q", request.Method))
		}
		if err := s.responseError(); err != nil {
			s.stopActiveBuild()
			s.builds.Wait()
			return err
		}
	}

	s.stopActiveBuild()
	s.builds.Wait()
	if err := scanner.Err(); err != nil {
		return fmt.Errorf("build API: read request: %w", err)
	}
	return s.responseError()
}

// ResultFromCompile converts compiler-owned data to the stable protocol shape.
func ResultFromCompile(projectDir string, result *compile.BuildResult, diagnostics []compile.DiagnosticInfo, elapsed time.Duration, buildErr error) BuildResult {
	response := BuildResult{
		OK:          buildErr == nil,
		DurationMS:  elapsed.Milliseconds(),
		Diagnostics: []Diagnostic{},
		Outputs:     []string{},
	}
	if result != nil {
		response.Outputs = make([]string, 0, len(result.Outputs))
		for output := range result.Outputs {
			response.Outputs = append(response.Outputs, output)
		}
		sort.Strings(response.Outputs)
		if buildErr == nil {
			response.Files = len(result.Outputs)
		}
	}
	for _, diagnostic := range diagnostics {
		severity := "error"
		if diagnostic.Warning {
			severity = "warning"
		}
		file := ""
		if diagnostic.FileName != "" {
			file = relativePath(projectDir, diagnostic.FileName)
		}
		response.Diagnostics = append(response.Diagnostics, Diagnostic{
			File: file, Line: diagnostic.Line, Col: diagnostic.Col, Code: diagnostic.Code,
			Severity: severity, Message: diagnostic.Message,
		})
	}
	if buildErr != nil && len(response.Diagnostics) == 0 {
		response.Diagnostics = append(response.Diagnostics, Diagnostic{Severity: "error", Message: buildErr.Error()})
	}
	return response
}

type request struct {
	ID     *int64          `json:"id,omitempty"`
	Method string          `json:"method"`
	Params json.RawMessage `json:"params,omitempty"`
}

type response struct {
	JSONRPC string         `json:"jsonrpc"`
	ID      int64          `json:"id"`
	Result  any            `json:"result,omitempty"`
	Error   *responseError `json:"error,omitempty"`
}

type responseError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

type initializeParams struct {
	ProtocolVersion int    `json:"protocolVersion"`
	ClientVersion   string `json:"clientVersion"`
}

type initializeResult struct {
	ProtocolVersion int      `json:"protocolVersion"`
	ServerVersion   string   `json:"serverVersion"`
	Capabilities    []string `json:"capabilities"`
}

type buildParams struct {
	Project string `json:"project"`
}

func (s *Server) initialize(id int64, raw json.RawMessage) {
	if s.initialized {
		s.respondError(id, "ALREADY_INITIALIZED", "the session is already initialized")
		return
	}
	var params initializeParams
	if err := decodeParams(raw, &params); err != nil {
		s.respondError(id, "INVALID_REQUEST", err.Error())
		return
	}
	if params.ProtocolVersion != ProtocolVersion {
		s.respondError(id, "PROTOCOL_VERSION_MISMATCH", fmt.Sprintf("client protocol %d is incompatible with server protocol %d", params.ProtocolVersion, ProtocolVersion))
		return
	}
	if params.ClientVersion != s.version {
		s.respondError(id, "VERSION_MISMATCH", fmt.Sprintf("client version %s does not match server version %s", params.ClientVersion, s.version))
		return
	}
	s.initialized = true
	s.respond(response{ID: id, Result: initializeResult{
		ProtocolVersion: ProtocolVersion,
		ServerVersion:   s.version,
		Capabilities:    capabilities,
	}})
}

func (s *Server) startBuild(id int64, raw json.RawMessage) {
	if !s.initialized {
		s.respondError(id, "NOT_INITIALIZED", "initialize must complete before build")
		return
	}
	var params buildParams
	if err := decodeParams(raw, &params); err != nil {
		s.respondError(id, "INVALID_REQUEST", err.Error())
		return
	}
	if params.Project == "" {
		s.respondError(id, "INVALID_REQUEST", "project must be a non-empty path")
		return
	}

	s.activeMu.Lock()
	if s.activeCancel != nil {
		s.activeMu.Unlock()
		s.respondError(id, "BUILD_IN_PROGRESS", "the session already has an active build")
		return
	}
	ctx, cancel := context.WithCancel(context.Background())
	s.activeID = id
	s.activeCancel = cancel
	s.activeMu.Unlock()

	s.builds.Add(1)
	go func() {
		defer s.builds.Done()
		result, err := s.build(ctx, params.Project)

		s.activeMu.Lock()
		if s.activeID == id {
			s.activeID = 0
			s.activeCancel = nil
		}
		s.activeMu.Unlock()
		cancel()

		if errors.Is(err, context.Canceled) {
			s.respondError(id, "BUILD_CANCELLED", "the build was cancelled")
			return
		}
		if err != nil {
			s.respondError(id, "BUILD_FAILED", err.Error())
			return
		}
		s.respond(response{ID: id, Result: result})
	}()
}

func (s *Server) stopActiveBuild() {
	s.activeMu.Lock()
	defer s.activeMu.Unlock()
	if s.activeCancel != nil {
		s.activeCancel()
	}
}

func (s *Server) respondError(id int64, code, message string) {
	s.respond(response{ID: id, Error: &responseError{Code: code, Message: message}})
}

func (s *Server) respond(message response) {
	s.writeMu.Lock()
	defer s.writeMu.Unlock()
	message.JSONRPC = "2.0"
	if err := json.NewEncoder(s.out).Encode(message); err != nil && s.writeErr == nil {
		s.writeErr = fmt.Errorf("build API: write response: %w", err)
	}
}

func (s *Server) responseError() error {
	s.writeMu.Lock()
	defer s.writeMu.Unlock()
	return s.writeErr
}

func decodeParams(raw json.RawMessage, target any) error {
	if len(raw) == 0 {
		return errors.New("params are required")
	}
	if err := json.Unmarshal(raw, target); err != nil {
		return fmt.Errorf("invalid params: %w", err)
	}
	return nil
}

func relativePath(projectDir, fileName string) string {
	relative, err := filepath.Rel(projectDir, fileName)
	if err != nil {
		return filepath.ToSlash(fileName)
	}
	return filepath.ToSlash(relative)
}
