package compile

import (
	"bufio"
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"maps"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"slices"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"rotor/tsgo/vfs/osvfs"
)

const (
	sidecarDaemonProtocol    = 2
	sidecarDaemonIdleTimeout = 5 * time.Minute
	sidecarDaemonStartWait   = 5 * time.Second
	sidecarDaemonLockExpiry  = 15 * time.Second
	sidecarResultExpiry      = 30 * time.Minute
	sidecarDaemonRuntimeEnv  = "ROTOR_DAEMON_RUNTIME_DIR"
)

// SidecarDaemonCall is one request for a persistent Node worker. StampFileNames
// and Overlays are complete snapshots for this call: the daemon, rather than a
// short-lived compiler process, owns the file stamps used to derive changedFiles.
type SidecarDaemonCall struct {
	WorkspaceKey         string
	WorkerKey            string
	ProjectDir           string
	SidecarDir           string
	NodePath             string
	ChildEnv             []string
	LeaseOwner           string
	Payload              []byte
	StampFileNames       []string
	Overlays             map[string]string
	InvalidatedFileNames []string
}

// SidecarDaemonResult carries the Node worker's newline-delimited JSON response
// and the disk work the daemon performed while preparing it.
type SidecarDaemonResult struct {
	Payload      []byte
	Stats        int
	Reads        int
	ChangedFiles int
	Spawned      bool
	abandon      func()
}

// SidecarDaemonInfo is the live status of one workspace daemon.
type SidecarDaemonInfo struct {
	ID           string
	PID          int
	StartedAt    time.Time
	LastActiveAt time.Time
	IdleDeadline time.Time
	WorkerCount  int
}

type sidecarDaemonMetadata struct {
	Protocol  int       `json:"protocol"`
	ID        string    `json:"id"`
	PID       int       `json:"pid"`
	Endpoint  string    `json:"endpoint"`
	StartedAt time.Time `json:"startedAt"`
}

type sidecarDaemonMessage struct {
	Protocol             int               `json:"protocol"`
	DaemonID             string            `json:"daemonId"`
	Kind                 string            `json:"kind"`
	WorkerKey            string            `json:"workerKey,omitempty"`
	ProjectDir           string            `json:"projectDir,omitempty"`
	SidecarDir           string            `json:"sidecarDir,omitempty"`
	NodePath             string            `json:"nodePath,omitempty"`
	ChildEnv             []string          `json:"childEnv,omitempty"`
	Payload              json.RawMessage   `json:"payload,omitempty"`
	StampFileNames       []string          `json:"stampFileNames,omitempty"`
	Overlays             map[string]string `json:"overlays,omitempty"`
	InvalidatedFileNames []string          `json:"invalidatedFileNames,omitempty"`
	Deadline             time.Time         `json:"deadline,omitempty"`
	LeaseOwner           string            `json:"leaseOwner,omitempty"`
}

type sidecarDaemonReply struct {
	Protocol int                  `json:"protocol"`
	Payload  json.RawMessage      `json:"payload,omitempty"`
	Error    string               `json:"error,omitempty"`
	Info     *SidecarDaemonInfo   `json:"info,omitempty"`
	IO       sidecarDaemonReplyIO `json:"io,omitempty"`
}

type sidecarDaemonReplyIO struct {
	Stats        int  `json:"stats,omitempty"`
	Reads        int  `json:"reads,omitempty"`
	ChangedFiles int  `json:"changedFiles,omitempty"`
	Spawned      bool `json:"spawned,omitempty"`
}

type sidecarDaemonChangedFile struct {
	FileName string `json:"fileName"`
	Text     string `json:"text,omitempty"`
	Deleted  bool   `json:"deleted,omitempty"`
}

type sidecarDaemonWorker struct {
	mu          sync.Mutex
	session     *sidecarSession
	lastUsed    time.Time
	refs        int
	initialized bool
	leases      map[string]sidecarDaemonLease
	leaseDone   chan struct{}
	recycle     bool
}

type sidecarDaemonLease struct {
	owner           string
	ownerPID        int
	ownerGeneration string
	retainedAt      time.Time
}

var persistentSidecarDaemonEnabled atomic.Bool

// EnablePersistentSidecarDaemon opts the product CLI into cross-process
// workers. Library callers and Go test binaries retain the in-process path
// unless their executable can serve the hidden daemon command.
func EnablePersistentSidecarDaemon() {
	persistentSidecarDaemonEnabled.Store(true)
}

// PersistentSidecarDaemonEnabled reports whether the current executable opted
// into serving and starting persistent workers.
func PersistentSidecarDaemonEnabled() bool {
	return persistentSidecarDaemonEnabled.Load()
}

