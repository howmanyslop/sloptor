package compile

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"maps"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"runtime/trace"
	"slices"
	"strings"
	"sync"
	"time"

	"rotor/internal/config"
	"rotor/internal/logservice"
	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/vfs"
	"rotor/tsgo/vfs/osvfs"
)

// defaultSidecarResponseTimeout is intentionally generous: upstream rbxtsc
// awaits the transformer with no timeout, so the cap only guards against a
// hung worker — never a legitimately slow build (Flamework + React Compiler
// on a full project can take minutes on slow machines).
const defaultSidecarResponseTimeout = 10 * time.Minute

const sidecarResponseTimeoutEnv = "ROTOR_SIDECAR_TIMEOUT"

type sidecarRequest struct {
	Protocol         int                  `json:"protocol"`
	TsConfigPath     string               `json:"tsConfigPath"`
	ProjectDir       string               `json:"projectDir"`
	CompileFileNames []string             `json:"compileFileNames"`
	ChangedFiles     []sidecarChangedFile `json:"changedFiles"`
	Plugins          []json.RawMessage    `json:"plugins,omitempty"`
	TransformSources bool                 `json:"transformSources"`
	EmitDeclarations bool                 `json:"emitDeclarations"`
}

type sidecarEmitMode struct {
	transformSources bool
	emitDeclarations bool
}

var (
	sidecarEmitSources      = sidecarEmitMode{transformSources: true}
	sidecarEmitDeclarations = sidecarEmitMode{emitDeclarations: true}
	sidecarEmitBoth         = sidecarEmitMode{transformSources: true, emitDeclarations: true}
)

type sidecarChangedFile struct {
	FileName string `json:"fileName"`
	Text     string `json:"text"`
}

type sidecarResponse struct {
	Diagnostics  []sidecarDiagnostic     `json:"diagnostics"`
	Transformed  []sidecarOutputFile     `json:"transformed"`
	Declarations []sidecarOutputFile     `json:"declarations"`
	Metrics      *sidecarResponseMetrics `json:"metrics,omitempty"`
}

type sidecarResponseMetrics struct {
	WallMs      int64  `json:"wallMs"`
	CPUUserUs   int64  `json:"cpuUserUs"`
	CPUSystemUs int64  `json:"cpuSystemUs"`
	NodeVersion string `json:"nodeVersion"`
}

type sidecarCallStats struct {
	wait            time.Duration
	prep            time.Duration
	roundTrip       time.Duration
	decode          time.Duration
	requestBytes    int64
	responseBytes   int64
	stats           int
	reads           int
	changedFiles    int
	spawned         bool
	restarted       bool
	nodeWallMs      int64
	nodeCPUUserUs   int64
	nodeCPUSystemUs int64
	nodeVersion     string
}

type sidecarDiagnostic struct {
	Category string `json:"category"`
	Code     string `json:"code"`
	File     string `json:"file"`
	Start    int    `json:"start"`
	Length   int    `json:"length"`
	Message  string `json:"message"`
}

type sidecarOutputFile struct {
	FileName string `json:"fileName"`
	Text     string `json:"text"`
	TraceMap string `json:"traceMap"`
}

type preparedTransformerProgram struct {
	program                  *compiler.Program
	sourceFiles              []*ast.SourceFile
	flamework                *config.FlameworkConfig
	declarations             []sidecarOutputFile
	sourceTraces             diagnosticTraces
	sidecarWaitDuration      time.Duration
	sidecarPrepDuration      time.Duration
	sidecarRoundTripDuration time.Duration
	sidecarDecodeDuration    time.Duration
	overlayProgramDuration   time.Duration
	sidecarRequestBytes      int64
	sidecarResponseBytes     int64
	sidecarStats             int
	sidecarReads             int
	sidecarChangedFiles      int
	sidecarSpawned           bool
	sidecarRestarted         bool
	nodeWallMs               int64
	nodeCPUUserUs            int64
	nodeCPUSystemUs          int64
	nodeVersion              string
	sidecarRoundTripRecorded bool
	overlayProgramRecorded   bool
}

func prepareProjectProgramForCompile(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string) (*compiler.Program, []*ast.SourceFile, diagnosticTraces, []string, error) {
	prepared, diags, err := prepareTransformerProgram(dir, program, sourceFiles, overlays)
	if err != nil {
		return nil, nil, nil, diags, err
	}
	return prepared.program, prepared.sourceFiles, prepared.sourceTraces, nil, nil
}

func prepareTransformerProgram(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string) (*preparedTransformerProgram, []string, error) {
	flamework, diags, err := prepareFlameworkConfig(dir, program.CommandLine())
	if err != nil {
		return nil, diags, err
	}
	if len(sourceFiles) == 0 {
		return &preparedTransformerProgram{program: program, sourceFiles: sourceFiles, flamework: flamework}, nil, nil
	}
	if !projectUsesTransformerPlugins(program.CommandLine()) && !declarationUsesPathAliases(program) {
		return &preparedTransformerProgram{program: program, sourceFiles: sourceFiles, flamework: flamework}, nil, nil
	}

	transformed, diags, err := applyTransformerSidecar(dir, program, sourceFiles, overlays)
	if err != nil {
		return nil, diags, err
	}
	if transformed.program == program {
		prepared := *transformed
		prepared.sourceFiles = sourceFiles
		prepared.flamework = flamework
		return &prepared, nil, nil
	}

	remapped, err := remapProgramSourceFiles(transformed.program, sourceFiles)
	if err != nil {
		return nil, nil, err
	}
	prepared := *transformed
	prepared.sourceFiles = remapped
	prepared.flamework = flamework
	return &prepared, nil, nil
}

