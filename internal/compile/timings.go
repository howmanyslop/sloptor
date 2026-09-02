package compile

import (
	"context"
	"path/filepath"
	"runtime"
	"runtime/debug"
	"runtime/pprof"
	"runtime/trace"
	"sync"
	"time"

	"rotor/internal/logservice"
)

const BuildTimingSchemaVersion = 2

// StageSemanticsWorkMs is the schema 2 invariant: top-level totalMs is elapsed
// wall time, while stage values are aggregate work milliseconds. Concurrent
// projects can make the stage sum exceed totalMs.
const StageSemanticsWorkMs = "workMs"

const (
	ProjectTimingStatusBuilt   = "built"
	ProjectTimingStatusSkipped = "skipped"
	ProjectTimingStatusBlocked = "blocked"
	ProjectTimingStatusFailed  = "failed"
)

type BuildTimings struct {
	SchemaVersion  int                   `json:"schemaVersion"`
	OK             bool                  `json:"ok"`
	TotalMs        int64                 `json:"totalMs"`
	StageSemantics string                `json:"stageSemantics"`
	Stages         BuildTimingStages     `json:"stages"`
	Counts         BuildTimingCounts     `json:"counts"`
	Projects       []ProjectBuildTimings `json:"projects,omitempty"`
	Metadata       BuildTimingMetadata   `json:"metadata"`

	mu                       sync.Mutex
	started                  time.Time
	finished                 bool
	parent                   *BuildTimings
	ctx                      context.Context
	configPath               string
	status                   string
	blockedBy                string
	buildWallMs              int64
	projectIndex             map[string]int
	preparedDirectories      map[string]struct{}
	sidecarRoundTripRecorded bool
	overlayProgramRecorded   bool
}

type ProjectBuildTimings struct {
	ConfigPath  string            `json:"configPath"`
	Status      string            `json:"status"`
	BlockedBy   string            `json:"blockedBy,omitempty"`
	BuildWallMs int64             `json:"buildWallMs"`
	Stages      BuildTimingStages `json:"stages"`
	Counts      BuildTimingCounts `json:"counts"`
}

type BuildTimingMetadata struct {
	Version               string `json:"version,omitempty"`
	Revision              string `json:"revision,omitempty"`
	Dirty                 bool   `json:"dirty,omitempty"`
	GoVersion             string `json:"goVersion,omitempty"`
	GOOS                  string `json:"goos,omitempty"`
	GOARCH                string `json:"goarch,omitempty"`
	GOMAXPROCS            int    `json:"gomaxprocs,omitempty"`
	GOGC                  int    `json:"gogc,omitempty"`
	MemoryLimit           int64  `json:"memoryLimit,omitempty"`
	RequestedBuilders     *int   `json:"requestedBuilders,omitempty"`
	RequestedCheckers     *int   `json:"requestedCheckers,omitempty"`
	EffectiveBuilders     int    `json:"effectiveBuilders,omitempty"`
	EffectiveWriteWorkers int    `json:"effectiveWriteWorkers,omitempty"`
	NodeVersion           string `json:"nodeVersion,omitempty"`
}

type BuildTimingStages struct {
	InitialProgramMs        int64 `json:"initialProgramMs"`
	IncrementalSelectionMs  int64 `json:"incrementalSelectionMs"`
	CleanupMs               int64 `json:"cleanupMs"`
	IncludeCopyMs           int64 `json:"includeCopyMs"`
	NonCompiledCopyMs       int64 `json:"nonCompiledCopyMs"`
	SidecarSessionWaitMs    int64 `json:"sidecarSessionWaitMs"`
	SidecarPreparationMs    int64 `json:"sidecarPreparationMs"`
	SidecarRoundTripMs      int64 `json:"sidecarRoundTripMs"`
	SidecarResponseDecodeMs int64 `json:"sidecarResponseDecodeMs"`
	OverlayProgramMs        int64 `json:"overlayProgramMs"`
	ProjectContextMs        int64 `json:"projectContextMs"`
	SemanticDiagnosticsMs   int64 `json:"semanticDiagnosticsMs"`
	NativeTransformRenderMs int64 `json:"nativeTransformRenderMs"`
	CompiledOutputWritesMs  int64 `json:"compiledOutputWritesMs"`
	DeclarationEmitWritesMs int64 `json:"declarationEmitWritesMs"`
	IncrementalManifestMs   int64 `json:"incrementalManifestMs"`
	PersistenceMs           int64 `json:"persistenceMs"`
}

