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

var capabilities = []string{"build", "project-selection", "shutdown", "terminal-cancel", "transformer-callback"}

// Diagnostic is the stable wire representation of one compiler diagnostic.
type Diagnostic struct {
	File     string `json:"file"`
	Line     int    `json:"line"`
	Col      int    `json:"col"`
	Code     string `json:"code,omitempty"`
	Severity string `json:"severity"`
	Message  string `json:"message"`
}

// BuildResult is the complete request response at the protocol seam.
type BuildResult struct {
	OK          bool            `json:"ok"`
	Files       int             `json:"files"`
	DurationMS  int64           `json:"durationMs"`
	Diagnostics []Diagnostic    `json:"diagnostics"`
	Outputs     []string        `json:"outputs"`
	Projects    []ProjectResult `json:"projects"`
	Telemetry   BuildTelemetry  `json:"telemetry"`
}

type BuildTelemetry struct {
	ScheduledProjects   int `json:"scheduledProjects"`
	SelectedProjects    int `json:"selectedProjects"`
	SatisfiedProjects   int `json:"satisfiedProjects"`
	TransformedProjects int `json:"transformedProjects"`
	EmittedProjects     int `json:"emittedProjects"`
}

type ProjectResult struct {
	Config      string                        `json:"config"`
	Status      compile.SolutionProjectStatus `json:"status"`
	Blockers    []string                      `json:"blockers"`
	Diagnostics []Diagnostic                  `json:"diagnostics"`
	Outputs     []string                      `json:"outputs"`
	OutputCount int                           `json:"outputCount"`
	Timings     ProjectTimings                `json:"timings"`
}

type ProjectTimings struct {
	DurationMS int64                     `json:"durationMs"`
	Stages     compile.BuildTimingStages `json:"stages"`
	Counts     compile.BuildTimingCounts `json:"counts"`
}

// BuildRequest is the canonical request passed to the native build adapter.
type BuildRequest struct {
	Roots     []string
	Selection *compile.SolutionProjectSelection
}

// BuildFunc performs one build. Build failures belong in BuildResult;
// returned errors are reserved for cancellation or server failures.
type BuildFunc func(ctx context.Context, request BuildRequest) (BuildResult, error)

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
	callbackMu   sync.Mutex
	callbackNext uint64
	callbacks    map[string]chan callbackResult
	callbackBase context.Context
}

// NewServer creates a native build server behind a small build function seam.
func NewServer(version string, build BuildFunc) *Server {
	server := &Server{version: version, build: build, callbacks: map[string]chan callbackResult{}}
	server.callbackBase = compile.WithTransformerCallback(context.Background(), server.callTransformer)
	return server
}