func declarationUsesPathAliases(program *compiler.Program) bool {
	options := program.Options()
	return options.GetEmitDeclarations() && (options.BaseUrl != "" || options.Paths != nil && options.Paths.Size() > 0)
}

func applyTransformerSidecar(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string) (*preparedTransformerProgram, []string, error) {
	return applyTransformerSidecarWithPlugins(dir, program, sourceFiles, overlays, nil, sidecarEmitBoth, nil)
}

func applyTransformerSidecarWithPlugins(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string, plugins []json.RawMessage, mode sidecarEmitMode, state *sidecarBuildState) (*preparedTransformerProgram, []string, error) {
	configPath := program.Options().ConfigFilePath
	if configPath == "" {
		configPath = filepath.ToSlash(filepath.Join(filepath.FromSlash(dir), "tsconfig.json"))
	}

	sidecarRegion := trace.StartRegion(context.Background(), "transformer sidecar")
	response, stats, err := runTransformerSidecar(dir, configPath, sourceFiles, projectSourceFiles(program), overlays, plugins, mode, state)
	sidecarRegion.End()
	if err != nil {
		return nil, []string{err.Error()}, err
	}

	var errorDiags []string
	for _, diag := range response.Diagnostics {
		text := formatSidecarDiagnostic(diag)
		if strings.EqualFold(diag.Category, "warning") {
			logservice.Warn(text)
			continue
		}
		errorDiags = append(errorDiags, text)
	}
	if len(errorDiags) > 0 {
		return nil, errorDiags, errors.New("compile: transformer sidecar diagnostics")
	}
	decodeStarted := time.Now()
	sourceTraces := make(diagnosticTraces)
	for _, file := range response.Transformed {
		if file.TraceMap == "" {
			continue
		}
		original := program.GetSourceFile(file.FileName)
		if original == nil {
			return nil, nil, fmt.Errorf("compile: transformer trace source missing from program: %s", file.FileName)
		}
		fileTrace, err := newSourceTraceMap(file.TraceMap, original.FileName(), original.Text())
		if err != nil {
			return nil, nil, err
		}
		sourceTraces[normalizeSourceFilePath(file.FileName)] = fileTrace
	}
	stats.decode += time.Since(decodeStarted)
	state.addCall(stats)
	if mode.transformSources {
		state.absorbTransformed(response.Transformed)
	}
	prepared := preparedFromSidecarStats(program, response.Declarations, sourceTraces, stats)
	if len(response.Transformed) == 0 {
		return prepared, nil, nil
	}

	// The worker reports transformed text only for the files it was asked to
	// compile, which on a single-file or incremental route is a subset of the
	// project. Rebuilding on that alone would read every other file off disk
	// and drop the caller's overlay on it, so the two layer: transformed text
	// wins where it exists, the caller's overlay stands everywhere else.
	programOverlays := normalizeOverlays(overlays)
	caseSensitive := osvfs.FS().UseCaseSensitiveFileNames()
	for _, file := range response.Transformed {
		programOverlays[normalizeOverlayPath(file.FileName, caseSensitive)] = file.Text
	}
	overlayStarted := time.Now()
	overlayRegion := trace.StartRegion(context.Background(), "overlay program creation and parse/load")
	transformedProgram, _, err := updateProgramWithTextOverlays(program, programOverlays)
	overlayRegion.End()
	overlayDuration := time.Since(overlayStarted)
	if err != nil {
		transformedProgram, _, err = newProjectProgramWithOverlay(dir, configPath, programOverlays, program.Options().Checkers, singleThreadedFromOptions(program.Options()))
		if err != nil {
			return nil, nil, err
		}
	}
	if transformedProgram == nil {
		return nil, nil, errors.New("compile: overlay Program update returned nil")
	}
	prepared.program = transformedProgram
	prepared.overlayProgramDuration = overlayDuration
	prepared.overlayProgramRecorded = true
	return prepared, nil, nil
}

func preparedFromSidecarStats(program *compiler.Program, declarations []sidecarOutputFile, sourceTraces diagnosticTraces, stats sidecarCallStats) *preparedTransformerProgram {
	return &preparedTransformerProgram{
		program:                  program,
		declarations:             declarations,
		sourceTraces:             sourceTraces,
		sidecarWaitDuration:      stats.wait,
		sidecarPrepDuration:      stats.prep,
		sidecarRoundTripDuration: stats.roundTrip,
		sidecarDecodeDuration:    stats.decode,
		sidecarRequestBytes:      stats.requestBytes,
		sidecarResponseBytes:     stats.responseBytes,
		sidecarStats:             stats.stats,
		sidecarReads:             stats.reads,
		sidecarChangedFiles:      stats.changedFiles,
		sidecarSpawned:           stats.spawned,
		sidecarRestarted:         stats.restarted,
		nodeWallMs:               stats.nodeWallMs,
		nodeCPUUserUs:            stats.nodeCPUUserUs,
		nodeCPUSystemUs:          stats.nodeCPUSystemUs,
		nodeVersion:              stats.nodeVersion,
		sidecarRoundTripRecorded: true,
	}
}

