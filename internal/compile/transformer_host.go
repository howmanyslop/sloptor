package compile

import (
	"context"
	"encoding/json"
	"path/filepath"

	"rotor/tsgo/ast"
)

// TransformerRequest is the native-to-JavaScript transformer protocol. It is
// shared by the subprocess sidecar and API callback adapters.
type TransformerRequest struct {
	Protocol         int                      `json:"protocol"`
	Operation        string                   `json:"operation"`
	TsConfigPath     string                   `json:"tsConfigPath"`
	ProjectDir       string                   `json:"projectDir"`
	CompileFileNames []string                 `json:"compileFileNames"`
	RootFileNames    []string                 `json:"rootFileNames,omitempty"`
	ChangedFiles     []TransformerChangedFile `json:"changedFiles"`
	Plugins          []json.RawMessage        `json:"plugins,omitempty"`
}

type TransformerChangedFile struct {
	FileName string `json:"fileName"`
	Text     string `json:"text"`
}

type TransformerResponse struct {
	Diagnostics                   []TransformerDiagnostic      `json:"diagnostics"`
	Transformed                   []TransformerOutputFile      `json:"transformed"`
	Metrics                       *TransformerResponseMetrics  `json:"metrics,omitempty"`
	AfterDeclarationsTransformers int                          `json:"afterDeclarationsTransformers"`
	Transport                     *TransformerTransportMetrics `json:"-"`
}

// TransformerTransportMetrics are populated by transports that can measure
// the bytes they actually send and receive without serializing payloads again.
type TransformerTransportMetrics struct {
	RequestBytes  int64
	ResponseBytes int64
}

type TransformerResponseMetrics struct {
	WallMs      int64                     `json:"wallMs"`
	CPUUserUs   int64                     `json:"cpuUserUs"`
	CPUSystemUs int64                     `json:"cpuSystemUs"`
	NodeVersion string                    `json:"nodeVersion"`
	Plugins     []TransformerPluginMetric `json:"plugins,omitempty"`
}

type TransformerPluginMetric struct {
	Transform string `json:"transform"`
	Ms        int64  `json:"ms"`
}

type TransformerDiagnostic struct {
	Category string `json:"category"`
	Code     string `json:"code"`
	File     string `json:"file"`
	Start    int    `json:"start"`
	Length   int    `json:"length"`
	Message  string `json:"message"`
}

type TransformerOutputFile struct {
	FileName string `json:"fileName"`
	Text     string `json:"text"`
	TraceMap string `json:"traceMap"`
}

// TransformerCallback executes a transformer request in the API client.
type TransformerCallback func(context.Context, TransformerRequest) (TransformerResponse, error)

type transformerCallbackContextKey struct{}

type transformerHost interface {
	transform(context.Context, transformerCall) (*sidecarResponse, sidecarCallStats, error)
	validate(context.Context, string, string) (*sidecarResponse, sidecarCallStats, error)
}

type transformerCall struct {
	dir          string
	configPath   string
	compileFiles []*ast.SourceFile
	stampFiles   []*ast.SourceFile
	overlays     map[string]string
	plugins      []json.RawMessage
	state        *sidecarBuildState
}

func canonicalTransformerRequestPaths(dir, configPath string) (string, string, string) {
	dir = canonicalSidecarPath(dir)
	configPath = canonicalSidecarPath(configPath)
	return dir, configPath, dir + "|" + configPath
}

func prepareTransformerRequest(call transformerCall, tracker *sidecarSession, dir, configPath string, stats *sidecarCallStats) (sidecarRequest, error) {
	stampNames := make([]string, 0, len(call.stampFiles))
	for _, sourceFile := range call.stampFiles {
		stampNames = append(stampNames, sourceFile.FileName())
	}
	overlays, overlayReads := mergeSidecarOverlays(call.compileFiles, call.overlays, call.state, true)
	stats.reads += overlayReads
	skipDiskScan := call.state != nil && call.state.diskScanned && len(tracker.stamps) > 0
	changedFiles, ioStats, err := tracker.collectChangedFiles(stampNames, overlays, skipDiskScan)
	stats.stats += ioStats.stats
	stats.reads += ioStats.reads
	stats.changedFiles += ioStats.changedFiles
	if err != nil {
		return sidecarRequest{}, err
	}

	request := sidecarRequest{
		Protocol:         1,
		Operation:        "transform",
		TsConfigPath:     filepath.FromSlash(configPath),
		ProjectDir:       filepath.FromSlash(dir),
		CompileFileNames: make([]string, 0, len(call.compileFiles)),
		ChangedFiles:     changedFiles,
		Plugins:          call.plugins,
	}
	for _, sourceFile := range call.compileFiles {
		request.CompileFileNames = append(request.CompileFileNames, filepath.FromSlash(sourceFile.FileName()))
	}
	request.RootFileNames = narrowedSidecarRoots(call.compileFiles, call.stampFiles)
	return request, nil
}

func prepareTransformerValidationRequest(dir, configPath string) (sidecarRequest, string, string, string) {
	dir, configPath, key := canonicalTransformerRequestPaths(dir, configPath)
	return sidecarRequest{
		Protocol:     1,
		Operation:    "validate",
		TsConfigPath: filepath.FromSlash(configPath),
		ProjectDir:   filepath.FromSlash(dir),
	}, dir, configPath, key
}

type transformerCallbackContext struct {
	callback TransformerCallback
	host     transformerHost
}

func WithTransformerCallback(ctx context.Context, callback TransformerCallback) context.Context {
	value := &transformerCallbackContext{callback: callback}
	value.host = newCallbackTransformerHost(callback)
	return context.WithValue(ctx, transformerCallbackContextKey{}, value)
}

func TransformerCallbackFromContext(ctx context.Context) TransformerCallback {
	value, _ := ctx.Value(transformerCallbackContextKey{}).(*transformerCallbackContext)
	if value == nil {
		return nil
	}
	return value.callback
}

func transformerHostFromContext(ctx context.Context) transformerHost {
	if ctx != nil {
		if value, _ := ctx.Value(transformerCallbackContextKey{}).(*transformerCallbackContext); value != nil {
			return value.host
		}
	}
	return sidecarTransformerHost{}
}