// Run serves one newline-framed JSON-RPC connection until shutdown or EOF.
func (s *Server) Run(in io.Reader, out io.Writer) error {
	s.out = out

	scanner := bufio.NewScanner(in)
	scanner.Buffer(make([]byte, 64*1024), 16*1024*1024)
	for scanner.Scan() {
		var message incomingMessage
		if err := json.Unmarshal(scanner.Bytes(), &message); err != nil {
			return fmt.Errorf("build API: malformed request: %w", err)
		}
		if message.JSONRPC != "2.0" {
			s.stopActiveBuild()
			s.builds.Wait()
			return errors.New(`build API: message jsonrpc must be "2.0"`)
		}
		if message.Method == "" {
			if err := s.completeCallback(message); err != nil {
				s.stopActiveBuild()
				s.builds.Wait()
				return err
			}
			continue
		}
		if len(message.ID) == 0 {
			return errors.New("build API: request is missing an id")
		}
		var id int64
		if err := json.Unmarshal(message.ID, &id); err != nil {
			return errors.New("build API: client request id must be an integer")
		}

		switch message.Method {
		case "initialize":
			s.initialize(id, message.Params)
		case "build":
			s.startBuild(id, message.Params)
		case "shutdown":
			s.stopActiveBuild()
			s.builds.Wait()
			s.respond(response{ID: id, Result: struct{}{}})
			return s.responseError()
		default:
			s.respondError(id, "METHOD_NOT_FOUND", fmt.Sprintf("unknown method %q", message.Method))
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
		Projects:    []ProjectResult{},
	}
	if result != nil {
		response.Outputs = make([]string, 0, len(result.Outputs))
		for output := range result.Outputs {
			response.Outputs = append(response.Outputs, projectOutputPath(projectDir, output))
		}
		sort.Strings(response.Outputs)
		if buildErr == nil {
			response.Files = len(result.Outputs)
		}
	}
	response.Diagnostics = diagnosticsFromCompile(projectDir, diagnostics)
	if buildErr != nil && len(response.Diagnostics) == 0 {
		response.Diagnostics = append(response.Diagnostics, Diagnostic{Severity: "error", Message: buildErr.Error()})
	}
	return response
}

// ResultFromSolution converts coordinator-owned outcomes without reconstructing
// diagnostic or output ownership from the aggregate result.
func ResultFromSolution(projects []compile.SolutionProjectResult, timings *compile.BuildTimings, elapsed time.Duration, buildErr error) BuildResult {
	response := BuildResult{
		OK:          buildErr == nil,
		DurationMS:  elapsed.Milliseconds(),
		Diagnostics: []Diagnostic{},
		Outputs:     []string{},
		Projects:    make([]ProjectResult, 0, len(projects)),
	}
	if timings != nil {
		response.Telemetry = BuildTelemetry{
			ScheduledProjects:   timings.Counts.ScheduledProjects,
			SelectedProjects:    timings.Counts.SelectedProjects,
			SatisfiedProjects:   timings.Counts.SatisfiedProjects,
			TransformedProjects: timings.Counts.TransformedProjects,
			EmittedProjects:     timings.Counts.EmittedProjects,
		}
	}
	for _, project := range projects {
		projectDir := filepath.Dir(project.ConfigPath)
		wire := ProjectResult{
			Config:      filepath.ToSlash(project.ConfigPath),
			Status:      project.Status,
			Blockers:    make([]string, len(project.Blockers)),
			Diagnostics: diagnosticsFromCompile(projectDir, project.Diagnostics),
			Outputs:     append([]string{}, project.Outputs...),
			OutputCount: project.OutputCount,
			Timings: ProjectTimings{
				DurationMS: project.Timings.BuildWallMs,
				Stages:     project.Timings.Stages,
				Counts:     project.Timings.Counts,
			},
		}
		for index, blocker := range project.Blockers {
			wire.Blockers[index] = filepath.ToSlash(blocker)
		}
		response.Projects = append(response.Projects, wire)
		response.Files += project.OutputCount
		for _, output := range project.Outputs {
			response.Outputs = append(response.Outputs, projectOutputPath(projectDir, output))
		}
		if project.Status == compile.SolutionProjectFailed {
			response.Diagnostics = append(response.Diagnostics, wire.Diagnostics...)
		}
	}
	sort.Strings(response.Outputs)
	if buildErr != nil {
		response.Files = 0
		if len(response.Diagnostics) == 0 {
			response.Diagnostics = append(response.Diagnostics, Diagnostic{Severity: "error", Message: buildErr.Error()})
		}
	}
	return response
}

func projectOutputPath(projectDir, output string) string {
	path := filepath.FromSlash(output)
	if !filepath.IsAbs(path) {
		path = filepath.Join(projectDir, path)
	}
	return filepath.ToSlash(path)
}

func diagnosticsFromCompile(projectDir string, diagnostics []compile.DiagnosticInfo) []Diagnostic {
	result := make([]Diagnostic, 0, len(diagnostics))
	for _, diagnostic := range diagnostics {
		severity := "error"
		if diagnostic.Warning {
			severity = "warning"
		}
		file := ""
		if diagnostic.FileName != "" {
			file = relativePath(projectDir, diagnostic.FileName)
		}
		result = append(result, Diagnostic{
			File: file, Line: diagnostic.Line, Col: diagnostic.Col, Code: diagnostic.Code,
			Severity: severity, Message: diagnostic.Message,
		})
	}
	return result
}

type incomingMessage struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
	Result  json.RawMessage `json:"result,omitempty"`
	Error   *responseError  `json:"error,omitempty"`
}

type callbackRequest struct {
	JSONRPC string                     `json:"jsonrpc"`
	ID      string                     `json:"id"`
	Method  string                     `json:"method"`
	Params  compile.TransformerRequest `json:"params"`
}