type BuildTimingCounts struct {
	TotalSources               int   `json:"totalSources"`
	SelectedSources            int   `json:"selectedSources"`
	EmittedEntries             int   `json:"emittedEntries"`
	ScheduledSourceMapWrites   int   `json:"scheduledSourceMapWrites"`
	ScheduledDeclarationWrites int   `json:"scheduledDeclarationWrites"`
	ActualWrites               int   `json:"actualWrites"`
	HashSkips                  int   `json:"hashSkips"`
	UniquePreparedDirectories  int   `json:"uniquePreparedDirectories"`
	EffectiveWriteWorkers      int   `json:"effectiveWriteWorkers"`
	SidecarRequestBytes        int64 `json:"sidecarRequestBytes,omitempty"`
	SidecarResponseBytes       int64 `json:"sidecarResponseBytes,omitempty"`
	SidecarStats               int   `json:"sidecarStats,omitempty"`
	SidecarSourceReads         int   `json:"sidecarSourceReads,omitempty"`
	SidecarChangedFiles        int   `json:"sidecarChangedFiles,omitempty"`
	SidecarSpawns              int   `json:"sidecarSpawns,omitempty"`
	SidecarRestarts            int   `json:"sidecarRestarts,omitempty"`
	NodeWallMs                 int64 `json:"nodeWallMs,omitempty"`
	NodeCPUUserUs              int64 `json:"nodeCpuUserUs,omitempty"`
	NodeCPUSystemUs            int64 `json:"nodeCpuSystemUs,omitempty"`
	ParseCacheHits             int64 `json:"parseCacheHits,omitempty"`
	ParseCacheMisses           int64 `json:"parseCacheMisses,omitempty"`
}

func (timings *BuildTimings) addParseCacheCounts(hits, misses int64) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Counts.ParseCacheHits += hits
	timings.Counts.ParseCacheMisses += misses
}

func (timings *BuildTimings) setHashSkips(skips int) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Counts.HashSkips = skips
}

type buildTimingStage uint8

const (
	initialProgramStage buildTimingStage = iota
	incrementalSelectionStage
	cleanupStage
	includeCopyStage
	nonCompiledCopyStage
	sidecarSessionWaitStage
	sidecarPreparationStage
	sidecarRoundTripStage
	sidecarResponseDecodeStage
	overlayProgramStage
	projectContextStage
	semanticDiagnosticsStage
	nativeTransformRenderStage
	compiledOutputWritesStage
	declarationEmitWritesStage
	incrementalManifestStage
	persistenceStage
)

func NewBuildTimings() *BuildTimings {
	return &BuildTimings{
		SchemaVersion:       BuildTimingSchemaVersion,
		StageSemantics:      StageSemanticsWorkMs,
		started:             time.Now(),
		preparedDirectories: map[string]struct{}{},
		Metadata:            captureRuntimeMetadata(),
	}
}

func (timings *BuildTimings) context() context.Context {
	if timings != nil && timings.ctx != nil {
		return timings.ctx
	}
	return context.Background()
}

func (timings *BuildTimings) projectLabel() string {
	if timings == nil {
		return ""
	}
	return projectLabel(timings.configPath)
}

func projectLabel(configPath string) string {
	if configPath == "" {
		return ""
	}
	return filepath.Base(filepath.Dir(configPath))
}

// logStage emits the verbose start and completion lines for work whose
// duration reaches BuildTimings later through
// recordPreparedTransformerProgram; the returned stop reports the elapsed
// time so callers keep a single measurement.
func logStage(configPath string, stage buildTimingStage) func() time.Duration {
	return logStageNamed(configPath, stage.traceName())
}