// SidecarDaemonRoundTrip sends exactly one request to the daemon for the
// canonical workspace. Once the request has been written, transport or worker
// failures are returned without retrying the accepted work.
func SidecarDaemonRoundTrip(ctx context.Context, call SidecarDaemonCall) (*SidecarDaemonResult, error) {
	if call.WorkspaceKey == "" {
		return nil, errors.New("sidecar daemon workspace key is empty")
	}
	if call.WorkerKey == "" {
		return nil, errors.New("sidecar daemon worker key is empty")
	}
	if len(call.Payload) == 0 {
		return nil, errors.New("sidecar daemon payload is empty")
	}
	if call.NodePath == "" {
		nodePath, _, err := resolveSidecarNodeIdentity()
		if err != nil {
			return nil, err
		}
		call.NodePath = nodePath
	}
	if call.ChildEnv == nil {
		call.ChildEnv = sidecarEnv(call.ProjectDir, call.SidecarDir)
	}
	runtimeDir, err := sidecarDaemonRuntimeDir()
	if err != nil {
		return nil, err
	}
	id, err := sidecarDaemonID(call.WorkspaceKey)
	if err != nil {
		return nil, err
	}
	metadata, err := ensureSidecarDaemon(ctx, runtimeDir, id)
	if err != nil {
		return nil, err
	}
	message := sidecarDaemonMessage{
		Protocol:             sidecarDaemonProtocol,
		DaemonID:             id,
		Kind:                 "roundTrip",
		WorkerKey:            call.WorkerKey,
		ProjectDir:           call.ProjectDir,
		SidecarDir:           call.SidecarDir,
		NodePath:             call.NodePath,
		ChildEnv:             call.ChildEnv,
		LeaseOwner:           call.LeaseOwner,
		Payload:              json.RawMessage(call.Payload),
		StampFileNames:       call.StampFileNames,
		Overlays:             call.Overlays,
		InvalidatedFileNames: call.InvalidatedFileNames,
		Deadline:             deadlineFromContext(ctx),
	}
	var request struct {
		Operation string `json:"operation"`
	}
	_ = json.Unmarshal(call.Payload, &request)
	abandon := func() {}
	if call.LeaseOwner != "" && request.Operation == "transform" {
		abandon = func() {
			sendSidecarDaemonAbandon(metadata.Endpoint, id, call.WorkerKey, call.LeaseOwner)
		}
	}
	reply, err := exchangeSidecarDaemon(ctx, metadata.Endpoint, message)
	if err != nil {
		abandon()
		return nil, err
	}
	return &SidecarDaemonResult{
		Payload:      slices.Clone(reply.Payload),
		Stats:        reply.IO.Stats,
		Reads:        reply.IO.Reads,
		ChangedFiles: reply.IO.ChangedFiles,
		Spawned:      reply.IO.Spawned,
		abandon:      abandon,
	}, nil
}

// SidecarDaemonStatus reports every live workspace daemon under the current
// user's runtime root. Metadata left by exited processes is removed.
func SidecarDaemonStatus(ctx context.Context) ([]SidecarDaemonInfo, error) {
	runtimeDir, err := sidecarDaemonRuntimeDir()
	if err != nil {
		return nil, err
	}
	metadata, err := readAllSidecarDaemonMetadata(runtimeDir)
	if err != nil {
		return nil, err
	}
	infos := make([]SidecarDaemonInfo, 0, len(metadata))
	for _, entry := range metadata {
		reply, exchangeErr := exchangeSidecarDaemon(ctx, entry.Endpoint, sidecarDaemonMessage{Protocol: sidecarDaemonProtocol, DaemonID: entry.ID, Kind: "status"})
		if exchangeErr != nil {
			if !sidecarProcessAlive(entry.PID) {
				removeSidecarDaemonArtifacts(runtimeDir, entry.ID)
				continue
			}
			return nil, fmt.Errorf("query sidecar daemon %d: %w", entry.PID, exchangeErr)
		}
		if reply.Info == nil {
			return nil, fmt.Errorf("query sidecar daemon %d: missing status", entry.PID)
		}
		infos = append(infos, *reply.Info)
	}
	slices.SortFunc(infos, func(a, b SidecarDaemonInfo) int { return strings.Compare(a.ID, b.ID) })
	return infos, nil
}

// StopSidecarDaemons stops every live workspace daemon under the current
// user's runtime root and returns the number that acknowledged the request.
func StopSidecarDaemons(ctx context.Context) (int, error) {
	runtimeDir, err := sidecarDaemonRuntimeDir()
	if err != nil {
		return 0, err
	}
	metadata, err := readAllSidecarDaemonMetadata(runtimeDir)
	if err != nil {
		return 0, err
	}
	stopped := 0
	for _, entry := range metadata {
		_, exchangeErr := exchangeSidecarDaemon(ctx, entry.Endpoint, sidecarDaemonMessage{Protocol: sidecarDaemonProtocol, DaemonID: entry.ID, Kind: "stop"})
		if exchangeErr != nil {
			if !sidecarProcessAlive(entry.PID) {
				removeSidecarDaemonArtifacts(runtimeDir, entry.ID)
				continue
			}
			return stopped, fmt.Errorf("stop sidecar daemon %d: %w", entry.PID, exchangeErr)
		}
		stopped++
	}
	return stopped, nil
}

// RunSidecarDaemon serves the hidden daemon process. The runtime directory and
// daemon ID come from the already-coordinated parent process.
func RunSidecarDaemon(runtimeDir, id string) error {
	return runSidecarDaemon(runtimeDir, id, sidecarDaemonIdleTimeout)
}

func runSidecarDaemon(runtimeDir, id string, idleTimeout time.Duration) error {
	if err := prepareSidecarDaemonRuntimeDir(runtimeDir); err != nil {
		return err
	}
	compatible, err := sidecarDaemonIDCompatible(id)
	if err != nil {
		return err
	}
	if !compatible {
		return errors.New("sidecar daemon identifier is incompatible with this executable")
	}
	endpoint := sidecarDaemonEndpoint(runtimeDir, id)
	listener, err := listenSidecarDaemon(endpoint)
	if err != nil {
		return fmt.Errorf("listen for sidecar daemon: %w", err)
	}
	defer listener.Close()
	defer removeOwnedSidecarDaemonArtifacts(runtimeDir, id, os.Getpid())

	startedAt := time.Now().UTC()
	metadata := sidecarDaemonMetadata{
		Protocol: sidecarDaemonProtocol,
		ID:       id, PID: os.Getpid(), Endpoint: endpoint, StartedAt: startedAt,
	}
	if err := writeSidecarDaemonMetadata(runtimeDir, metadata); err != nil {
		return err
	}
	_ = os.Remove(sidecarDaemonLockPath(runtimeDir, id))

	server := &sidecarDaemonServer{
		id: id, listener: listener, startedAt: startedAt,
		lastActive: startedAt, idleDeadline: startedAt.Add(idleTimeout),
		idleTimeout: idleTimeout, workers: map[string]*sidecarDaemonWorker{},
	}
	defer server.closeWorkers()
	return server.serve()
}

