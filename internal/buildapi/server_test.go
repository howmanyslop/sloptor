package buildapi

import (
	"bufio"
	"context"
	"encoding/json"
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