type sidecarFileStamp struct {
	modTime time.Time
	size    int64
}

// sidecarSession is one warm Node worker. It lives for the rotor process
// lifetime (the worker exits when our pipes close), so watch rebuilds reuse
// the JS program — upstream's persistent transformerWatcher semantics.
type sidecarSession struct {
	cmd    *exec.Cmd
	stdin  io.WriteCloser
	stdout *bufio.Reader
	stderr *sidecarStderrTail
	stamps map[string]sidecarFileStamp
	// overlaid names the files whose text in the worker came from a caller
	// overlay rather than from disk, keyed the way changedFilesFor looks an
	// overlay up and valued with the path a request carries. The worker's
	// overrides map outlives the round trip that filled it, so an overlay that
	// goes away has to be undone by hand.
	overlaid map[string]string
	dead     bool
}

type sidecarSlot struct {
	mu      sync.Mutex
	session *sidecarSession
}

type sidecarBuildState struct {
	stageOverlays map[string]string
	diskScanned   bool
	call          sidecarCallStats
}

func newSidecarBuildState() *sidecarBuildState {
	return &sidecarBuildState{stageOverlays: map[string]string{}}
}

func (s *sidecarBuildState) absorbSourceFiles(files []*ast.SourceFile) {
	if s == nil {
		return
	}
	if s.stageOverlays == nil {
		s.stageOverlays = map[string]string{}
	}
	for _, file := range files {
		s.stageOverlays[file.FileName()] = file.Text()
	}
}

func (s *sidecarBuildState) absorbTransformed(files []sidecarOutputFile) {
	if s == nil {
		return
	}
	if s.stageOverlays == nil {
		s.stageOverlays = map[string]string{}
	}
	for _, file := range files {
		s.stageOverlays[file.FileName] = file.Text
	}
}

func (s *sidecarBuildState) addCall(stats sidecarCallStats) {
	if s == nil {
		return
	}
	s.call.wait += stats.wait
	s.call.prep += stats.prep
	s.call.roundTrip += stats.roundTrip
	s.call.decode += stats.decode
	s.call.requestBytes += stats.requestBytes
	s.call.responseBytes += stats.responseBytes
	s.call.stats += stats.stats
	s.call.reads += stats.reads
	s.call.changedFiles += stats.changedFiles
	s.call.spawned = s.call.spawned || stats.spawned
	s.call.restarted = s.call.restarted || stats.restarted
	s.call.nodeWallMs += stats.nodeWallMs
	s.call.nodeCPUUserUs += stats.nodeCPUUserUs
	s.call.nodeCPUSystemUs += stats.nodeCPUSystemUs
	if stats.nodeVersion != "" {
		s.call.nodeVersion = stats.nodeVersion
	}
}

func (s *sidecarBuildState) applyTo(prepared *preparedTransformerProgram) {
	if s == nil || prepared == nil {
		return
	}
	prepared.sidecarWaitDuration = s.call.wait
	prepared.sidecarPrepDuration = s.call.prep
	prepared.sidecarRoundTripDuration = s.call.roundTrip
	prepared.sidecarDecodeDuration = s.call.decode
	prepared.sidecarRequestBytes = s.call.requestBytes
	prepared.sidecarResponseBytes = s.call.responseBytes
	prepared.sidecarStats = s.call.stats
	prepared.sidecarReads = s.call.reads
	prepared.sidecarChangedFiles = s.call.changedFiles
	prepared.sidecarSpawned = s.call.spawned
	prepared.sidecarRestarted = s.call.restarted
	prepared.nodeWallMs = s.call.nodeWallMs
	prepared.nodeCPUUserUs = s.call.nodeCPUUserUs
	prepared.nodeCPUSystemUs = s.call.nodeCPUSystemUs
	if s.call.nodeVersion != "" {
		prepared.nodeVersion = s.call.nodeVersion
	}
	if s.call.roundTrip > 0 || s.call.prep > 0 || s.call.wait > 0 {
		prepared.sidecarRoundTripRecorded = true
	}
}

var (
	sidecarRegistryMu sync.Mutex
	sidecarSlots      = map[string]*sidecarSlot{}
)

func sidecarSlotFor(key string) *sidecarSlot {
	sidecarRegistryMu.Lock()
	defer sidecarRegistryMu.Unlock()
	slot := sidecarSlots[key]
	if slot == nil {
		slot = &sidecarSlot{}
		sidecarSlots[key] = slot
	}
	return slot
}

func sidecarSlotCount() int {
	sidecarRegistryMu.Lock()
	defer sidecarRegistryMu.Unlock()
	return len(sidecarSlots)
}

// sidecarStderrTail collects the worker's stderr (plugin console output —
// Flamework logs there after the stdout-protocol redirect) for two readers:
// drainTo forwards new lines to the compiler log, and String keeps a tail
// for error reporting. The reader goroutine only buffers; logservice is not
// goroutine-safe, so forwarding happens on the calling goroutine after each
// round trip.
type sidecarStderrTail struct {
	mu      sync.Mutex
	tail    []string
	pending []string
}