type sidecarDaemonServer struct {
	id          string
	listener    net.Listener
	startedAt   time.Time
	idleTimeout time.Duration

	mu           sync.Mutex
	lastActive   time.Time
	idleDeadline time.Time
	active       int
	stopping     bool
	workers      map[string]*sidecarDaemonWorker
	wg           sync.WaitGroup
}

func (s *sidecarDaemonServer) serve() error {
	maintenanceDone := make(chan struct{})
	go s.maintain(maintenanceDone)
	defer close(maintenanceDone)
	for {
		conn, err := s.listener.Accept()
		if err != nil {
			s.mu.Lock()
			stopping := s.stopping
			s.mu.Unlock()
			s.wg.Wait()
			if stopping {
				return nil
			}
			return fmt.Errorf("accept sidecar daemon connection: %w", err)
		}
		s.wg.Add(1)
		go func() {
			defer s.wg.Done()
			defer conn.Close()
			s.handleConnection(conn)
		}()
	}
}

func (s *sidecarDaemonServer) maintain(done <-chan struct{}) {
	interval := s.idleTimeout / 4
	if interval > time.Second {
		interval = time.Second
	}
	if interval < 10*time.Millisecond {
		interval = 10 * time.Millisecond
	}
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	for {
		select {
		case <-done:
			return
		case now := <-ticker.C:
			s.mu.Lock()
			s.expireIdleWorkersLocked(now)
			if !s.stopping && s.active == 0 && s.retainedResultCountLocked() == 0 && !now.Before(s.idleDeadline) {
				s.stopping = true
				_ = s.listener.Close()
			}
			s.mu.Unlock()
		}
	}
}

func (s *sidecarDaemonServer) handleConnection(conn net.Conn) {
	reader := bufio.NewReader(conn)
	line, err := reader.ReadBytes('\n')
	if err != nil {
		_ = writeSidecarDaemonReply(conn, sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: fmt.Sprintf("read request: %v", err)})
		return
	}
	var request sidecarDaemonMessage
	if err := json.Unmarshal(line, &request); err != nil {
		_ = writeSidecarDaemonReply(conn, sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: fmt.Sprintf("decode request: %v", err)})
		return
	}
	if request.Protocol != sidecarDaemonProtocol {
		_ = writeSidecarDaemonReply(conn, sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: "unsupported daemon protocol"})
		return
	}
	if request.DaemonID != s.id {
		_ = writeSidecarDaemonReply(conn, sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: "sidecar daemon identifier mismatch"})
		return
	}

	switch request.Kind {
	case "status":
		now := time.Now().UTC()
		s.mu.Lock()
		s.expireIdleWorkersLocked(now)
		info := SidecarDaemonInfo{ID: s.id, PID: os.Getpid(), StartedAt: s.startedAt, LastActiveAt: s.lastActive, IdleDeadline: s.idleDeadline, WorkerCount: s.workerCountLocked()}
		s.mu.Unlock()
		_ = writeSidecarDaemonReply(conn, sidecarDaemonReply{
			Protocol: sidecarDaemonProtocol,
			Info:     &info,
		})
	case "stop":
		_ = writeSidecarDaemonReply(conn, sidecarDaemonReply{Protocol: sidecarDaemonProtocol})
		s.mu.Lock()
		if !s.stopping {
			s.stopping = true
			_ = s.listener.Close()
		}
		s.mu.Unlock()
	case "roundTrip":
		requestContext, cancel := context.WithCancel(context.Background())
		clientClosed := make(chan struct{})
		go func() {
			_, _ = reader.ReadByte()
			cancel()
			close(clientClosed)
		}()
		reply := s.roundTrip(request, requestContext)
		cancel()
		if err := writeSidecarDaemonReply(conn, reply); err != nil && request.LeaseOwner != "" {
			s.abandonWorkerLeaseOwner(request.WorkerKey, request.LeaseOwner)
		}
		_ = conn.Close()
		<-clientClosed
	case "abandon":
		if request.WorkerKey == "" || request.LeaseOwner == "" {
			_ = writeSidecarDaemonReply(conn, sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: "incomplete abandoned-result request"})
			return
		}
		s.abandonWorkerLeaseOwner(request.WorkerKey, request.LeaseOwner)
		_ = writeSidecarDaemonReply(conn, sidecarDaemonReply{Protocol: sidecarDaemonProtocol})
	default:
		_ = writeSidecarDaemonReply(conn, sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: "unknown daemon request"})
	}
}