type callbackResult struct {
	result json.RawMessage
	err    error
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
	Project   string   `json:"project"`
	Roots     []string `json:"roots"`
	Selected  []string `json:"selected"`
	Satisfied []string `json:"satisfied"`
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
	if params.Project != "" && len(params.Roots) != 0 {
		s.respondError(id, "INVALID_REQUEST", "provide project or roots, not both")
		return
	}
	roots := params.Roots
	if params.Project != "" {
		roots = []string{params.Project}
	}
	if len(roots) == 0 {
		s.respondError(id, "INVALID_REQUEST", "roots must contain at least one path")
		return
	}
	for _, root := range roots {
		if root == "" {
			s.respondError(id, "INVALID_REQUEST", "roots must contain only non-empty paths")
			return
		}
	}
	if (params.Selected == nil) != (params.Satisfied == nil) {
		s.respondError(id, "INVALID_REQUEST", "provide selected and satisfied together, or omit both")
		return
	}
	var selection *compile.SolutionProjectSelection
	if params.Selected != nil {
		for _, config := range append(append([]string(nil), params.Selected...), params.Satisfied...) {
			if config == "" {
				s.respondError(id, "INVALID_REQUEST", "selected and satisfied must contain only non-empty paths")
				return
			}
		}
		selection = &compile.SolutionProjectSelection{Selected: params.Selected, Satisfied: params.Satisfied}
	}

	s.activeMu.Lock()
	if s.activeCancel != nil {
		s.activeMu.Unlock()
		s.respondError(id, "BUILD_IN_PROGRESS", "the session already has an active build")
		return
	}
	ctx, cancel := context.WithCancel(s.callbackBase)
	s.activeID = id
	s.activeCancel = cancel
	s.activeMu.Unlock()

	s.builds.Add(1)
	go func() {
		defer s.builds.Done()
		result, err := s.build(ctx, BuildRequest{Roots: roots, Selection: selection})

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

func (s *Server) callTransformer(ctx context.Context, request compile.TransformerRequest) (compile.TransformerResponse, error) {
	s.callbackMu.Lock()
	s.callbackNext++
	id := fmt.Sprintf("transform-%d", s.callbackNext)
	result := make(chan callbackResult, 1)
	s.callbacks[id] = result
	s.callbackMu.Unlock()

	requestBytes := s.respondCallback(callbackRequest{ID: id, Method: "transform", Params: request})
	if err := s.responseError(); err != nil {
		s.removeCallback(id)
		return compile.TransformerResponse{}, err
	}

	select {
	case completed := <-result:
		if completed.err != nil {
			return compile.TransformerResponse{}, completed.err
		}
		var response compile.TransformerResponse
		if err := json.Unmarshal(completed.result, &response); err != nil {
			return compile.TransformerResponse{}, fmt.Errorf("build API: invalid transformer callback response: %w", err)
		}
		response.Transport = &compile.TransformerTransportMetrics{
			RequestBytes:  requestBytes,
			ResponseBytes: int64(len(completed.result)),
		}
		return response, nil
	case <-ctx.Done():
		s.removeCallback(id)
		return compile.TransformerResponse{}, fmt.Errorf("build API: transformer callback cancelled: %w", ctx.Err())
	}
}

func (s *Server) completeCallback(message incomingMessage) error {
	var id string
	if err := json.Unmarshal(message.ID, &id); err != nil || id == "" {
		return errors.New("build API: callback response id must be a string")
	}
	s.callbackMu.Lock()
	pending := s.callbacks[id]
	delete(s.callbacks, id)
	s.callbackMu.Unlock()
	if pending == nil {
		return fmt.Errorf("build API: callback response has unknown id %q", id)
	}
	if message.Error != nil {
		pending <- callbackResult{err: fmt.Errorf("transformer callback %s: %s", message.Error.Code, message.Error.Message)}
		return nil
	}
	if len(message.Result) == 0 {
		pending <- callbackResult{err: errors.New("transformer callback response is missing a result")}
		return nil
	}
	pending <- callbackResult{result: message.Result}
	return nil
}

func (s *Server) removeCallback(id string) {
	s.callbackMu.Lock()
	delete(s.callbacks, id)
	s.callbackMu.Unlock()
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

func (s *Server) respondCallback(message callbackRequest) int64 {
	s.writeMu.Lock()
	defer s.writeMu.Unlock()
	message.JSONRPC = "2.0"
	payload, err := json.Marshal(message)
	if err == nil {
		_, err = s.out.Write(append(payload, '\n'))
	}
	if err != nil && s.writeErr == nil {
		s.writeErr = fmt.Errorf("build API: write callback: %w", err)
	}
	return int64(len(payload))
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