// logStageNamed is logStage under a caller-supplied name, for stages that
// carry a detail suffix such as the transformer plugins a sidecar round trip
// runs.
func logStageNamed(configPath, name string) func() time.Duration {
	label := projectLabel(configPath)
	logservice.WriteStageStartIfVerbose(label, name)
	started := time.Now()
	return func() time.Duration {
		elapsed := time.Since(started)
		logservice.WriteStageDoneIfVerbose(label, name, elapsed)
		return elapsed
	}
}

func (timings *BuildTimings) startStage(stage buildTimingStage) func() {
	ctx := context.Background()
	label := ""
	if timings != nil {
		ctx = timings.context()
		label = timings.projectLabel()
	}
	name := stage.traceName()
	logservice.WriteStageStartIfVerbose(label, name)
	ctx = pprof.WithLabels(ctx, pprof.Labels("stage", name))
	if timings != nil && timings.configPath != "" {
		ctx = pprof.WithLabels(ctx, pprof.Labels("project", timings.configPath, "stage", name))
	}
	pprof.SetGoroutineLabels(ctx)
	region := trace.StartRegion(ctx, name)
	started := time.Now()
	return func() {
		region.End()
		duration := time.Since(started)
		if timings != nil {
			timings.addStageDuration(stage, duration)
			pprof.SetGoroutineLabels(timings.context())
		} else {
			pprof.SetGoroutineLabels(context.Background())
		}
		logservice.WriteStageDoneIfVerbose(label, name, duration)
	}
}

func (stage buildTimingStage) traceName() string {
	switch stage {
	case initialProgramStage:
		return "program creation and parse/load"
	case incrementalSelectionStage:
		return "incremental selection"
	case cleanupStage:
		return "cleanup"
	case includeCopyStage:
		return "copy include files"
	case nonCompiledCopyStage:
		return "copy non-compiled files"
	case sidecarSessionWaitStage:
		return "transformer sidecar wait"
	case sidecarPreparationStage:
		return "transformer sidecar preparation"
	case sidecarRoundTripStage:
		return "transformer sidecar"
	case sidecarResponseDecodeStage:
		return "transformer sidecar decode"
	case overlayProgramStage:
		return "overlay program creation and parse/load"
	case projectContextStage:
		return "project context"
	case semanticDiagnosticsStage:
		return "semantic diagnostics"
	case nativeTransformRenderStage:
		return "transform/render"
	case compiledOutputWritesStage:
		return "compiled output writes"
	case declarationEmitWritesStage:
		return "declaration emit/write"
	case incrementalManifestStage:
		return "incremental manifest"
	case persistenceStage:
		return "persistence"
	default:
		return "build"
	}
}

func (timings *BuildTimings) addStageDuration(stage buildTimingStage, duration time.Duration) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	timings.addStageLocked(stage, duration)
	parent := timings.parent
	finished := timings.finished
	var snapshot ProjectBuildTimings
	if finished && parent != nil {
		snapshot = timings.snapshotLocked()
	}
	timings.mu.Unlock()
	if parent != nil {
		parent.addStageDuration(stage, duration)
		if finished {
			parent.replaceProjectSnapshot(snapshot)
		}
	}
}