func (s *sidecarDaemonServer) roundTrip(request sidecarDaemonMessage, requestContext context.Context) sidecarDaemonReply {
	if request.WorkerKey == "" || request.ProjectDir == "" || request.SidecarDir == "" || request.NodePath == "" || len(request.Payload) == 0 {
		return sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: "incomplete round-trip request"}
	}
	var nodeRequest map[string]json.RawMessage
	if err := json.Unmarshal(request.Payload, &nodeRequest); err != nil || nodeRequest == nil {
		return sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: "Node request payload must be a JSON object"}
	}
	var operation string
	var fileContentIdentities map[string]string
	if rawOperation := nodeRequest["operation"]; len(rawOperation) > 0 {
		_ = json.Unmarshal(rawOperation, &operation)
	}
	if rawIdentities := nodeRequest["fileContentIdentities"]; len(rawIdentities) > 0 {
		_ = json.Unmarshal(rawIdentities, &fileContentIdentities)
	}
	worker := s.acquireWorker(request.WorkerKey)
	defer s.releaseWorker(request.WorkerKey, worker)
	if err := s.waitForWorkerLease(requestContext, worker, operation, request.LeaseOwner, request.Deadline); err != nil {
		return sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: err.Error()}
	}
	if worker.recycle {
		if worker.session != nil {
			worker.session.close()
		}
		worker.session = nil
		worker.initialized = false
		worker.recycle = false
	}

	spawned := false
	if worker.session == nil || worker.session.dead {
		if worker.session != nil {
			worker.session.close()
		}
		session, err := spawnSidecarSessionWithRuntime(request.ProjectDir, request.SidecarDir, request.NodePath, request.ChildEnv)
		if err != nil {
			return sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: err.Error()}
		}
		worker.session = session
		worker.initialized = false
		s.mu.Lock()
		s.clearWorkerLeasesLocked(worker)
		s.mu.Unlock()
		spawned = true
	}
	var changedFiles []sidecarDaemonChangedFile
	var ioStats sidecarCallStats
	if operation == "transform" {
		var err error
		changedFiles, ioStats, err = collectSidecarDaemonChanges(worker.session, request.StampFileNames, fileContentIdentities, request.Overlays, request.InvalidatedFileNames, !worker.initialized)
		if err != nil {
			return sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: err.Error()}
		}
		encodedChanged, _ := json.Marshal(changedFiles)
		nodeRequest["changedFiles"] = encodedChanged
	}
	payload, err := json.Marshal(nodeRequest)
	if err != nil {
		return sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: err.Error()}
	}

	roundTripContext := requestContext
	cancel := func() {}
	if !request.Deadline.IsZero() {
		roundTripContext, cancel = context.WithDeadline(roundTripContext, request.Deadline)
	}
	line, err := worker.session.writeAndRead(roundTripContext, payload)
	cancel()
	if err != nil {
		worker.session.close()
		worker.session = nil
		worker.initialized = false
		s.mu.Lock()
		s.clearWorkerLeasesLocked(worker)
		s.mu.Unlock()
		return sidecarDaemonReply{Protocol: sidecarDaemonProtocol, Error: err.Error()}
	}
	var nodeResponse struct {
		ResultHandle string `json:"resultHandle"`
	}
	_ = json.Unmarshal(bytes.TrimSpace(line), &nodeResponse)
	if operation == "transform" && nodeResponse.ResultHandle != "" {
		ownerPID := sidecarLeaseOwnerPID(request.LeaseOwner)
		s.mu.Lock()
		if len(worker.leases) == 0 {
			worker.leaseDone = make(chan struct{})
		}
		worker.leases[nodeResponse.ResultHandle] = sidecarDaemonLease{
			owner:           request.LeaseOwner,
			ownerPID:        ownerPID,
			ownerGeneration: sidecarProcessGeneration(ownerPID),
			retainedAt:      time.Now().UTC(),
		}
		s.mu.Unlock()
	}
	if operation == "release" {
		var resultHandle string
		_ = json.Unmarshal(nodeRequest["resultHandle"], &resultHandle)
		s.mu.Lock()
		delete(worker.leases, resultHandle)
		if len(worker.leases) == 0 {
			s.signalWorkerLeasesDoneLocked(worker)
		}
		s.mu.Unlock()
	}
	if operation == "transform" {
		worker.initialized = true
	}
	return sidecarDaemonReply{
		Protocol: sidecarDaemonProtocol,
		Payload:  json.RawMessage(strings.TrimSpace(string(line))),
		IO:       sidecarDaemonReplyIO{Stats: ioStats.stats, Reads: ioStats.reads, ChangedFiles: len(changedFiles), Spawned: spawned},
	}
}

func (s *sidecarDaemonServer) waitForWorkerLease(ctx context.Context, worker *sidecarDaemonWorker, operation, owner string, deadline time.Time) error {
	if operation == "maps" || operation == "release" {
		return nil
	}
	for {
		s.mu.Lock()
		if len(worker.leases) == 0 || owner != "" && workerLeasesOwnedBy(worker, owner) {
			s.mu.Unlock()
			return nil
		}
		done := worker.leaseDone
		s.mu.Unlock()

		worker.mu.Unlock()
		waitContext := ctx
		cancel := func() {}
		if !deadline.IsZero() {
			waitContext, cancel = context.WithDeadline(ctx, deadline)
		}
		select {
		case <-done:
			cancel()
			worker.mu.Lock()
		case <-waitContext.Done():
			err := waitContext.Err()
			cancel()
			worker.mu.Lock()
			return err
		}
	}
}

func workerLeasesOwnedBy(worker *sidecarDaemonWorker, owner string) bool {
	for _, lease := range worker.leases {
		if lease.owner != owner {
			return false
		}
	}
	return true
}

func sidecarLeaseOwnerPID(owner string) int {
	prefix, _, ok := strings.Cut(owner, "-")
	if !ok {
		return 0
	}
	pid, err := strconv.Atoi(prefix)
	if err != nil || pid <= 0 {
		return 0
	}
	return pid
}