func newSidecarStderrTail(pipe io.Reader) *sidecarStderrTail {
	t := &sidecarStderrTail{}
	go func() {
		scanner := bufio.NewScanner(pipe)
		scanner.Buffer(make([]byte, 64*1024), 1024*1024)
		for scanner.Scan() {
			line := scanner.Text()
			t.mu.Lock()
			t.tail = append(t.tail, line)
			if len(t.tail) > 50 {
				t.tail = t.tail[len(t.tail)-50:]
			}
			t.pending = append(t.pending, line)
			t.mu.Unlock()
		}
	}()
	return t
}

// drainTo writes lines buffered since the last drain to the compiler log.
func (t *sidecarStderrTail) drainTo() {
	t.mu.Lock()
	pending := t.pending
	t.pending = nil
	t.mu.Unlock()
	for _, line := range pending {
		logservice.WriteLine(line)
	}
}

func (t *sidecarStderrTail) String() string {
	t.mu.Lock()
	defer t.mu.Unlock()
	return strings.Join(t.tail, "\n")
}

func spawnSidecarSession(dir, sidecarDir string) (*sidecarSession, error) {
	nodeCommand := os.Getenv("ROTOR_NODE_PATH")
	if nodeCommand != "" {
		if _, err := os.Stat(nodeCommand); err != nil {
			return nil, errors.New("node executable not found; rotor transformer plugins require Node.js on PATH")
		}
	} else {
		nodeCommand = "node"
	}
	nodePath, err := exec.LookPath(nodeCommand)
	if err != nil {
		return nil, errors.New("node executable not found; rotor transformer plugins require Node.js on PATH")
	}

	cmd := exec.Command(nodePath, filepath.Join(sidecarDir, "main.js"))
	cmd.Dir = filepath.FromSlash(dir)
	cmd.Env = sidecarEnv(dir, sidecarDir)

	stderrPipe, err := cmd.StderrPipe()
	if err != nil {
		return nil, err
	}
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, err
	}
	if err := cmd.Start(); err != nil {
		return nil, err
	}
	return &sidecarSession{
		cmd:      cmd,
		stdin:    stdin,
		stdout:   bufio.NewReader(stdout),
		stderr:   newSidecarStderrTail(stderrPipe),
		stamps:   map[string]sidecarFileStamp{},
		overlaid: map[string]string{},
	}, nil
}

type sidecarRoundTripResult struct {
	line []byte
	err  error
}

func (s *sidecarSession) roundTrip(ctx context.Context, request sidecarRequest) (*sidecarResponse, error) {
	payload, err := json.Marshal(request)
	if err != nil {
		return nil, s.fail(err)
	}
	line, err := s.writeAndRead(ctx, payload)
	if err != nil {
		return nil, err
	}
	var response sidecarResponse
	if err := json.Unmarshal(bytes.TrimSpace(line), &response); err != nil {
		return nil, s.fail(err)
	}
	return &response, nil
}

func (s *sidecarSession) writeAndRead(ctx context.Context, payload []byte) ([]byte, error) {
	result := make(chan sidecarRoundTripResult, 1)
	go func() {
		_, err := s.stdin.Write(append(payload, '\n'))
		if err != nil {
			result <- sidecarRoundTripResult{err: err}
			return
		}
		line, err := s.stdout.ReadBytes('\n')
		result <- sidecarRoundTripResult{line: line, err: err}
	}()

	select {
	case response := <-result:
		if response.err != nil {
			return nil, s.fail(response.err)
		}
		return response.line, nil
	case <-ctx.Done():
		return nil, s.fail(fmt.Errorf("transformer sidecar response timed out: %w", ctx.Err()))
	}
}

func (s *sidecarSession) fail(err error) error {
	s.dead = true
	if tail := s.stderr.String(); tail != "" {
		return fmt.Errorf("transformer sidecar failed: %w: %s", err, strings.TrimSpace(tail))
	}
	return err
}

func sidecarResponseTimeout() (time.Duration, error) {
	configured := os.Getenv(sidecarResponseTimeoutEnv)
	if configured == "" {
		return defaultSidecarResponseTimeout, nil
	}
	timeout, err := time.ParseDuration(configured)
	if err != nil || timeout <= 0 {
		return 0, fmt.Errorf("invalid %s value %q", sidecarResponseTimeoutEnv, configured)
	}
	return timeout, nil
}

func (s *sidecarSession) close() {
	_ = s.stdin.Close()
	if s.cmd.Process != nil {
		done := make(chan struct{})
		go func() { _ = s.cmd.Wait(); close(done) }()
		select {
		case <-done:
		case <-time.After(2 * time.Second):
			_ = s.cmd.Process.Kill()
			<-done
		}
	}
}

// changedFilesFor stat-diffs the program's project files against the
// session's last-seen stamps. Fresh sessions only record stamps (the worker
// reads from disk); warm sessions ship new text so the LanguageService
// snapshot versions advance (upstream updateFile semantics).
//
// Overlaid files are outside all of that. An overlay exists nowhere on disk,
// so a stat says nothing about it and a worker left to read disk would answer
// on text the caller never sent. Each one ships on every round trip, fresh
// session or not, and the stat-diff skips it.
//
// Overlays are keyed by the caller's spelling, so they are matched to fileNames
// through normalizeOverlayPath rather than compared directly.
func (s *sidecarSession) changedFilesFor(fileNames []string, overlays map[string]string) ([]sidecarChangedFile, error) {
	changed, _, err := s.collectChangedFiles(fileNames, overlays, false)
	return changed, err
}

