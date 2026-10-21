package compile

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"runtime/pprof"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"rotor/internal/config"
	"rotor/internal/logservice"
)

func TestParallelSplitPreservesRequest(t *testing.T) {
	request := sidecarRequest{
		Protocol: 1, Operation: "transform", ProjectDir: "project", TsConfigPath: "config",
		CompileFileNames: []string{"a", "b", "c", "d", "e"},
		RootFileNames:    []string{"all", "roots"}, ChangedFiles: []sidecarChangedFile{{FileName: "c", Text: "overlay"}},
		Plugins: []json.RawMessage{json.RawMessage(`{"transform":"example","silent":true}`)},
	}
	workers := splitParallelRequest(request)
	if !reflect.DeepEqual(workers[0].CompileFileNames, []string{"a", "b", "c"}) ||
		!reflect.DeepEqual(workers[1].CompileFileNames, []string{"d", "e"}) {
		t.Fatal(workers)
	}
	for _, worker := range workers {
		worker.CompileFileNames = request.CompileFileNames
		worker.ParallelWorker = false
		if !reflect.DeepEqual(worker, request) {
			t.Fatalf("request changed: %#v", worker)
		}
	}
}

func TestParallelMergeOrderAndBatchDiagnostics(t *testing.T) {
	common := sidecarDiagnostic{Category: "error", Code: "transformer-not-found", Message: "factory failed"}
	one := sidecarDiagnostic{File: "a", Message: "first"}
	two := sidecarDiagnostic{File: "b", Message: "second"}
	first := &sidecarResponse{ParallelBatchDiagnostics: 1, Diagnostics: []sidecarDiagnostic{common, one},
		Transformed: []sidecarOutputFile{{FileName: "a", Text: "A", TraceMap: "trace-a"}}, AfterDeclarationsTransformers: 1,
		Metrics: &TransformerResponseMetrics{WallMs: 500, CPUUserUs: 100, Plugins: []TransformerPluginMetric{{Transform: "example", Ms: 20}}}}
	second := &sidecarResponse{ParallelBatchDiagnostics: 1, Diagnostics: []sidecarDiagnostic{common, two},
		Transformed: []sidecarOutputFile{{FileName: "b", Text: "B", TraceMap: "trace-b"}}, AfterDeclarationsTransformers: 1,
		Metrics: &TransformerResponseMetrics{WallMs: 400, CPUUserUs: 120, Plugins: []TransformerPluginMetric{{Transform: "example", Ms: 30}}}}
	merged, last, err := mergeParallelResponses(first, second, 550*time.Millisecond)
	if err != nil || last != 1 {
		t.Fatalf("%d: %v", last, err)
	}
	if !reflect.DeepEqual(merged.Diagnostics, []sidecarDiagnostic{common, one, two}) ||
		!reflect.DeepEqual(merged.Transformed, append(first.Transformed, second.Transformed...)) {
		t.Fatal(merged)
	}
	if merged.Metrics.WallMs != 550 || merged.Metrics.CPUUserUs != 220 || merged.Metrics.Plugins[0].Ms != 50 || merged.Metrics.Phases != nil {
		t.Fatal(merged.Metrics)
	}
	second.Diagnostics[0].Message = "different state"
	if _, _, err := mergeParallelResponses(first, second, time.Second); err == nil {
		t.Fatal("accepted incompatible factory state")
	}
}

func TestParallelMergeInternalFailureDiscardsAllOutputs(t *testing.T) {
	success := &sidecarResponse{Diagnostics: []sidecarDiagnostic{}, Transformed: []sidecarOutputFile{{FileName: "a"}}}
	failure := &sidecarResponse{Diagnostics: []sidecarDiagnostic{{Category: "error", Code: "sidecar-internal", Message: "exact stack\r\n"}}, Transformed: []sidecarOutputFile{}}
	for _, responses := range [][2]*sidecarResponse{{failure, success}, {success, failure}} {
		merged, last, err := mergeParallelResponses(responses[0], responses[1], time.Second)
		want := 0
		if responses[0] == success {
			want = 1
		}
		if err != nil || last != want || !reflect.DeepEqual(merged.Diagnostics, failure.Diagnostics) || len(merged.Transformed) != 0 {
			t.Fatalf("%#v, %d, %v", merged, last, err)
		}
	}
}

