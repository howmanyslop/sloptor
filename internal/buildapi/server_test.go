package buildapi

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"io"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"rotor/internal/compile"
)

func TestServerRoutesTransformerCallbacks(t *testing.T) {
	server := NewServer("2.7.0", func(ctx context.Context, request BuildRequest) (BuildResult, error) {
		callback := compile.TransformerCallbackFromContext(ctx)
		if callback == nil {
			t.Fatal("build context has no transformer callback")
		}
		response, err := callback(ctx, compile.TransformerRequest{Protocol: 1, Operation: "validate", ProjectDir: request.Roots[0], TsConfigPath: request.Roots[0] + "/tsconfig.json"})
		if err != nil {
			return BuildResult{}, err
		}
		if len(response.Diagnostics) != 1 || response.Diagnostics[0].Code != "fixture" {
			t.Fatalf("callback response = %#v", response)
		}
		if response.Transport == nil || response.Transport.RequestBytes == 0 || response.Transport.ResponseBytes == 0 {
			t.Fatalf("callback transport metrics = %#v, want measured request and response bytes", response.Transport)
		}
		return emptyBuildResult(), nil
	})

	inputReader, inputWriter := io.Pipe()
	outputReader, outputWriter := io.Pipe()
	done := make(chan error, 1)
	go func() { done <- server.Run(inputReader, outputWriter) }()
	t.Cleanup(func() {
		_ = inputWriter.Close()
		_ = outputReader.Close()
	})

	writeJSONLine(t, inputWriter, map[string]any{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": map[string]any{"protocolVersion": 1, "clientVersion": "2.7.0"}})
	reader := bufio.NewReader(outputReader)
	readJSONLine(t, reader)
	writeJSONLine(t, inputWriter, map[string]any{"jsonrpc": "2.0", "id": 2, "method": "build", "params": map[string]any{"roots": []string{"fixture"}}})

	callback := readJSONLine(t, reader)
	if callback["method"] != "transform" {
		t.Fatalf("callback method = %v", callback["method"])
	}
	callbackID, ok := callback["id"].(string)
	if !ok || callbackID == "" {
		t.Fatalf("callback id = %#v, want string", callback["id"])
	}
	writeJSONLine(t, inputWriter, map[string]any{
		"jsonrpc": "2.0",
		"id":      callbackID,
		"result": map[string]any{
			"diagnostics": []map[string]any{{"category": "error", "code": "fixture", "message": "fixture"}},
			"transformed": []any{},
		},
	})
	build := readJSONLine(t, reader)
	if build["id"] != float64(2) || build["error"] != nil {
		t.Fatalf("build response = %#v", build)
	}

	writeJSONLine(t, inputWriter, map[string]any{"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": map[string]any{}})
	readJSONLine(t, reader)
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("server did not stop")
	}
}

func TestServerEOFUnblocksTransformerCallback(t *testing.T) {
	callbackStarted := make(chan struct{})
	buildReturned := make(chan error, 1)
	server := NewServer("2.7.0", func(ctx context.Context, _ BuildRequest) (BuildResult, error) {
		close(callbackStarted)
		_, err := compile.TransformerCallbackFromContext(ctx)(ctx, compile.TransformerRequest{Protocol: 1, Operation: "validate", ProjectDir: "fixture", TsConfigPath: "fixture/tsconfig.json"})
		buildReturned <- err
		return BuildResult{}, err
	})
	inputReader, inputWriter := io.Pipe()
	var output strings.Builder
	done := make(chan error, 1)
	go func() { done <- server.Run(inputReader, &output) }()

	writeJSONLine(t, inputWriter, map[string]any{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": map[string]any{"protocolVersion": 1, "clientVersion": "2.7.0"}})
	writeJSONLine(t, inputWriter, map[string]any{"jsonrpc": "2.0", "id": 2, "method": "build", "params": map[string]any{"roots": []string{"fixture"}}})
	<-callbackStarted
	_ = inputWriter.Close()

	select {
	case err := <-buildReturned:
		if err == nil {
			t.Fatal("callback unexpectedly succeeded after EOF")
		}
	case <-time.After(2 * time.Second):
		t.Fatal("callback stayed blocked after EOF")
	}
	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("server stayed blocked after EOF")
	}
}

func TestServerRejectsInvalidJSONRPCRequests(t *testing.T) {
	for name, envelope := range map[string]string{
		"missing": `{"id":1,"method":"initialize","params":{}}` + "\n",
		"wrong":   `{"jsonrpc":"1.0","id":1,"method":"initialize","params":{}}` + "\n",
	} {
		t.Run(name, func(t *testing.T) {
			server := NewServer("2.7.0", func(context.Context, BuildRequest) (BuildResult, error) {
				t.Fatal("build must not run")
				return BuildResult{}, nil
			})
			err := server.Run(strings.NewReader(envelope), io.Discard)
			if err == nil || !strings.Contains(err.Error(), "jsonrpc") {
				t.Fatalf("Run() error = %v, want invalid jsonrpc error", err)
			}
		})
	}
}

func TestServerRejectsInvalidJSONRPCCallbackResponses(t *testing.T) {
	callbackStarted := make(chan struct{})
	server := NewServer("2.7.0", func(ctx context.Context, _ BuildRequest) (BuildResult, error) {
		close(callbackStarted)
		_, err := compile.TransformerCallbackFromContext(ctx)(ctx, compile.TransformerRequest{Protocol: 1, Operation: "validate"})
		return BuildResult{}, err
	})
	inputReader, inputWriter := io.Pipe()
	outputReader, outputWriter := io.Pipe()
	done := make(chan error, 1)
	go func() { done <- server.Run(inputReader, outputWriter) }()
	t.Cleanup(func() {
		_ = inputWriter.Close()
		_ = outputReader.Close()
	})

	writeJSONLine(t, inputWriter, map[string]any{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": map[string]any{"protocolVersion": 1, "clientVersion": "2.7.0"}})
	reader := bufio.NewReader(outputReader)
	readJSONLine(t, reader)
	writeJSONLine(t, inputWriter, map[string]any{"jsonrpc": "2.0", "id": 2, "method": "build", "params": map[string]any{"roots": []string{"fixture"}}})
	<-callbackStarted
	callback := readJSONLine(t, reader)
	writeJSONLine(t, inputWriter, map[string]any{"jsonrpc": "1.0", "id": callback["id"], "result": map[string]any{}})
	readJSONLine(t, reader)

	select {
	case err := <-done:
		if err == nil || !strings.Contains(err.Error(), "jsonrpc") {
			t.Fatalf("Run() error = %v, want invalid jsonrpc error", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("server did not reject invalid callback response")
	}
}

func TestServerDecodesCallbackResponsesLargerThan16MiB(t *testing.T) {
	// Given: a transformer callback whose response is larger than 16 MiB.
	transformed := strings.Repeat("x", 17*1024*1024)
	server := NewServer("2.7.0", func(ctx context.Context, _ BuildRequest) (BuildResult, error) {
		response, err := compile.TransformerCallbackFromContext(ctx)(ctx, compile.TransformerRequest{Protocol: 1, Operation: "transform"})
		if err != nil {
			return BuildResult{}, err
		}
		if len(response.Transformed) != 1 || response.Transformed[0].Text != transformed {
			t.Errorf("callback response lost the large transformed text")
		}
		return emptyBuildResult(), nil
	})
	session := startCallbackSession(t, server)

	// When: the client answers the callback with the large payload. The
	// write runs aside so a server that stops reading fails the test, not
	// hangs it.
	payload, err := json.Marshal(map[string]any{
		"jsonrpc": "2.0",
		"id":      session.callbackID,
		"result":  map[string]any{"diagnostics": []any{}, "transformed": []map[string]any{{"fileName": "main.ts", "text": transformed}}},
	})
	if err != nil {
		t.Fatal(err)
	}
	go func() { _, _ = session.input.Write(append(payload, '\n')) }()

	// Then: the server decodes it and completes the build.
	if build := readJSONLine(t, session.output); build["id"] != float64(2) || build["error"] != nil {
		t.Fatalf("build response = %#v", build)
	}
	writeJSONLine(t, session.input, map[string]any{"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": map[string]any{}})
	readJSONLine(t, session.output)
	if err := session.wait(t); err != nil {
		t.Fatal(err)
	}
}

func TestServerTransportFailureNamesCauseInBuildResponse(t *testing.T) {
	for name, fail := range map[string]func(*io.PipeWriter){
		"read error": func(input *io.PipeWriter) { _ = input.CloseWithError(errors.New("pipe broke")) },
		"malformed":  func(input *io.PipeWriter) { _, _ = input.Write([]byte("{not json\n")) },
	} {
		t.Run(name, func(t *testing.T) {
			// Given: a build waiting on a transformer callback.
			server := NewServer("2.7.0", func(ctx context.Context, _ BuildRequest) (BuildResult, error) {
				_, err := compile.TransformerCallbackFromContext(ctx)(ctx, compile.TransformerRequest{Protocol: 1, Operation: "transform"})
				return BuildResult{}, err
			})
			session := startCallbackSession(t, server)

			// When: the request stream fails before the callback completes.
			go fail(session.input)

			// Then: the build response names the transport failure, not a
			// client cancellation, and Run returns the same cause.
			build := readJSONLineWithin(t, session.output, 5*time.Second)
			buildError, _ := build["error"].(map[string]any)
			if buildError == nil || buildError["code"] == "BUILD_CANCELLED" {
				t.Fatalf("build response = %#v, want a transport failure", build)
			}
			message, _ := buildError["message"].(string)
			err := session.wait(t)
			if err == nil || !strings.Contains(message, err.Error()) {
				t.Fatalf("build error message = %q, Run() error = %v; want the Run error in the message", message, err)
			}
		})
	}
}

type callbackSession struct {
	input      *io.PipeWriter
	output     *bufio.Reader
	done       chan error
	callbackID string
}

// startCallbackSession initializes server, starts build 2, and reads the
// first transformer callback request.
func startCallbackSession(t *testing.T, server *Server) callbackSession {
	t.Helper()
	inputReader, inputWriter := io.Pipe()
	outputReader, outputWriter := io.Pipe()
	done := make(chan error, 1)
	go func() { done <- server.Run(inputReader, outputWriter) }()
	t.Cleanup(func() {
		_ = inputWriter.Close()
		_ = outputReader.Close()
	})

	output := bufio.NewReader(outputReader)
	writeJSONLine(t, inputWriter, map[string]any{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": map[string]any{"protocolVersion": 1, "clientVersion": "2.7.0"}})
	readJSONLine(t, output)
	writeJSONLine(t, inputWriter, map[string]any{"jsonrpc": "2.0", "id": 2, "method": "build", "params": map[string]any{"roots": []string{"fixture"}}})
	callback := readJSONLine(t, output)
	callbackID, ok := callback["id"].(string)
	if callback["method"] != "transform" || !ok {
		t.Fatalf("callback request = %#v", callback)
	}
	return callbackSession{input: inputWriter, output: output, done: done, callbackID: callbackID}
}

func (s callbackSession) wait(t *testing.T) error {
	t.Helper()
	select {
	case err := <-s.done:
		return err
	case <-time.After(5 * time.Second):
		t.Fatal("server did not stop")
		return nil
	}
}

func TestResultFromSolutionUsesProjectAnchoredAggregateOutputsForEveryBatchSize(t *testing.T) {
	root := t.TempDir()
	first := compile.SolutionProjectResult{
		ConfigPath:  filepath.Join(root, "first", "tsconfig.json"),
		Status:      compile.SolutionProjectSuccess,
		Outputs:     []string{"out/main.luau"},
		OutputCount: 1,
	}
	second := compile.SolutionProjectResult{
		ConfigPath:  filepath.Join(root, "second", "tsconfig.json"),
		Status:      compile.SolutionProjectSuccess,
		Outputs:     []string{"out/main.luau"},
		OutputCount: 1,
	}
	wantFirst := filepath.ToSlash(filepath.Join(root, "first", "out", "main.luau"))
	wantSecond := filepath.ToSlash(filepath.Join(root, "second", "out", "main.luau"))

	if got := ResultFromSolution([]compile.SolutionProjectResult{first}, nil, 0, nil).Outputs; !reflect.DeepEqual(got, []string{wantFirst}) {
		t.Fatalf("batch of one outputs = %v, want project-anchored %v", got, wantFirst)
	}
	if got := ResultFromSolution([]compile.SolutionProjectResult{first, second}, nil, 0, nil).Outputs; !reflect.DeepEqual(got, []string{wantFirst, wantSecond}) {
		t.Fatalf("batch of two outputs = %v, want project-anchored outputs", got)
	}
}

func emptyBuildResult() BuildResult {
	return BuildResult{Diagnostics: []Diagnostic{}, Outputs: []string{}, Projects: []ProjectResult{}}
}

func writeJSONLine(t *testing.T, writer io.Writer, value any) {
	t.Helper()
	if err := json.NewEncoder(writer).Encode(value); err != nil {
		t.Fatal(err)
	}
}

func readJSONLineWithin(t *testing.T, reader *bufio.Reader, timeout time.Duration) map[string]any {
	t.Helper()
	type read struct {
		line []byte
		err  error
	}
	result := make(chan read, 1)
	go func() {
		line, err := reader.ReadBytes('\n')
		result <- read{line, err}
	}()
	select {
	case got := <-result:
		if got.err != nil {
			t.Fatal(got.err)
		}
		var value map[string]any
		if err := json.Unmarshal(got.line, &value); err != nil {
			t.Fatal(err)
		}
		return value
	case <-time.After(timeout):
		t.Fatal("no response from server")
		return nil
	}
}

func readJSONLine(t *testing.T, reader *bufio.Reader) map[string]any {
	t.Helper()
	line, err := reader.ReadBytes('\n')
	if err != nil {
		t.Fatal(err)
	}
	var value map[string]any
	if err := json.Unmarshal(line, &value); err != nil {
		t.Fatal(err)
	}
	return value
}