func (s *sidecarSession) collectChangedFiles(fileNames []string, overlays map[string]string, skipDiskScan bool) ([]sidecarChangedFile, sidecarCallStats, error) {
	var ioStats sidecarCallStats
	// An empty stamp map still means "no round trip yet". Overlaid files skip
	// the stat-diff, but revertDroppedOverlays stamps each one as it hands the
	// file back, so a session cannot reach a second round trip having stamped
	// nothing and still owe the worker a disk edit.
	fresh := len(s.stamps) == 0
	caseSensitive := osvfs.FS().UseCaseSensitiveFileNames()
	overlaid := make(map[string]sidecarChangedFile, len(overlays))
	for path, text := range overlays {
		key := normalizeOverlayPath(path, caseSensitive)
		overlaid[key] = sidecarChangedFile{FileName: filepath.FromSlash(path), Text: text}
	}

	changed, revertReads, revertStats := s.revertDroppedOverlays(overlaid)
	ioStats.reads += revertReads
	ioStats.stats += revertStats
	for _, key := range slices.Sorted(maps.Keys(overlaid)) {
		changed = append(changed, overlaid[key])
		s.overlaid[key] = overlaid[key].FileName
	}

	if skipDiskScan {
		ioStats.changedFiles = len(changed)
		return changed, ioStats, nil
	}

	// Keying every project file to answer a lookup that cannot hit costs a
	// Clean, a FromSlash and (off a case-sensitive filesystem) a ToLower per
	// file, so a build with no overlays skips it.
	overlayAware := len(overlaid) > 0
	for _, fileName := range fileNames {
		path := filepath.FromSlash(fileName)
		if overlayAware {
			key := normalizeOverlayPath(path, caseSensitive)
			if _, ok := overlaid[key]; ok {
				// The program's spelling, not the caller's, so the stamp a
				// revert writes is the one this loop later looks up.
				s.overlaid[key] = path
				continue
			}
		}
		info, err := os.Stat(path)
		ioStats.stats++
		if err != nil {
			continue
		}
		stamp := sidecarFileStamp{modTime: info.ModTime(), size: info.Size()}
		if prev, ok := s.stamps[path]; !fresh && (!ok || prev != stamp) {
			text, err := os.ReadFile(path)
			ioStats.reads++
			if err != nil {
				return nil, ioStats, err
			}
			changed = append(changed, sidecarChangedFile{FileName: path, Text: string(text)})
		}
		s.stamps[path] = stamp
	}
	ioStats.changedFiles = len(changed)
	return changed, ioStats, nil
}

// revertDroppedOverlays undoes the overlays this round trip no longer carries,
// by resending each file's disk text.
//
// The worker's override map outlives the request that filled it, so without
// this a file overlaid once stays overlaid for every later build in the
// process. Resending the disk text puts the override back where the worker
// would have read it from anyway. Stamping the file at the same time keeps the
// stat-diff below from sending it a second time.
//
// Known limitation: a file overlaid and then DELETED from disk keeps its
// override in the worker for the session's lifetime. There is nothing to resend
// and the protocol carries no way to forget a file, so rotor only drops its own
// stamp. Unreachable from `rotor diagnostics`, which sets its overlays once per
// process; it would need a watch session that both overlays and deletes.
//
// overlaid is this round trip's overlays, keyed the way normalizeOverlayPath
// keys them.
func (s *sidecarSession) revertDroppedOverlays(overlaid map[string]sidecarChangedFile) ([]sidecarChangedFile, int, int) {
	changed := []sidecarChangedFile{}
	reads := 0
	stats := 0
	for _, key := range slices.Sorted(maps.Keys(s.overlaid)) {
		if _, ok := overlaid[key]; ok {
			continue
		}
		path := s.overlaid[key]
		delete(s.overlaid, key)

		text, err := os.ReadFile(path)
		reads++
		info, statErr := os.Stat(path)
		stats++
		if err != nil || statErr != nil {
			delete(s.stamps, path)
			continue
		}
		changed = append(changed, sidecarChangedFile{FileName: path, Text: string(text)})
		s.stamps[path] = sidecarFileStamp{modTime: info.ModTime(), size: info.Size()}
	}
	return changed, reads, stats
}