func collectSidecarDaemonChanges(session *sidecarSession, fileNames []string, contentIdentities map[string]string, overlays map[string]string, invalidated []string, fresh bool) ([]sidecarDaemonChangedFile, sidecarCallStats, error) {
	var ioStats sidecarCallStats
	nextStamps := maps.Clone(session.stamps)
	if nextStamps == nil {
		nextStamps = make(map[string]sidecarFileStamp)
	}
	nextOverlaid := maps.Clone(session.overlaid)
	if nextOverlaid == nil {
		nextOverlaid = make(map[string]string)
	}
	contentIdentities = normalizedSidecarContentIdentities(fileNames, contentIdentities)
	caseSensitive := osvfs.FS().UseCaseSensitiveFileNames()
	currentFiles := make(map[string]string, len(fileNames)+len(overlays))
	for _, fileName := range fileNames {
		path := filepath.FromSlash(fileName)
		if contentIdentities[path] == "" {
			return nil, ioStats, fmt.Errorf("missing content identity for %q", path)
		}
		currentFiles[normalizeOverlayPath(path, caseSensitive)] = path
	}
	overlaid := make(map[string]sidecarDaemonChangedFile, len(overlays))
	for fileName, text := range overlays {
		path := filepath.FromSlash(fileName)
		key := normalizeOverlayPath(path, caseSensitive)
		currentFiles[key] = path
		overlaid[key] = sidecarDaemonChangedFile{FileName: path, Text: text}
	}
	deleted := make(map[string]struct{})
	for _, fileName := range invalidated {
		path := filepath.FromSlash(fileName)
		key := normalizeOverlayPath(path, caseSensitive)
		deleted[path] = struct{}{}
		delete(currentFiles, key)
		delete(overlaid, key)
	}

	changed := make([]sidecarDaemonChangedFile, 0, len(overlays))
	for key, previousPath := range nextOverlaid {
		if _, stillOverlaid := overlaid[key]; stillOverlaid {
			continue
		}
		delete(nextOverlaid, key)
		currentPath, stillCurrent := currentFiles[key]
		if !stillCurrent {
			deleted[previousPath] = struct{}{}
			continue
		}
		info, statErr := os.Stat(currentPath)
		ioStats.stats++
		text, readErr := readSidecarSourceText(currentPath)
		ioStats.reads++
		if errors.Is(readErr, os.ErrNotExist) || errors.Is(statErr, os.ErrNotExist) {
			return nil, ioStats, fmt.Errorf("source changed after the compiler snapshot: %s", currentPath)
		}
		if readErr != nil {
			return nil, ioStats, readErr
		}
		if statErr != nil {
			return nil, ioStats, statErr
		}
		if sidecarTextContentIdentity(text) != contentIdentities[currentPath] {
			return nil, ioStats, fmt.Errorf("source changed after the compiler snapshot: %s", currentPath)
		}
		changed = append(changed, sidecarDaemonChangedFile{FileName: currentPath, Text: text})
		nextStamps[currentPath] = newSidecarFileStampWithContent(info, contentIdentities[currentPath])
	}
	for _, key := range slices.Sorted(maps.Keys(overlaid)) {
		file := overlaid[key]
		changed = append(changed, file)
		nextOverlaid[key] = file.FileName
	}

	for _, fileName := range fileNames {
		path := filepath.FromSlash(fileName)
		key := normalizeOverlayPath(path, caseSensitive)
		if _, isOverlay := overlaid[key]; isOverlay {
			nextOverlaid[key] = path
			continue
		}
		if _, isDeleted := deleted[path]; isDeleted {
			continue
		}
		info, statErr := os.Stat(path)
		ioStats.stats++
		if errors.Is(statErr, os.ErrNotExist) {
			return nil, ioStats, fmt.Errorf("source changed after the compiler snapshot: %s", path)
		}
		if statErr != nil {
			return nil, ioStats, statErr
		}
		stamp := newSidecarFileStampWithContent(info, contentIdentities[path])
		if previous, ok := nextStamps[path]; !fresh && (!ok || previous != stamp) {
			text, readErr := readSidecarSourceText(path)
			ioStats.reads++
			if readErr != nil {
				return nil, ioStats, readErr
			}
			if sidecarTextContentIdentity(text) != contentIdentities[path] {
				return nil, ioStats, fmt.Errorf("source changed after the compiler snapshot: %s", path)
			}
			changed = append(changed, sidecarDaemonChangedFile{FileName: path, Text: text})
		}
		nextStamps[path] = stamp
	}
	for stampedPath := range nextStamps {
		if _, current := currentFiles[normalizeOverlayPath(stampedPath, caseSensitive)]; !current {
			deleted[stampedPath] = struct{}{}
		}
	}
	for _, path := range slices.Sorted(maps.Keys(deleted)) {
		delete(nextStamps, path)
		delete(nextOverlaid, normalizeOverlayPath(path, caseSensitive))
		changed = append(changed, sidecarDaemonChangedFile{FileName: path, Deleted: true})
	}
	session.stamps = nextStamps
	session.overlaid = nextOverlaid
	ioStats.changedFiles = len(changed)
	return changed, ioStats, nil
}

func (s *sidecarDaemonServer) acquireWorker(key string) *sidecarDaemonWorker {
	s.mu.Lock()
	worker := s.workers[key]
	if worker == nil {
		worker = &sidecarDaemonWorker{leases: make(map[string]sidecarDaemonLease)}
		s.workers[key] = worker
	}
	worker.refs++
	s.active++
	s.mu.Unlock()
	worker.mu.Lock()
	return worker
}

func (s *sidecarDaemonServer) releaseWorker(key string, worker *sidecarDaemonWorker) {
	now := time.Now().UTC()
	s.mu.Lock()
	worker.refs--
	s.active--
	worker.lastUsed = now
	s.lastActive = now
	s.idleDeadline = now.Add(s.idleTimeout)
	if worker.session == nil && worker.refs == 0 {
		delete(s.workers, key)
	}
	s.trimIdleWorkersLocked()
	s.mu.Unlock()
	worker.mu.Unlock()
}

func (s *sidecarDaemonServer) abandonWorkerLeaseOwner(key, owner string) {
	worker := s.acquireWorker(key)
	defer s.releaseWorker(key, worker)
	s.mu.Lock()
	owned := false
	for _, lease := range worker.leases {
		if lease.owner == owner {
			owned = true
			break
		}
	}
	if owned {
		s.clearWorkerLeasesLocked(worker)
	}
	s.mu.Unlock()
	if !owned {
		return
	}
	if worker.session != nil {
		worker.session.close()
	}
	worker.session = nil
	worker.initialized = false
}