func (timings *BuildTimings) addStageLocked(stage buildTimingStage, duration time.Duration) {
	milliseconds := duration.Milliseconds()
	switch stage {
	case initialProgramStage:
		timings.Stages.InitialProgramMs += milliseconds
	case incrementalSelectionStage:
		timings.Stages.IncrementalSelectionMs += milliseconds
	case cleanupStage:
		timings.Stages.CleanupMs += milliseconds
	case includeCopyStage:
		timings.Stages.IncludeCopyMs += milliseconds
	case nonCompiledCopyStage:
		timings.Stages.NonCompiledCopyMs += milliseconds
	case sidecarSessionWaitStage:
		timings.Stages.SidecarSessionWaitMs += milliseconds
	case sidecarPreparationStage:
		timings.Stages.SidecarPreparationMs += milliseconds
	case sidecarRoundTripStage:
		timings.Stages.SidecarRoundTripMs += milliseconds
	case sidecarResponseDecodeStage:
		timings.Stages.SidecarResponseDecodeMs += milliseconds
	case overlayProgramStage:
		timings.Stages.OverlayProgramMs += milliseconds
	case projectContextStage:
		timings.Stages.ProjectContextMs += milliseconds
	case semanticDiagnosticsStage:
		timings.Stages.SemanticDiagnosticsMs += milliseconds
	case nativeTransformRenderStage:
		timings.Stages.NativeTransformRenderMs += milliseconds
	case compiledOutputWritesStage:
		timings.Stages.CompiledOutputWritesMs += milliseconds
	case declarationEmitWritesStage:
		timings.Stages.DeclarationEmitWritesMs += milliseconds
	case incrementalManifestStage:
		timings.Stages.IncrementalManifestMs += milliseconds
	case persistenceStage:
		timings.Stages.PersistenceMs += milliseconds
	}
}

func (timings *BuildTimings) recordPreparedTransformerProgram(prepared *preparedTransformerProgram) {
	if timings == nil || prepared == nil {
		return
	}
	timings.addStageDuration(sidecarSessionWaitStage, prepared.sidecarWaitDuration)
	timings.addStageDuration(sidecarPreparationStage, prepared.sidecarPrepDuration)
	timings.addStageDuration(sidecarRoundTripStage, prepared.sidecarRoundTripDuration)
	timings.addStageDuration(sidecarResponseDecodeStage, prepared.sidecarDecodeDuration)
	timings.addStageDuration(overlayProgramStage, prepared.overlayProgramDuration)
	timings.mu.Lock()
	timings.sidecarRoundTripRecorded = timings.sidecarRoundTripRecorded || prepared.sidecarRoundTripRecorded
	timings.overlayProgramRecorded = timings.overlayProgramRecorded || prepared.overlayProgramRecorded
	timings.Counts.SidecarRequestBytes += prepared.sidecarRequestBytes
	timings.Counts.SidecarResponseBytes += prepared.sidecarResponseBytes
	timings.Counts.SidecarStats += prepared.sidecarStats
	timings.Counts.SidecarSourceReads += prepared.sidecarReads
	timings.Counts.SidecarChangedFiles += prepared.sidecarChangedFiles
	if prepared.sidecarSpawned {
		timings.Counts.SidecarSpawns++
	}
	if prepared.sidecarRestarted {
		timings.Counts.SidecarRestarts++
	}
	timings.Counts.NodeWallMs += prepared.nodeWallMs
	timings.Counts.NodeCPUUserUs += prepared.nodeCPUUserUs
	timings.Counts.NodeCPUSystemUs += prepared.nodeCPUSystemUs
	if prepared.nodeVersion != "" {
		timings.Metadata.NodeVersion = prepared.nodeVersion
	}
	parent := timings.parent
	recordedRoundTrip := timings.sidecarRoundTripRecorded
	recordedOverlay := timings.overlayProgramRecorded
	nodeVersion := timings.Metadata.NodeVersion
	timings.mu.Unlock()
	if parent != nil {
		parent.mu.Lock()
		parent.sidecarRoundTripRecorded = parent.sidecarRoundTripRecorded || recordedRoundTrip
		parent.overlayProgramRecorded = parent.overlayProgramRecorded || recordedOverlay
		parent.Counts.SidecarRequestBytes += prepared.sidecarRequestBytes
		parent.Counts.SidecarResponseBytes += prepared.sidecarResponseBytes
		parent.Counts.SidecarStats += prepared.sidecarStats
		parent.Counts.SidecarSourceReads += prepared.sidecarReads
		parent.Counts.SidecarChangedFiles += prepared.sidecarChangedFiles
		if prepared.sidecarSpawned {
			parent.Counts.SidecarSpawns++
		}
		if prepared.sidecarRestarted {
			parent.Counts.SidecarRestarts++
		}
		parent.Counts.NodeWallMs += prepared.nodeWallMs
		parent.Counts.NodeCPUUserUs += prepared.nodeCPUUserUs
		parent.Counts.NodeCPUSystemUs += prepared.nodeCPUSystemUs
		if nodeVersion != "" {
			parent.Metadata.NodeVersion = nodeVersion
		}
		parent.mu.Unlock()
	}
}