func TestParallelMergePreservesSerialDiagnosticPhases(t *testing.T) {
	common := sidecarDiagnostic{Code: "transformer-not-found", Message: "factory"}
	missing0 := sidecarDiagnostic{Code: "source-file-missing", File: "a"}
	missing1 := sidecarDiagnostic{Code: "source-file-missing", File: "b"}
	transform0 := sidecarDiagnostic{Code: "transform", File: "a"}
	transform1 := sidecarDiagnostic{Code: "transform", File: "b"}
	first := &sidecarResponse{ParallelBatchDiagnostics: 1, ParallelSourceDiagnostics: 1, Diagnostics: []sidecarDiagnostic{common, missing0, transform0}}
	second := &sidecarResponse{ParallelBatchDiagnostics: 1, ParallelSourceDiagnostics: 1, Diagnostics: []sidecarDiagnostic{common, missing1, transform1}}
	merged, _, err := mergeParallelResponses(first, second, time.Second)
	if err != nil || !reflect.DeepEqual(merged.Diagnostics, []sidecarDiagnostic{common, missing0, missing1, transform0, transform1}) {
		t.Fatalf("%#v: %v", merged, err)
	}
}

func TestParallelStderrUsesSerialLineForwarding(t *testing.T) {
	var output bytes.Buffer
	previous := logservice.Output
	logservice.Output = &output
	defer func() { logservice.Output = previous }()
	logservice.Write("start\n")
	empty := newSidecarStderrTail(strings.NewReader(""))
	<-empty.done
	empty.drainTo()
	logservice.WriteLine("next")
	if output.String() != "start\nnext\n" {
		t.Fatalf("empty stderr changed log state: %q", output.String())
	}
	output.Reset()
	payload := strings.Repeat("x", sidecarStderrLineLimit+1) + "\r\nunterminated"
	stderr := newSidecarStderrTail(strings.NewReader(payload))
	<-stderr.done
	stderr.drainTo()
	want := strings.Repeat("x", sidecarStderrLineLimit) + sidecarStderrTruncated + "\nunterminated\n"
	if output.String() != want {
		t.Fatal("stderr differs from serial line forwarding")
	}
}

func TestParallelResolutionRejectsSourceAliasesAndNodeModules(t *testing.T) {
	root, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	source := filepath.Join(root, "a.ts")
	if err := os.WriteFile(source, []byte("export {};"), 0o600); err != nil {
		t.Fatal(err)
	}
	if parallelResolutionScope(root, []string{filepath.Join(root, "node_modules", "plugin", "a.ts")}) {
		t.Fatal("accepted dependency scope")
	}
	if parallelResolutionScope(root, []string{filepath.Join(root, "missing", "a.ts")}) {
		t.Fatal("accepted unknown directory")
	}
	if parallelResolutionScope(root, []string{filepath.Join(root, "missing.ts")}) {
		t.Fatal("accepted missing source")
	}
	alias := filepath.Join(root, "alias.ts")
	if err := os.Symlink(source, alias); err != nil {
		t.Skipf("source symlinks unavailable: %v", err)
	}
	if parallelResolutionScope(root, []string{source, alias}) {
		t.Fatal("accepted source alias")
	}
}