func runTransformerSidecar(dir, configPath string, compileFiles, stampFiles []*ast.SourceFile, overlays map[string]string, plugins []json.RawMessage, mode sidecarEmitMode, state *sidecarBuildState) (*sidecarResponse, sidecarCallStats, error) {
	var stats sidecarCallStats
	sidecarDir, err := resolveSidecarDir()
	if err != nil {
		return nil, stats, err
	}
	timeout, err := sidecarResponseTimeout()
	if err != nil {
		return nil, stats, err
	}

	key := normalizeSourceFilePath(dir) + "|" + normalizeSourceFilePath(configPath)
	slot := sidecarSlotFor(key)
	waitStarted := time.Now()
	slot.mu.Lock()
	stats.wait = time.Since(waitStarted)
	defer slot.mu.Unlock()

	for attempt := 0; ; attempt++ {
		prepStarted := time.Now()
		session := slot.session
		if session == nil || session.dead {
			if session != nil {
				session.close()
			}
			session, err = spawnSidecarSession(dir, sidecarDir)
			if err != nil {
				return nil, stats, nodeRequirementError(err, configPath, plugins)
			}
			slot.session = session
			if attempt == 0 {
				stats.spawned = true
			} else {
				stats.restarted = true
			}
		}

		stampNames := make([]string, 0, len(stampFiles))
		for _, sourceFile := range stampFiles {
			stampNames = append(stampNames, sourceFile.FileName())
		}
		sidecarOverlays, overlayReads := mergeSidecarOverlays(compileFiles, overlays, state, mode.transformSources)
		stats.reads += overlayReads
		skipDiskScan := state != nil && state.diskScanned && len(session.stamps) > 0
		changedFiles, ioStats, err := session.collectChangedFiles(stampNames, sidecarOverlays, skipDiskScan)
		stats.stats += ioStats.stats
		stats.reads += ioStats.reads
		stats.changedFiles += ioStats.changedFiles
		if err != nil {
			return nil, stats, err
		}

		request := sidecarRequest{
			Protocol:         1,
			TsConfigPath:     filepath.FromSlash(configPath),
			ProjectDir:       filepath.FromSlash(dir),
			CompileFileNames: make([]string, 0, len(compileFiles)),
			ChangedFiles:     changedFiles,
			Plugins:          plugins,
			TransformSources: mode.transformSources,
			EmitDeclarations: mode.emitDeclarations,
		}
		for _, sourceFile := range compileFiles {
			request.CompileFileNames = append(request.CompileFileNames, filepath.FromSlash(sourceFile.FileName()))
		}
		payload, err := json.Marshal(request)
		stats.prep += time.Since(prepStarted)
		if err != nil {
			return nil, stats, err
		}
		stats.requestBytes += int64(len(payload))

		ctx, cancel := context.WithTimeout(context.Background(), timeout)
		roundStarted := time.Now()
		line, err := session.writeAndRead(ctx, payload)
		stats.roundTrip += time.Since(roundStarted)
		cancel()
		session.stderr.drainTo()
		if err != nil {
			session.close()
			slot.session = nil
			if errors.Is(err, context.DeadlineExceeded) {
				return nil, stats, err
			}
			if attempt == 0 {
				continue
			}
			return nil, stats, err
		}
		if state != nil {
			state.diskScanned = true
		}
		stats.responseBytes += int64(len(line))
		decodeStarted := time.Now()
		var response sidecarResponse
		if err := json.Unmarshal(bytes.TrimSpace(line), &response); err != nil {
			return nil, stats, session.fail(err)
		}
		stats.decode += time.Since(decodeStarted)
		if response.Metrics != nil {
			stats.nodeWallMs = response.Metrics.WallMs
			stats.nodeCPUUserUs = response.Metrics.CPUUserUs
			stats.nodeCPUSystemUs = response.Metrics.CPUSystemUs
			stats.nodeVersion = response.Metrics.NodeVersion
		}
		return &response, stats, nil
	}
}

func nodeRequirementError(err error, configPath string, plugins []json.RawMessage) error {
	if !strings.Contains(err.Error(), "node executable not found") {
		return err
	}
	names := make([]string, 0, len(plugins))
	for _, plugin := range plugins {
		var config struct {
			Transform string `json:"transform"`
		}
		if json.Unmarshal(plugin, &config) == nil && config.Transform != "" {
			names = append(names, config.Transform)
		}
	}
	if plugins == nil {
		names, _ = configFileTransformerPlugins(normalizeSourceFilePath(configPath))
	}
	if len(names) == 1 {
		return fmt.Errorf("node executable not found; external transformer %q requires Node.js on PATH", names[0])
	}
	return err
}

func closeSidecarSessions() {
	sidecarRegistryMu.Lock()
	slots := make([]*sidecarSlot, 0, len(sidecarSlots))
	for _, slot := range sidecarSlots {
		slots = append(slots, slot)
	}
	sidecarSlots = map[string]*sidecarSlot{}
	sidecarRegistryMu.Unlock()
	for _, slot := range slots {
		slot.mu.Lock()
		if slot.session != nil {
			slot.session.close()
			slot.session = nil
		}
		slot.mu.Unlock()
	}
}

func mergeSidecarOverlays(compileFiles []*ast.SourceFile, overlays map[string]string, state *sidecarBuildState, includeStageOverlays bool) (map[string]string, int) {
	sidecarOverlays := maps.Clone(overlays)
	if sidecarOverlays == nil {
		sidecarOverlays = map[string]string{}
	}
	if state != nil {
		if includeStageOverlays {
			for path, text := range state.stageOverlays {
				if _, ok := overlayByPath(sidecarOverlays, path); ok {
					continue
				}
				sidecarOverlays[path] = text
			}
		}
		if state.diskScanned {
			return sidecarOverlays, 0
		}
	}
	reads := 0
	for _, sourceFile := range compileFiles {
		if _, ok := overlayByPath(sidecarOverlays, sourceFile.FileName()); ok {
			continue
		}
		onDisk, readErr := os.ReadFile(sourceFile.FileName())
		reads++
		if readErr != nil || sourceFile.Text() == string(onDisk) {
			continue
		}
		sidecarOverlays[sourceFile.FileName()] = sourceFile.Text()
	}
	return sidecarOverlays, reads
}