func (timings *BuildTimings) setSourceCounts(totalSources, selectedSources int) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Counts.TotalSources = totalSources
	timings.Counts.SelectedSources = selectedSources
}

func (timings *BuildTimings) setEffectiveWriteWorkers(workers int) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Counts.EffectiveWriteWorkers = workers
	timings.Metadata.EffectiveWriteWorkers = workers
}

func (timings *BuildTimings) addScheduledSourceMapWrite() {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Counts.ScheduledSourceMapWrites++
}

func (timings *BuildTimings) addScheduledDeclarationWrites(writes int) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Counts.ScheduledDeclarationWrites += writes
}

func (timings *BuildTimings) recordOutputWrite(path string, wrote bool) {
	if timings == nil || !wrote {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Counts.ActualWrites++
	timings.preparedDirectories[filepath.Clean(filepath.Dir(path))] = struct{}{}
	timings.Counts.UniquePreparedDirectories = len(timings.preparedDirectories)
}

func (timings *BuildTimings) setEmittedEntries(entries int) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Counts.EmittedEntries = entries
}

func (timings *BuildTimings) finish() {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	if timings.finished {
		timings.mu.Unlock()
		return
	}
	timings.finished = true
	elapsed := time.Since(timings.started).Milliseconds()
	if timings.parent == nil {
		timings.TotalMs = elapsed
		timings.mu.Unlock()
		return
	}
	timings.buildWallMs = elapsed
	timings.TotalMs = elapsed
	if timings.status == "" {
		timings.status = ProjectTimingStatusBuilt
	}
	snapshot := timings.snapshotLocked()
	parent := timings.parent
	timings.mu.Unlock()
	parent.attachProject(snapshot, timings)
}

func (timings *BuildTimings) SetOK(ok bool) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.OK = ok
}

func (timings *BuildTimings) initProjects(projects []SolutionProject) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Projects = make([]ProjectBuildTimings, len(projects))
	timings.projectIndex = make(map[string]int, len(projects))
	for index, project := range projects {
		timings.Projects[index] = ProjectBuildTimings{ConfigPath: project.ConfigPath}
		timings.projectIndex[project.ConfigPath] = index
	}
}

func (timings *BuildTimings) newProject(configPath string) *BuildTimings {
	if timings == nil {
		return nil
	}
	child := NewBuildTimings()
	child.parent = timings
	child.configPath = configPath
	child.ctx = pprof.WithLabels(context.Background(), pprof.Labels("project", configPath))
	return child
}

func (timings *BuildTimings) setProjectStatus(configPath, status, blockedBy string) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	if timings.parent != nil {
		timings.status = status
		timings.blockedBy = blockedBy
		finished := timings.finished
		snapshot := timings.snapshotLocked()
		parent := timings.parent
		timings.mu.Unlock()
		if finished {
			parent.replaceProjectSnapshot(snapshot)
		}
		return
	}
	index, ok := timings.projectIndex[configPath]
	if ok {
		timings.Projects[index].Status = status
		timings.Projects[index].BlockedBy = blockedBy
		timings.Projects[index].ConfigPath = configPath
	}
	timings.mu.Unlock()
}

func (timings *BuildTimings) setConcurrencyMetadata(builders int, requestedBuilders, requestedCheckers *int) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Metadata.EffectiveBuilders = builders
	timings.Metadata.RequestedBuilders = requestedBuilders
	timings.Metadata.RequestedCheckers = requestedCheckers
}