func (s *sidecarDaemonServer) expireIdleWorkersLocked(now time.Time) {
	for key, worker := range s.workers {
		for handle, lease := range worker.leases {
			ownerExited := sidecarLeaseOwnerExited(lease)
			unownedExpired := lease.ownerPID == 0 && now.Sub(lease.retainedAt) >= sidecarResultExpiry
			if ownerExited || unownedExpired {
				delete(worker.leases, handle)
				worker.recycle = true
			}
		}
		if len(worker.leases) == 0 {
			s.signalWorkerLeasesDoneLocked(worker)
		}
		if worker.recycle && worker.refs == 0 {
			if worker.session != nil {
				worker.session.close()
			}
			worker.session = nil
			worker.initialized = false
			worker.recycle = false
		}
		if worker.refs == 0 && len(worker.leases) == 0 && !worker.lastUsed.IsZero() && now.Sub(worker.lastUsed) >= s.idleTimeout {
			if worker.session != nil {
				worker.session.close()
			}
			delete(s.workers, key)
		}
	}
}

func sidecarLeaseOwnerExited(lease sidecarDaemonLease) bool {
	if lease.ownerPID == 0 {
		return false
	}
	if !sidecarProcessAlive(lease.ownerPID) {
		return true
	}
	generation := sidecarProcessGeneration(lease.ownerPID)
	return lease.ownerGeneration != "" && generation != lease.ownerGeneration
}

func (s *sidecarDaemonServer) clearWorkerLeasesLocked(worker *sidecarDaemonWorker) {
	clear(worker.leases)
	s.signalWorkerLeasesDoneLocked(worker)
}

func (s *sidecarDaemonServer) signalWorkerLeasesDoneLocked(worker *sidecarDaemonWorker) {
	if worker.leaseDone != nil {
		close(worker.leaseDone)
		worker.leaseDone = nil
	}
}

func (s *sidecarDaemonServer) trimIdleWorkersLocked() {
	for s.idleWorkerCountLocked() > 2 {
		var oldestKey string
		var oldest time.Time
		for key, worker := range s.workers {
			if worker.refs != 0 || worker.session == nil || len(worker.leases) != 0 {
				continue
			}
			if oldestKey == "" || worker.lastUsed.Before(oldest) {
				oldestKey = key
				oldest = worker.lastUsed
			}
		}
		worker := s.workers[oldestKey]
		worker.session.close()
		delete(s.workers, oldestKey)
	}
}

func (s *sidecarDaemonServer) idleWorkerCountLocked() int {
	count := 0
	for _, worker := range s.workers {
		if worker.refs == 0 && worker.session != nil && len(worker.leases) == 0 {
			count++
		}
	}
	return count
}

func (s *sidecarDaemonServer) workerCountLocked() int {
	count := 0
	for _, worker := range s.workers {
		if worker.session != nil {
			count++
		}
	}
	return count
}

func (s *sidecarDaemonServer) retainedResultCountLocked() int {
	count := 0
	for _, worker := range s.workers {
		count += len(worker.leases)
	}
	return count
}

func (s *sidecarDaemonServer) closeWorkers() {
	s.mu.Lock()
	defer s.mu.Unlock()
	for key, worker := range s.workers {
		s.clearWorkerLeasesLocked(worker)
		if worker.session != nil {
			worker.session.close()
		}
		delete(s.workers, key)
	}
}

func exchangeSidecarDaemon(ctx context.Context, endpoint string, request sidecarDaemonMessage) (sidecarDaemonReply, error) {
	conn, err := dialSidecarDaemon(ctx, endpoint)
	if err != nil {
		return sidecarDaemonReply{}, err
	}
	defer conn.Close()
	stopCancel := context.AfterFunc(ctx, func() { _ = conn.SetDeadline(time.Now()) })
	defer stopCancel()
	if deadline, ok := ctx.Deadline(); ok {
		_ = conn.SetDeadline(deadline)
	}
	if err := json.NewEncoder(conn).Encode(request); err != nil {
		return sidecarDaemonReply{}, fmt.Errorf("write sidecar daemon request: %w", err)
	}
	var reply sidecarDaemonReply
	if err := json.NewDecoder(conn).Decode(&reply); err != nil {
		return sidecarDaemonReply{}, fmt.Errorf("read sidecar daemon response: %w", err)
	}
	if reply.Protocol != sidecarDaemonProtocol {
		return sidecarDaemonReply{}, errors.New("sidecar daemon protocol mismatch")
	}
	if reply.Error != "" {
		return sidecarDaemonReply{}, errors.New(reply.Error)
	}
	return reply, nil
}

func sendSidecarDaemonAbandon(endpoint, daemonID, workerKey, owner string) {
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	conn, err := dialSidecarDaemon(ctx, endpoint)
	if err != nil {
		return
	}
	defer conn.Close()
	if deadline, ok := ctx.Deadline(); ok {
		_ = conn.SetWriteDeadline(deadline)
	}
	_ = json.NewEncoder(conn).Encode(sidecarDaemonMessage{
		Protocol:   sidecarDaemonProtocol,
		DaemonID:   daemonID,
		Kind:       "abandon",
		WorkerKey:  workerKey,
		LeaseOwner: owner,
	})
}

func deadlineFromContext(ctx context.Context) time.Time {
	deadline, _ := ctx.Deadline()
	return deadline
}

func writeSidecarDaemonReply(w io.Writer, reply sidecarDaemonReply) error {
	return json.NewEncoder(w).Encode(reply)
}