func overlayByPath(overlays map[string]string, path string) (string, bool) {
	if text, ok := overlays[path]; ok {
		return text, true
	}
	caseSensitive := osvfs.FS().UseCaseSensitiveFileNames()
	key := normalizeOverlayPath(path, caseSensitive)
	for existing, text := range overlays {
		if normalizeOverlayPath(existing, caseSensitive) == key {
			return text, true
		}
	}
	return "", false
}

func sidecarEnv(projectDir, sidecarDir string) []string {
	nodePaths := []string{
		filepath.Join(filepath.FromSlash(projectDir), "node_modules"),
		filepath.Join(sidecarDir, "node_modules"),
	}

	var filtered []string
	seen := map[string]struct{}{}
	for _, path := range nodePaths {
		if path == "" {
			continue
		}
		path = filepath.Clean(path)
		if _, err := os.Stat(path); err != nil {
			continue
		}
		key := path
		if runtime.GOOS == "windows" {
			key = strings.ToLower(key)
		}
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}
		filtered = append(filtered, path)
	}

	env := os.Environ()
	if len(filtered) == 0 {
		return env
	}

	nodePathValue := strings.Join(filtered, string(os.PathListSeparator))
	for i, entry := range env {
		if strings.HasPrefix(entry, "NODE_PATH=") {
			if existing := strings.TrimPrefix(entry, "NODE_PATH="); existing != "" {
				nodePathValue += string(os.PathListSeparator) + existing
			}
			env[i] = "NODE_PATH=" + nodePathValue
			return env
		}
	}
	return append(env, "NODE_PATH="+nodePathValue)
}

func newProjectProgramWithOverlay(projectDir, tsConfigPath string, overlays map[string]string, checkers *int, singleThreaded *bool) (*compiler.Program, []string, error) {
	dir := filepath.ToSlash(projectDir)
	if abs, err := filepath.Abs(projectDir); err == nil {
		dir = filepath.ToSlash(abs)
	}
	configPath := filepath.ToSlash(tsConfigPath)
	if abs, err := filepath.Abs(tsConfigPath); err == nil {
		configPath = filepath.ToSlash(abs)
	}

	fs := newOverlayFS(osvfs.FS(), configPath, overlays)
	return newProjectProgramFromFSWithOptions(dir, configPath, fs, checkers, singleThreaded)
}

func singleThreadedFromOptions(options *core.CompilerOptions) *bool {
	if options == nil || options.SingleThreaded == core.TSUnknown {
		return nil
	}
	value := options.SingleThreaded.IsTrue()
	return &value
}

func newProjectProgramFromFS(dir, configPath string, fs vfs.FS) (*compiler.Program, []string, error) {
	return newProjectProgramFromFSWithOptions(dir, configPath, fs, nil, nil)
}

func newProjectProgramFromFSWithOptions(dir, configPath string, fs vfs.FS, checkers *int, singleThreaded *bool) (*compiler.Program, []string, error) {
	// rotor extension: serve the synthetic $env ambient declaration from an
	// in-memory file next to the tsconfig (see envdecl.go for the parity
	// rationale) ...
	declPath := envDeclPath(configPath)
	fs = injectEnvDeclFS(fs, declPath)

	// ... and likewise the synthetic $asset ambient declaration (assetdecl.go).
	assetDecl := assetDeclPath(configPath)
	fs = injectAssetDeclFS(fs, assetDecl)

	// ... and the shared synthetic declaration for the $nameof / $keys / $file /
	// $git / $buildTime macros (macrodecl.go).
	macroDecl := macroDeclPath(configPath)
	fs = injectMacroDeclFS(fs, macroDecl)

	host := compiler.NewCompilerHost(dir, fs, bundled.LibPath(), nil, nil)
	parsed, configDiags := tsoptions.GetParsedCommandLineOfConfigFile(configPath, nil, nil, host, nil)
	if len(configDiags) > 0 {
		return nil, diagnosticStrings(configDiags), errors.New("compile: tsconfig.json has errors")
	}
	ApplyCheckerOverride(parsed.CompilerOptions(), checkers)
	ApplySingleThreadedOverride(parsed.CompilerOptions(), singleThreaded)

	raw := readRawEnforcedOptions(filepath.FromSlash(configPath))
	if msg := validateCompilerOptions(parsed.CompilerOptions(), dir, raw); msg != "" {
		return nil, []string{msg}, errors.New("compile: invalid tsconfig.json configuration")
	}

	// ... and append it to the program's root files AFTER config parse so
	// the config-derived file set (and its order) is untouched. Skipped when
	// the project already includes an on-disk rotor-env.d.ts (the generated
	// editor companion): appending the synthetic declaration as well would be
	// a duplicate-identifier error (see projectDeclaresEnvOnDisk).
	if !projectDeclaresEnvOnDisk(fs, parsed.ParsedConfig.FileNames) {
		parsed.ParsedConfig.FileNames = append(parsed.ParsedConfig.FileNames, declPath)
	}
	// Likewise for $asset (skipped when an identical on-disk rotor-asset.d.ts
	// is already a root file — see projectDeclaresAssetOnDisk).
	if !projectDeclaresAssetOnDisk(fs, parsed.ParsedConfig.FileNames) {
		parsed.ParsedConfig.FileNames = append(parsed.ParsedConfig.FileNames, assetDecl)
	}
	// Likewise for the shared $nameof / $keys / $file / $git / $buildTime
	// declaration (skipped when an identical on-disk rotor-macros.d.ts is
	// already a root file — see projectDeclaresMacrosOnDisk).
	if !projectDeclaresMacrosOnDisk(fs, parsed.ParsedConfig.FileNames) {
		parsed.ParsedConfig.FileNames = append(parsed.ParsedConfig.FileNames, macroDecl)
	}

	return compiler.NewProgram(compiler.ProgramOptions{
		Host:   host,
		Config: parsed,
	}), nil, nil
}