func (timings *BuildTimings) SetProductVersion(version string) {
	if timings == nil {
		return
	}
	timings.mu.Lock()
	defer timings.mu.Unlock()
	timings.Metadata.Version = version
}

func (timings *BuildTimings) snapshotLocked() ProjectBuildTimings {
	return ProjectBuildTimings{
		ConfigPath:  timings.configPath,
		Status:      timings.status,
		BlockedBy:   timings.blockedBy,
		BuildWallMs: timings.buildWallMs,
		Stages:      timings.Stages,
		Counts:      timings.Counts,
	}
}

func (timings *BuildTimings) attachProject(snapshot ProjectBuildTimings, child *BuildTimings) {
	timings.mu.Lock()
	defer timings.mu.Unlock()
	index, ok := timings.projectIndex[snapshot.ConfigPath]
	if !ok {
		index = len(timings.Projects)
		timings.Projects = append(timings.Projects, snapshot)
		if timings.projectIndex == nil {
			timings.projectIndex = map[string]int{}
		}
		timings.projectIndex[snapshot.ConfigPath] = index
	} else {
		timings.Projects[index] = snapshot
	}
	timings.Counts.TotalSources += snapshot.Counts.TotalSources
	timings.Counts.SelectedSources += snapshot.Counts.SelectedSources
	timings.Counts.EmittedEntries += snapshot.Counts.EmittedEntries
	timings.Counts.ScheduledSourceMapWrites += snapshot.Counts.ScheduledSourceMapWrites
	timings.Counts.ScheduledDeclarationWrites += snapshot.Counts.ScheduledDeclarationWrites
	timings.Counts.ActualWrites += snapshot.Counts.ActualWrites
	timings.Counts.HashSkips += snapshot.Counts.HashSkips
	if timings.Counts.EffectiveWriteWorkers == 0 {
		timings.Counts.EffectiveWriteWorkers = snapshot.Counts.EffectiveWriteWorkers
	}
	if child != nil {
		for dir := range child.preparedDirectories {
			timings.preparedDirectories[dir] = struct{}{}
		}
		timings.Counts.UniquePreparedDirectories = len(timings.preparedDirectories)
		timings.sidecarRoundTripRecorded = timings.sidecarRoundTripRecorded || child.sidecarRoundTripRecorded
		timings.overlayProgramRecorded = timings.overlayProgramRecorded || child.overlayProgramRecorded
	}
}

func (timings *BuildTimings) replaceProjectSnapshot(snapshot ProjectBuildTimings) {
	timings.mu.Lock()
	defer timings.mu.Unlock()
	index, ok := timings.projectIndex[snapshot.ConfigPath]
	if !ok {
		return
	}
	timings.Projects[index] = snapshot
}

func timedPersist(timings *BuildTimings, persist func() error) func() error {
	if persist == nil {
		return nil
	}
	if timings == nil {
		return persist
	}
	return func() error {
		stop := timings.startStage(persistenceStage)
		defer stop()
		return persist()
	}
}

func captureRuntimeMetadata() BuildTimingMetadata {
	meta := BuildTimingMetadata{
		GoVersion:  runtime.Version(),
		GOOS:       runtime.GOOS,
		GOARCH:     runtime.GOARCH,
		GOMAXPROCS: runtime.GOMAXPROCS(0),
	}
	if info, ok := debug.ReadBuildInfo(); ok {
		if info.GoVersion != "" {
			meta.GoVersion = info.GoVersion
		}
		for _, setting := range info.Settings {
			switch setting.Key {
			case "vcs.revision":
				meta.Revision = setting.Value
			case "vcs.modified":
				meta.Dirty = setting.Value == "true"
			}
		}
	}
	gogc := debug.SetGCPercent(-1)
	debug.SetGCPercent(gogc)
	meta.GOGC = gogc
	limit := debug.SetMemoryLimit(-1)
	debug.SetMemoryLimit(limit)
	meta.MemoryLimit = limit
	return meta
}