func ensureSidecarDaemon(ctx context.Context, runtimeDir, id string) (sidecarDaemonMetadata, error) {
	if err := prepareSidecarDaemonRuntimeDir(runtimeDir); err != nil {
		return sidecarDaemonMetadata{}, err
	}
	if metadata, err := readSidecarDaemonMetadata(runtimeDir, id); err == nil {
		pingErr := pingSidecarDaemon(ctx, metadata)
		if pingErr == nil {
			return metadata, nil
		}
		if sidecarProcessAlive(metadata.PID) {
			retryDeadline := time.Now().Add(250 * time.Millisecond)
			for time.Now().Before(retryDeadline) && sidecarProcessAlive(metadata.PID) {
				select {
				case <-ctx.Done():
					return sidecarDaemonMetadata{}, ctx.Err()
				case <-time.After(20 * time.Millisecond):
				}
				if retryErr := pingSidecarDaemon(ctx, metadata); retryErr == nil {
					return metadata, nil
				}
			}
			if sidecarProcessAlive(metadata.PID) {
				return sidecarDaemonMetadata{}, fmt.Errorf("sidecar daemon %d is running but unavailable: %w", metadata.PID, pingErr)
			}
		}
		removeSidecarDaemonArtifacts(runtimeDir, id)
	}

	lockPath := sidecarDaemonLockPath(runtimeDir, id)
	startedPID := 0
	for {
		lock, err := os.OpenFile(lockPath, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
		if err == nil {
			if _, writeErr := fmt.Fprintf(lock, "%d\n", os.Getpid()); writeErr != nil {
				_ = lock.Close()
				_ = os.Remove(lockPath)
				return sidecarDaemonMetadata{}, fmt.Errorf("record sidecar daemon starter: %w", writeErr)
			}
			if closeErr := lock.Close(); closeErr != nil {
				_ = os.Remove(lockPath)
				return sidecarDaemonMetadata{}, fmt.Errorf("record sidecar daemon starter: %w", closeErr)
			}
			_ = os.Remove(sidecarDaemonMetadataPath(runtimeDir, id))
			if err := prepareSidecarDaemonEndpoint(sidecarDaemonEndpoint(runtimeDir, id)); err != nil {
				_ = os.Remove(lockPath)
				return sidecarDaemonMetadata{}, err
			}
			removeSidecarDaemonEndpoint(sidecarDaemonEndpoint(runtimeDir, id))
			daemonPID, err := startSidecarDaemonProcess(runtimeDir, id)
			if err != nil {
				_ = os.Remove(lockPath)
				return sidecarDaemonMetadata{}, err
			}
			startedPID = daemonPID
			break
		}
		if !errors.Is(err, os.ErrExist) {
			return sidecarDaemonMetadata{}, fmt.Errorf("coordinate sidecar daemon startup: %w", err)
		}
		ownerPID, ownerErr := readSidecarDaemonLockOwner(lockPath)
		if ownerErr == nil && !sidecarProcessAlive(ownerPID) {
			info, statErr := os.Stat(lockPath)
			if statErr == nil && time.Since(info.ModTime()) > sidecarDaemonLockExpiry {
				_ = os.Remove(lockPath)
				continue
			}
		}
		break
	}

	deadline := time.Now().Add(sidecarDaemonStartWait)
	for time.Now().Before(deadline) {
		metadata, err := readSidecarDaemonMetadata(runtimeDir, id)
		if err == nil {
			pingCtx, cancel := context.WithTimeout(ctx, 100*time.Millisecond)
			_, pingErr := exchangeSidecarDaemon(pingCtx, metadata.Endpoint, sidecarDaemonMessage{Protocol: sidecarDaemonProtocol, DaemonID: id, Kind: "status"})
			cancel()
			if pingErr == nil {
				return metadata, nil
			}
		}
		select {
		case <-ctx.Done():
			if startedPID != 0 && !sidecarProcessAlive(startedPID) {
				_ = os.Remove(lockPath)
			}
			return sidecarDaemonMetadata{}, ctx.Err()
		case <-time.After(20 * time.Millisecond):
		}
	}
	if startedPID != 0 && !sidecarProcessAlive(startedPID) {
		_ = os.Remove(lockPath)
	}
	return sidecarDaemonMetadata{}, errors.New("sidecar daemon did not become ready")
}

func pingSidecarDaemon(ctx context.Context, metadata sidecarDaemonMetadata) error {
	pingCtx, cancel := context.WithTimeout(ctx, 100*time.Millisecond)
	defer cancel()
	_, err := exchangeSidecarDaemon(pingCtx, metadata.Endpoint, sidecarDaemonMessage{Protocol: sidecarDaemonProtocol, DaemonID: metadata.ID, Kind: "status"})
	return err
}

func startSidecarDaemonProcess(runtimeDir, id string) (int, error) {
	executable, err := os.Executable()
	if err != nil {
		return 0, fmt.Errorf("locate sloptor executable: %w", err)
	}
	cmd := exec.Command(executable, "__sidecar-daemon", "--runtime-dir", runtimeDir, "--id", id)
	cmd.Stdin = nil
	cmd.Stdout = nil
	cmd.Stderr = nil
	configureSidecarDaemonProcess(cmd)
	if err := cmd.Start(); err != nil {
		return 0, fmt.Errorf("start sidecar daemon: %w", err)
	}
	pid := cmd.Process.Pid
	if err := cmd.Process.Release(); err != nil {
		return 0, err
	}
	return pid, nil
}

func readSidecarDaemonLockOwner(path string) (int, error) {
	contents, err := os.ReadFile(path)
	if err != nil {
		return 0, err
	}
	var pid int
	if _, err := fmt.Sscanf(string(contents), "%d", &pid); err != nil || pid <= 0 {
		return 0, errors.New("invalid sidecar daemon startup lock")
	}
	return pid, nil
}

func sidecarDaemonRuntimeDir() (string, error) {
	if configured := os.Getenv(sidecarDaemonRuntimeEnv); configured != "" {
		return filepath.Abs(configured)
	}
	if runtimeRoot := os.Getenv("XDG_RUNTIME_DIR"); runtimeRoot != "" {
		return filepath.Join(runtimeRoot, "rotor"), nil
	}
	cacheDir, err := os.UserCacheDir()
	if err != nil {
		return "", fmt.Errorf("resolve sidecar daemon runtime directory: %w", err)
	}
	return filepath.Join(cacheDir, "rotor", "daemon"), nil
}

func prepareSidecarDaemonRuntimeDir(runtimeDir string) error {
	if runtimeDir == "" {
		return errors.New("sidecar daemon runtime directory is empty")
	}
	if err := os.MkdirAll(runtimeDir, 0o700); err != nil {
		return fmt.Errorf("create sidecar daemon runtime directory: %w", err)
	}
	if err := os.Chmod(runtimeDir, 0o700); err != nil {
		return fmt.Errorf("secure sidecar daemon runtime directory: %w", err)
	}
	return nil
}

var (
	sidecarDaemonCompatibilityOnce sync.Once
	sidecarDaemonCompatibility     string
	sidecarDaemonCompatibilityErr  error
)

func sidecarDaemonID(workspaceKey string) (string, error) {
	compatibility, err := sidecarDaemonCompatibilityID()
	if err != nil {
		return "", err
	}
	return compatibility + "-" + shortSidecarDaemonHash(workspaceKey), nil
}

func sidecarDaemonIDCompatible(id string) (bool, error) {
	compatibility, err := sidecarDaemonCompatibilityID()
	if err != nil {
		return false, err
	}
	return strings.HasPrefix(id, compatibility+"-") && len(id) == len(compatibility)+1+20, nil
}

func sidecarDaemonCompatibilityID() (string, error) {
	sidecarDaemonCompatibilityOnce.Do(func() {
		executable, err := os.Executable()
		if err != nil {
			sidecarDaemonCompatibilityErr = fmt.Errorf("locate sloptor executable: %w", err)
			return
		}
		file, err := os.Open(executable)
		if err != nil {
			sidecarDaemonCompatibilityErr = fmt.Errorf("open sloptor executable: %w", err)
			return
		}
		defer file.Close()
		digest := sha256.New()
		_, _ = io.WriteString(digest, fmt.Sprintf("%d\x00%s\x00%s\x00%s\x00", sidecarDaemonProtocol, runtime.Version(), runtime.GOOS, runtime.GOARCH))
		if _, err := io.Copy(digest, file); err != nil {
			sidecarDaemonCompatibilityErr = fmt.Errorf("hash sloptor executable: %w", err)
			return
		}
		sidecarDaemonCompatibility = hex.EncodeToString(digest.Sum(nil)[:8])
	})
	return sidecarDaemonCompatibility, sidecarDaemonCompatibilityErr
}

func shortSidecarDaemonHash(value string) string {
	digest := sha256.Sum256([]byte(value))
	return hex.EncodeToString(digest[:10])
}

func sidecarDaemonMetadataPath(runtimeDir, id string) string {
	return filepath.Join(runtimeDir, id+".json")
}

func sidecarDaemonLockPath(runtimeDir, id string) string {
	return filepath.Join(runtimeDir, id+".lock")
}

func writeSidecarDaemonMetadata(runtimeDir string, metadata sidecarDaemonMetadata) error {
	temporary, err := os.CreateTemp(runtimeDir, metadata.ID+".metadata-")
	if err != nil {
		return fmt.Errorf("create sidecar daemon metadata: %w", err)
	}
	temporaryPath := temporary.Name()
	defer os.Remove(temporaryPath)
	if err := temporary.Chmod(0o600); err != nil {
		_ = temporary.Close()
		return err
	}
	if err := json.NewEncoder(temporary).Encode(metadata); err != nil {
		_ = temporary.Close()
		return err
	}
	if err := temporary.Close(); err != nil {
		return err
	}
	if err := os.Rename(temporaryPath, sidecarDaemonMetadataPath(runtimeDir, metadata.ID)); err != nil {
		return fmt.Errorf("publish sidecar daemon metadata: %w", err)
	}
	return nil
}

func readSidecarDaemonMetadata(runtimeDir, id string) (sidecarDaemonMetadata, error) {
	file, err := os.Open(sidecarDaemonMetadataPath(runtimeDir, id))
	if err != nil {
		return sidecarDaemonMetadata{}, err
	}
	defer file.Close()
	var metadata sidecarDaemonMetadata
	if err := json.NewDecoder(file).Decode(&metadata); err != nil {
		return sidecarDaemonMetadata{}, err
	}
	if metadata.Protocol != sidecarDaemonProtocol || metadata.ID != id || metadata.PID <= 0 || metadata.Endpoint != sidecarDaemonEndpoint(runtimeDir, id) || metadata.StartedAt.IsZero() {
		return sidecarDaemonMetadata{}, errors.New("invalid sidecar daemon metadata")
	}
	return metadata, nil
}

func readAllSidecarDaemonMetadata(runtimeDir string) ([]sidecarDaemonMetadata, error) {
	entries, err := os.ReadDir(runtimeDir)
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read sidecar daemon runtime directory: %w", err)
	}
	metadata := make([]sidecarDaemonMetadata, 0)
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".json" {
			continue
		}
		id := strings.TrimSuffix(entry.Name(), ".json")
		item, readErr := readSidecarDaemonMetadata(runtimeDir, id)
		if readErr != nil {
			return nil, fmt.Errorf("read sidecar daemon metadata %s: %w", entry.Name(), readErr)
		}
		metadata = append(metadata, item)
	}
	return metadata, nil
}

func removeSidecarDaemonArtifacts(runtimeDir, id string) {
	_ = os.Remove(sidecarDaemonMetadataPath(runtimeDir, id))
	_ = os.Remove(sidecarDaemonLockPath(runtimeDir, id))
	removeSidecarDaemonEndpoint(sidecarDaemonEndpoint(runtimeDir, id))
}

func removeOwnedSidecarDaemonArtifacts(runtimeDir, id string, pid int) {
	metadata, err := readSidecarDaemonMetadata(runtimeDir, id)
	if err == nil && metadata.PID != pid {
		return
	}
	removeSidecarDaemonArtifacts(runtimeDir, id)
}