func TestParallelAdmissionDefaultOffAndResolutionBoundary(t *testing.T) {
	root, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	src := filepath.Join(root, "src", "nested")
	if err := os.MkdirAll(src, 0o755); err != nil {
		t.Fatal(err)
	}
	files := []string{filepath.Join(src, "a.ts"), filepath.Join(src, "b.ts")}
	for _, file := range files {
		if err := os.WriteFile(file, []byte("export {};"), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	request := sidecarRequest{ProjectDir: root, CompileFileNames: files, Plugins: []json.RawMessage{json.RawMessage(`{"transform":"example"}`)}}
	call := transformerCall{dir: root}
	if parallelRequestSupported(call, request) || parallelConfigSupported(&config.Config{}) {
		t.Fatal("default enabled")
	}
	profile := &config.ParallelTransformersConfig{Workers: 2, BatchIndependent: true, TypeScript: "reviewed", Plugins: []config.ParallelTransformerPlugin{{Config: `{"transform":"example"}`}}}
	call.parallel = profile
	if !parallelRequestSupported(call, request) {
		t.Fatal("rejected simple authored scope")
	}
	request.RootFileNames = []string{files[0]}
	if parallelRequestSupported(call, request) {
		t.Fatal("accepted narrowed roots")
	}
	request.RootFileNames = nil
	request.ChangedFiles = []sidecarChangedFile{{FileName: filepath.Join(root, "tsconfig.json"), Text: "{}"}}
	if parallelRequestSupported(call, request) {
		t.Fatal("accepted config overlay")
	}
	request.ChangedFiles = []sidecarChangedFile{{FileName: files[1], Text: "export const overlay = true"}}
	if !parallelRequestSupported(call, request) {
		t.Fatal("rejected authored overlay")
	}
	if err := os.WriteFile(filepath.Join(src, "package.json"), []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	if parallelRequestSupported(call, request) {
		t.Fatal("accepted nested package boundary")
	}
	request.CompileFileNames[0] = filepath.Join(filepath.Dir(root), "outside.ts")
	if parallelRequestSupported(call, request) {
		t.Fatal("accepted outside source")
	}
	if parallelConfigSupported(&config.Config{ParallelTransformers: profile, Warnings: []string{`rotor.toml: unknown key "parallelTransformers.future" (ignored)`}}) {
		t.Fatal("accepted unknown attestation semantics")
	}
}

type parallelRecordingInput struct {
	mu          sync.Mutex
	requests    []sidecarRequest
	closed      bool
	onTransform func()
	closeReader io.Closer
}

func (input *parallelRecordingInput) Write(payload []byte) (int, error) {
	var request sidecarRequest
	if err := json.Unmarshal(bytes.TrimSpace(payload), &request); err != nil {
		return 0, err
	}
	input.mu.Lock()
	input.requests = append(input.requests, request)
	input.mu.Unlock()
	if request.Operation == "transform" && input.onTransform != nil {
		input.onTransform()
	}
	return len(payload), nil
}

func (input *parallelRecordingInput) recordedRequests() []sidecarRequest {
	input.mu.Lock()
	defer input.mu.Unlock()
	return slices.Clone(input.requests)
}

func (input *parallelRecordingInput) Close() error {
	input.closed = true
	if input.closeReader != nil {
		return input.closeReader.Close()
	}
	return nil
}

func parallelScriptedSession(ready bool, response *sidecarResponse, stderr string) (*sidecarSession, *parallelRecordingInput) {
	preflight, _ := json.Marshal(sidecarResponse{ParallelReady: ready})
	text := string(preflight) + "\n"
	if response != nil {
		encoded, _ := json.Marshal(response)
		text += string(encoded) + "\n"
	}
	input := &parallelRecordingInput{}
	tail := newSidecarStderrTail(strings.NewReader(stderr))
	<-tail.done
	return &sidecarSession{cmd: &exec.Cmd{}, stdin: input, stdout: bufio.NewReader(strings.NewReader(text)),
		stderr: tail}, input
}

func TestParallelPreflightMismatchKeepsSerialSession(t *testing.T) {
	first, input := parallelScriptedSession(false, nil, "")
	slot := &sidecarSlot{session: first}
	spawn := func(string, string, bool) (*sidecarSession, error) {
		t.Fatal("spawned before attestation matched")
		return nil, nil
	}
	_, handled, err := runParallelTransformerWithSpawner(context.Background(), time.Second, "", slot, sidecarRequest{}, nil, &sidecarCallStats{}, spawn)
	requests := input.recordedRequests()
	if err != nil || handled || input.closed || slot.session != first || len(requests) != 1 || requests[0].Operation != "parallel-preflight" {
		t.Fatalf("handled=%v err=%v requests=%#v", handled, err, requests)
	}
}

func TestParallelScriptedCloseAndStderrOrder(t *testing.T) {
	for _, failFirst := range []bool{false, true} {
		t.Run(fmtBool(failFirst), func(t *testing.T) {
			response := &sidecarResponse{Diagnostics: []sidecarDiagnostic{}, Transformed: []sidecarOutputFile{}}
			if failFirst {
				response.Diagnostics = []sidecarDiagnostic{{Category: "error", Code: "sidecar-internal", Message: "exact"}}
			}
			first, input0 := parallelScriptedSession(true, response, "first\r\npartial")
			second, input1 := parallelScriptedSession(true, &sidecarResponse{}, "second\n")
			var output bytes.Buffer
			previous := logservice.Output
			logservice.Output = &output
			defer func() { logservice.Output = previous }()
			slot := &sidecarSlot{session: first}
			spawns := 0
			spawn := func(string, string, bool) (*sidecarSession, error) { spawns++; return second, nil }
			request := sidecarRequest{CompileFileNames: []string{"a", "b"}, ChangedFiles: []sidecarChangedFile{{FileName: "b", Text: "overlay"}}}
			stats := &sidecarCallStats{}
			_, handled, err := runParallelTransformerWithSpawner(context.Background(), time.Second, "", slot, request, nil, stats, spawn)
			if err != nil || !handled || spawns != 1 || slot.session != nil || !input0.closed || !input1.closed {
				t.Fatalf("%v %v", handled, err)
			}
			if stats.additionalSpawns != 1 {
				t.Fatal("second worker missing from spawn metrics")
			}
			want := "first\npartial\nsecond\n"
			if failFirst {
				want = "first\npartial\n"
			}
			if output.String() != want {
				t.Fatalf("stderr %q != %q", output.String(), want)
			}
			for _, input := range []*parallelRecordingInput{input0, input1} {
				requests := input.recordedRequests()
				if len(requests) != 2 || !reflect.DeepEqual(requests[1].ChangedFiles, request.ChangedFiles) {
					t.Fatal(requests)
				}
			}
		})
	}
}

func fmtBool(value bool) string {
	if value {
		return "internal-failure"
	}
	return "success"
}

func TestParallelTransportFailureNeverRetriesSerial(t *testing.T) {
	first, input0 := parallelScriptedSession(true, nil, "")
	second, input1 := parallelScriptedSession(true, nil, "")
	slot := &sidecarSlot{session: first}
	spawns := 0
	spawn := func(string, string, bool) (*sidecarSession, error) { spawns++; return second, nil }
	_, handled, err := runParallelTransformerWithSpawner(context.Background(), time.Second, "", slot, sidecarRequest{CompileFileNames: []string{"a", "b"}}, nil, &sidecarCallStats{}, spawn)
	if !handled || err == nil || spawns != 1 || !input0.closed || !input1.closed || slot.session != nil || len(input0.recordedRequests()) > 2 || len(input1.recordedRequests()) > 2 {
		t.Fatalf("handled=%v error=%v spawns=%d", handled, err, spawns)
	}
}

func TestParallelCancellationClosesBothWorkers(t *testing.T) {
	first, input0 := parallelScriptedSession(true, nil, "")
	second, input1 := parallelScriptedSession(true, nil, "")
	reader, writer := io.Pipe()
	preflight, _ := json.Marshal(sidecarResponse{ParallelReady: true})
	first.stdout = bufio.NewReader(io.MultiReader(strings.NewReader(string(preflight)+"\n"), reader))
	input0.closeReader = writer
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	input0.onTransform = cancel
	// Worker 1 waits too, so its EOF cannot race and cancel worker 0 first.
	reader1, writer1 := io.Pipe()
	second.stdout = bufio.NewReader(io.MultiReader(strings.NewReader(string(preflight)+"\n"), reader1))
	input1.closeReader = writer1
	slot := &sidecarSlot{session: first}
	spawn := func(string, string, bool) (*sidecarSession, error) { return second, nil }
	_, handled, err := runParallelTransformerWithSpawner(ctx, time.Second, "", slot, sidecarRequest{Operation: "transform", CompileFileNames: []string{"a", "b"}}, nil, &sidecarCallStats{}, spawn)
	if !handled || !errors.Is(err, context.Canceled) || !input0.closed || !input1.closed || slot.session != nil {
		t.Fatalf("%v %v", handled, err)
	}
}

func TestParallelScopeRejectsPlainNestedNodeModules(t *testing.T) {
	root, err := filepath.EvalSymlinks(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	x, y := filepath.Join(root, "src", "a", "x.ts"), filepath.Join(root, "src", "b", "y.ts")
	for _, file := range []string{x, y} {
		if err := os.MkdirAll(filepath.Dir(file), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(file, []byte("export {};"), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.Mkdir(filepath.Join(root, "node_modules"), 0o755); err != nil {
		t.Fatal(err)
	}
	if !parallelResolutionScope(root, []string{x, y}) {
		t.Fatal("root dependency scope rejected")
	}
	if err := os.Mkdir(filepath.Join(filepath.Dir(x), "NoDe_MoDuLeS"), 0o755); err != nil {
		t.Fatal(err)
	}
	if parallelResolutionScope(root, []string{x, y}) {
		t.Fatal("plain nested dependency directory admitted")
	}
}

func TestParallelPrefixGatePreservesProjectLabelsAndSharedToken(t *testing.T) {
	dir := t.TempDir()
	text := `[parallelTransformers]
workers = 2
batchIndependent = true
typescript = "reviewed"
[[parallelTransformers.plugins]]
config = '{"transform":"example"}'
sha256 = "reviewed"
`
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte(text), 0o600); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithCancel(pprof.WithLabels(context.Background(), pprof.Labels("project", "labelled-project")))
	defer cancel()
	opts := ProjectOptions{Timings: &BuildTimings{ctx: ctx}}
	pipeline := &flameworkPipeline{prefix: []transformerPluginConfig{{Transform: "example"}}}
	if parallelPrefixContext(opts.context(), dir, pipeline, opts) != ctx {
		t.Fatal("unmarked/API/watch context admitted")
	}
	MarkOneShotCLITransformerBuild(&opts)
	marked := parallelPrefixContext(opts.context(), dir, pipeline, opts)
	prefix, _ := marked.Value(parallelPrefixKey{}).(*parallelPrefix)
	if prefix == nil {
		t.Fatal("one-shot prefix not marked")
	}
	if value, _ := pprof.Label(marked, "project"); value != "labelled-project" || marked.Done() != ctx.Done() || opts.Context != nil {
		t.Fatal("project profiling/cancellation context replaced")
	}
	pipeline.suffix = []transformerPluginConfig{{Transform: "later"}}
	if parallelPrefixContext(ctx, dir, pipeline, opts) != ctx {
		t.Fatal("later external suffix admitted")
	}
	pipeline.suffix = nil
	opts.census = &censusCollector{}
	if parallelPrefixContext(ctx, dir, pipeline, opts) != ctx {
		t.Fatal("census admitted")
	}
	opts.census = nil
	api := WithTransformerCallback(ctx, func(context.Context, TransformerRequest) (TransformerResponse, error) {
		return TransformerResponse{}, nil
	})
	if parallelPrefixContext(api, dir, pipeline, opts) != api {
		t.Fatal("API callback admitted")
	}
	configPath := filepath.Join(dir, "tsconfig.json")
	if err := os.WriteFile(configPath, []byte("{}"), 0o600); err != nil {
		t.Fatal(err)
	}
	referenced, err := ProjectOptionsForReferencedConfig(opts, configPath, false)
	if err != nil || referenced.cliTransformerBuild != opts.cliTransformerBuild {
		t.Fatalf("solution option copy lost shared provenance: %v", err)
	}
	if !opts.cliTransformerBuild.trySplit() || referenced.cliTransformerBuild.trySplit() {
		t.Fatal("concurrent split token admitted")
	}
	<-opts.cliTransformerBuild.split
	if !referenced.cliTransformerBuild.trySplit() {
		t.Fatal("released token not reusable by another project")
	}
	<-referenced.cliTransformerBuild.split
}

func TestParallelPreflightFailureKeepsBoundedSerialFallback(t *testing.T) {
	first, input := parallelScriptedSession(true, nil, "")
	first.stdout = bufio.NewReader(strings.NewReader("")) // transport EOF before transform
	slot := &sidecarSlot{session: first}
	spawn := func(string, string, bool) (*sidecarSession, error) {
		t.Fatal("auxiliary spawned after primary failed")
		return nil, nil
	}
	_, handled, err := runParallelTransformerWithSpawner(context.Background(), time.Second, "", slot, sidecarRequest{}, nil, &sidecarCallStats{}, spawn)
	if handled || err == nil || slot.session != nil || !input.closed || len(input.recordedRequests()) != 1 {
		t.Fatalf("handled=%v err=%v", handled, err)
	}
	// Auxiliary timeout must leave worker 0 untouched for a fresh ordinary
	// transform timeout, rather than spend its transport retry budget.
	first, input = parallelScriptedSession(true, &sidecarResponse{}, "")
	second, auxiliaryInput := parallelScriptedSession(true, nil, "")
	reader, writer := io.Pipe()
	second.stdout = bufio.NewReader(reader)
	auxiliaryInput.closeReader = writer
	slot.session = first
	spawn = func(string, string, bool) (*sidecarSession, error) { return second, nil }
	_, handled, err = runParallelTransformerWithSpawner(context.Background(), 10*time.Millisecond, "", slot, sidecarRequest{}, nil, &sidecarCallStats{}, spawn)
	if handled || err != nil || input.closed || slot.session != first || !auxiliaryInput.closed {
		t.Fatalf("auxiliary timeout: %v %v", handled, err)
	}
	response, err := parallelRoundTrip(context.Background(), first, sidecarRequest{Operation: "transform"}, &sidecarCallStats{})
	if err != nil || response == nil || len(input.recordedRequests()) != 2 {
		t.Fatalf("ordinary serial request failed: %v", err)
	}
}

func TestParallelInternalCodeRequiresSerialCatchShape(t *testing.T) {
	first := &sidecarResponse{Diagnostics: []sidecarDiagnostic{{Category: "error", Code: "sidecar-internal"}}, Transformed: []sidecarOutputFile{{FileName: "a"}}}
	second := &sidecarResponse{Transformed: []sidecarOutputFile{{FileName: "b"}}}
	merged, last, err := mergeParallelResponses(first, second, time.Second)
	if err != nil || last != 1 || len(merged.Transformed) != 2 {
		t.Fatal("arbitrary diagnostic code treated as a serial catch")
	}
}