func remapProgramSourceFiles(program *compiler.Program, sourceFiles []*ast.SourceFile) ([]*ast.SourceFile, error) {
	byPath := make(map[string]*ast.SourceFile)
	for _, sourceFile := range projectSourceFiles(program) {
		byPath[normalizeSourceFilePath(sourceFile.FileName())] = sourceFile
	}

	remapped := make([]*ast.SourceFile, 0, len(sourceFiles))
	for _, sourceFile := range sourceFiles {
		path := normalizeSourceFilePath(sourceFile.FileName())
		mapped := byPath[path]
		if mapped == nil {
			return nil, fmt.Errorf("compile: transformed source file missing from overlay program: %s", path)
		}
		remapped = append(remapped, mapped)
	}
	return remapped, nil
}

// projectUsesTransformerPlugins reports whether this project has a transformer
// to run — the gate on spawning the Node sidecar at all.
//
// `plugins` is an array-valued compiler option, so `extends` REPLACES it rather
// than merging: whichever config in the chain declares `plugins` last settles
// the list, and `"plugins": []` drops an inherited transform entirely. That is
// what `tsc --showConfig` reports, and it is what the sidecar runs (tools/
// sidecar/lib/plugins.js). The project's own config therefore answers on its
// own whenever it declares `plugins`, whatever its ancestors say.
//
// When it stays silent the list is inherited, and this cannot resolve it
// exactly: tsgo drops `plugins` while parsing options (a language-service
// option with no CompilerOptions field) and reports ExtendedSourceFiles sorted
// by path rather than in chain order, so the nearest declaring ancestor is not
// identifiable here. Asking whether ANY ancestor declares a transform is exact
// unless two levels of one chain disagree, where it over-approximates: the
// sidecar starts, resolves the list properly, and finds nothing to run. The
// cost is a wasted worker, never a dropped transform.
func projectUsesTransformerPlugins(parsed *tsoptions.ParsedCommandLine) bool {
	if parsed == nil {
		return false
	}
	if configName := parsed.ConfigName(); configName != "" {
		if declaresTransform, declared := configFilePluginsDeclaration(normalizeSourceFilePath(configName)); declared {
			return declaresTransform
		}
	}

	seen := map[string]struct{}{}
	for _, configPath := range parsed.ExtendedSourceFiles() {
		if configPath == "" {
			continue
		}
		path := normalizeSourceFilePath(configPath)
		if _, ok := seen[path]; ok {
			continue
		}
		seen[path] = struct{}{}
		if declaresTransform, _ := configFilePluginsDeclaration(path); declaresTransform {
			return true
		}
	}
	return false
}

// configFilePluginsDeclaration reports whether configPath's own
// `compilerOptions.plugins` names a transform, and whether it declares the key
// at all. The two answers differ for `"plugins": []`: declared, no transform —
// an override that replaces whatever the config extends. A key that is present
// but not an array is left undeclared so the caller falls back to the chain;
// tsgo reports the malformed value as a config error of its own.
func configFilePluginsDeclaration(configPath string) (declaresTransform bool, declared bool) {
	plugins, declared := configFileTransformerPlugins(configPath)
	return len(plugins) > 0, declared
}

func configFileTransformerPlugins(configPath string) ([]string, bool) {
	data, err := os.ReadFile(configPath)
	if err != nil {
		return nil, false
	}

	var root map[string]any
	if json.Unmarshal([]byte(stripJSONC(string(data))), &root) != nil {
		return nil, false
	}
	compilerOptions, ok := root["compilerOptions"].(map[string]any)
	if !ok {
		return nil, false
	}
	plugins, ok := compilerOptions["plugins"].([]any)
	if !ok {
		return nil, false
	}
	transforms := make([]string, 0, len(plugins))
	for _, plugin := range plugins {
		if pluginConfig, ok := plugin.(map[string]any); ok {
			if transform, ok := pluginConfig["transform"].(string); ok && transform != "" {
				transforms = append(transforms, transform)
			}
		}
	}
	return transforms, true
}

func formatSidecarDiagnostic(diag sidecarDiagnostic) string {
	if diag.File != "" {
		return filepath.FromSlash(diag.File) + ": " + diag.Message
	}
	return diag.Message
}

func normalizeOverlayPath(path string, caseSensitive bool) string {
	path = normalizeSourceFilePath(path)
	if !caseSensitive {
		path = strings.ToLower(path)
	}
	return path
}
