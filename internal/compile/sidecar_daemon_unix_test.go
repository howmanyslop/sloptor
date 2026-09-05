//go:build !windows

package compile

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// Catches a later compiler invocation serving a stale source snapshot from a
// warm worker. The expected changed text and deletion come from the complete
// file snapshot supplied by the caller, independently of the daemon internals.
func TestSidecarDaemonOwnsCrossProcessFileFreshness(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	sidecarDir := t.TempDir()
	nodePath := writeEchoNode(t)
	t.Setenv("ROTOR_NODE_PATH", nodePath)

	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)

	sourcePath := filepath.Join(projectDir, "source.ts")
	removedFromProjectPath := filepath.Join(projectDir, "removed.ts")
	if err := os.WriteFile(sourcePath, []byte("export const value = 1;\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(removedFromProjectPath, []byte("export const removed = true;\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	call := SidecarDaemonCall{
		WorkspaceKey:   projectDir,
		WorkerKey:      "tsconfig",
		ProjectDir:     projectDir,
		SidecarDir:     sidecarDir,
		Payload:        []byte(`{"protocol":2,"operation":"transform"}`),
		StampFileNames: []string{sourcePath, removedFromProjectPath},
	}
	first, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	if changed := decodeEchoedChanges(t, first.Payload); len(changed) != 0 {
		t.Fatalf("fresh worker received changed files: %+v", changed)
	}

	beforeRewrite, err := os.Stat(sourcePath)
	if err != nil {
		t.Fatal(err)
	}
	wantText := "export const value = 2;\n"
	if err := os.WriteFile(sourcePath, []byte(wantText), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Chtimes(sourcePath, beforeRewrite.ModTime(), beforeRewrite.ModTime()); err != nil {
		t.Fatal(err)
	}
	second, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	changed := decodeEchoedChanges(t, second.Payload)
	if len(changed) != 1 || changed[0].FileName != sourcePath || changed[0].Text != wantText || changed[0].Deleted {
		t.Fatalf("changed files = %+v, want updated source text", changed)
	}

	if err := os.Remove(sourcePath); err != nil {
		t.Fatal(err)
	}
	third, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	changed = decodeEchoedChanges(t, third.Payload)
	if len(changed) != 1 || changed[0].FileName != sourcePath || !changed[0].Deleted {
		t.Fatalf("changed files = %+v, want on-disk deletion record", changed)
	}

	call.StampFileNames = nil
	fourth, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	changed = decodeEchoedChanges(t, fourth.Payload)
	if len(changed) != 1 || changed[0].FileName != removedFromProjectPath || !changed[0].Deleted {
		t.Fatalf("changed files = %+v, want removed-project-file record", changed)
	}

	if _, err := StopSidecarDaemons(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := <-done; err != nil {
		t.Fatal(err)
	}
}

func TestSidecarDaemonWarmDoesNotPopulateTheFirstTransformDelta(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	t.Setenv("ROTOR_NODE_PATH", writeEchoNode(t))
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), time.Second)
		defer cancel()
		_, _ = StopSidecarDaemons(ctx)
	})

	sourcePath := filepath.Join(projectDir, "source.ts")
	if err := os.WriteFile(sourcePath, []byte("export const value = 1;\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	call := SidecarDaemonCall{
		WorkspaceKey: projectDir,
		WorkerKey:    "tsconfig",
		ProjectDir:   projectDir,
		SidecarDir:   t.TempDir(),
		Payload:      []byte(`{"protocol":2,"operation":"warm"}`),
	}
	if _, err := SidecarDaemonRoundTrip(context.Background(), call); err != nil {
		t.Fatal(err)
	}
	call.Payload = []byte(`{"protocol":2,"operation":"transform"}`)
	call.StampFileNames = []string{sourcePath}
	result, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	if changes := decodeEchoedChanges(t, result.Payload); len(changes) != 0 {
		t.Fatalf("first transform after warm shipped %d unchanged files", len(changes))
	}
}

// Catches solution projects blocking one another and concurrent requests for
// the same worker consuming each other's response. The shared marker is an
// external observation of whether two Node processes were active together.
func TestSidecarDaemonRunsDifferentWorkersConcurrentlyAndSerializesOneWorker(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	stateDir := t.TempDir()
	t.Setenv("SIDECAR_TEST_STATE", stateDir)
	t.Setenv("ROTOR_NODE_PATH", writeConcurrencyNode(t))
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 3*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)

	runConcurrentCalls(t, projectDir, "same", "same")
	if _, err := os.Stat(filepath.Join(stateDir, "overlap")); !os.IsNotExist(err) {
		t.Fatalf("same-worker requests overlapped: %v", err)
	}
	t.Setenv("SIDECAR_TEST_REQUIRE_OVERLAP", "1")
	runConcurrentCalls(t, projectDir, "alpha", "beta")
	if _, err := os.Stat(filepath.Join(stateDir, "overlap")); err != nil {
		t.Fatalf("different workers did not overlap: %v", err)
	}

	if _, err := StopSidecarDaemons(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := <-done; err != nil {
		t.Fatal(err)
	}
}

// Plugins may capture environment values when Node loads them. Two compiler
// invocations with different child environments must therefore reach distinct
// workers and observe their own value.
func TestSidecarDaemonUsesEnvironmentSpecificWorkers(t *testing.T) {
	fixture := newSidecarIdentityFixture(t)
	if err := os.WriteFile(fixture.nodePath, []byte("#!/bin/sh\nwhile IFS= read -r line; do printf '{\"value\":\"%s\"}\\n' \"$ROTOR_IDENTITY_FIXTURE_ENV\"; done\n"), 0o700); err != nil {
		t.Fatal(err)
	}
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	t.Setenv("ROTOR_IDENTITY_FIXTURE_ENV", "first")
	firstIdentity, err := resolveSidecarWorkerIdentity(fixture.projectDir, fixture.configPath)
	if err != nil {
		t.Fatal(err)
	}
	id, err := sidecarDaemonID(firstIdentity.WorkspaceKey)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), time.Second)
		defer cancel()
		_, _ = StopSidecarDaemons(ctx)
	})

	run := func(identity sidecarWorkerIdentity) string {
		t.Helper()
		result, err := SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
			WorkspaceKey: identity.WorkspaceKey,
			WorkerKey:    identity.WorkerKey,
			ProjectDir:   identity.ProjectDir,
			SidecarDir:   identity.SidecarDir,
			NodePath:     identity.NodePath,
			ChildEnv:     identity.ChildEnv,
			Payload:      []byte(`{"protocol":2,"operation":"transform"}`),
		})
		if err != nil {
			t.Fatal(err)
		}
		var response struct {
			Value string `json:"value"`
		}
		if err := json.Unmarshal(result.Payload, &response); err != nil {
			t.Fatal(err)
		}
		return response.Value
	}
	if got := run(firstIdentity); got != "first" {
		t.Fatalf("first worker environment = %q", got)
	}

	t.Setenv("ROTOR_IDENTITY_FIXTURE_ENV", "second")
	secondIdentity, err := resolveSidecarWorkerIdentity(fixture.projectDir, fixture.configPath)
	if err != nil {
		t.Fatal(err)
	}
	if secondIdentity.WorkerKey == firstIdentity.WorkerKey {
		t.Fatal("environment change reused the first worker")
	}
	if got := run(secondIdentity); got != "second" {
		t.Fatalf("second worker environment = %q", got)
	}
}

// Catches startup recovery unlinking an endpoint owned by a still-running PID.
// A live but unreachable daemon is an operational failure, not stale state.
func TestSidecarDaemonDoesNotReplaceLiveUnresponsiveMetadata(t *testing.T) {
	runtimeDir := t.TempDir()
	id, err := sidecarDaemonID(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	if err := writeSidecarDaemonMetadata(runtimeDir, sidecarDaemonMetadata{
		Protocol: sidecarDaemonProtocol, ID: id, PID: os.Getpid(),
		Endpoint: sidecarDaemonEndpoint(runtimeDir, id), StartedAt: time.Now().UTC(),
	}); err != nil {
		t.Fatal(err)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()
	if _, err := ensureSidecarDaemon(ctx, runtimeDir, id); err == nil {
		t.Fatal("live unresponsive metadata was replaced")
	}
	if _, err := os.Stat(sidecarDaemonMetadataPath(runtimeDir, id)); err != nil {
		t.Fatalf("live daemon metadata was removed: %v", err)
	}
}

// Catches clients reaching a compatible socket while speaking the wrong
// protocol or targeting another workspace daemon.
func TestSidecarDaemonRejectsProtocolAndIdentityMismatch(t *testing.T) {
	runtimeDir := t.TempDir()
	id, err := sidecarDaemonID(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	endpoint := sidecarDaemonEndpoint(runtimeDir, id)
	if _, err := exchangeSidecarDaemon(context.Background(), endpoint, sidecarDaemonMessage{Protocol: sidecarDaemonProtocol + 1, DaemonID: id, Kind: "status"}); err == nil {
		t.Fatal("daemon accepted an incompatible protocol")
	}
	if _, err := exchangeSidecarDaemon(context.Background(), endpoint, sidecarDaemonMessage{Protocol: sidecarDaemonProtocol, DaemonID: id + "x", Kind: "status"}); err == nil {
		t.Fatal("daemon accepted the wrong identity")
	}
	if _, err := exchangeSidecarDaemon(context.Background(), endpoint, sidecarDaemonMessage{Protocol: sidecarDaemonProtocol, DaemonID: id, Kind: "stop"}); err != nil {
		t.Fatal(err)
	}
	if err := <-done; err != nil {
		t.Fatal(err)
	}
}

// Catches an unused broker retaining its metadata and socket indefinitely.
// The injected short timeout is the same idle transition as the five-minute
// production duration.
func TestSidecarDaemonExpiresWhenIdle(t *testing.T) {
	runtimeDir := t.TempDir()
	id, err := sidecarDaemonID(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 60*time.Millisecond) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("idle daemon did not exit")
	}
	if _, err := os.Stat(sidecarDaemonMetadataPath(runtimeDir, id)); !os.IsNotExist(err) {
		t.Fatalf("idle daemon metadata remains: %v", err)
	}
}

// Catches a transform result being evicted before the compiler has requested
// its maps and released it. The short timeout exercises the production
// five-minute idle transition without making the test wait five minutes.
func TestSidecarDaemonPinsWorkerUntilResultRelease(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	t.Setenv("ROTOR_NODE_PATH", writeResultHandleNode(t))
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	const idleTimeout = 80 * time.Millisecond
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, idleTimeout) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), time.Second)
		defer cancel()
		_, _ = StopSidecarDaemons(ctx)
	})

	transformContext, cancelTransform := context.WithTimeout(context.Background(), time.Second)
	transform, err := SidecarDaemonRoundTrip(transformContext, SidecarDaemonCall{
		WorkspaceKey: projectDir,
		WorkerKey:    "tsconfig",
		ProjectDir:   projectDir,
		SidecarDir:   t.TempDir(),
		Payload:      []byte(`{"protocol":2,"operation":"transform"}`),
	})
	cancelTransform()
	if err != nil {
		t.Fatal(err)
	}
	var transformed struct {
		ResultHandle string `json:"resultHandle"`
	}
	if err := json.Unmarshal(transform.Payload, &transformed); err != nil {
		t.Fatal(err)
	}
	if transformed.ResultHandle != "result-1" {
		t.Fatalf("result handle = %q, want result-1", transformed.ResultHandle)
	}

	select {
	case err := <-done:
		t.Fatalf("daemon exited with retained result: %v", err)
	case <-time.After(3 * idleTimeout):
	}
	statusContext, cancelStatus := context.WithTimeout(context.Background(), time.Second)
	infos, err := SidecarDaemonStatus(statusContext)
	cancelStatus()
	if err != nil {
		t.Fatal(err)
	}
	if len(infos) != 1 || infos[0].WorkerCount != 1 {
		t.Fatalf("retained-result status = %+v, want one live worker", infos)
	}

	releasePayload, err := json.Marshal(map[string]any{
		"protocol": 2, "operation": "release",
		"resultHandle": transformed.ResultHandle, "outcome": "success",
	})
	if err != nil {
		t.Fatal(err)
	}
	releaseContext, cancelRelease := context.WithTimeout(context.Background(), time.Second)
	released, err := SidecarDaemonRoundTrip(releaseContext, SidecarDaemonCall{
		WorkspaceKey: projectDir,
		WorkerKey:    "tsconfig",
		ProjectDir:   projectDir,
		SidecarDir:   t.TempDir(),
		Payload:      releasePayload,
	})
	cancelRelease()
	if err != nil {
		t.Fatal(err)
	}
	var releaseResponse struct {
		Released bool `json:"released"`
	}
	if err := json.Unmarshal(released.Payload, &releaseResponse); err != nil {
		t.Fatal(err)
	}
	if !releaseResponse.Released {
		t.Fatal("fake Node worker did not acknowledge release")
	}

	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("daemon did not expire after result release")
	}
	if _, err := os.Stat(sidecarDaemonMetadataPath(runtimeDir, id)); !os.IsNotExist(err) {
		t.Fatalf("released daemon metadata remains: %v", err)
	}
}

// A later transform must not refresh the project session while an earlier
// build still owns transformed nodes for on-demand trace maps.
func TestSidecarDaemonSerializesTransformResultLifetimes(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	t.Setenv("ROTOR_NODE_PATH", writeResultHandleNode(t))
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), time.Second)
		defer cancel()
		_, _ = StopSidecarDaemons(ctx)
	})

	call := SidecarDaemonCall{
		WorkspaceKey: projectDir,
		WorkerKey:    "tsconfig",
		ProjectDir:   projectDir,
		SidecarDir:   t.TempDir(),
		Payload:      []byte(`{"protocol":2,"operation":"transform"}`),
		LeaseOwner:   "first-build",
	}
	first, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	var firstResponse struct {
		ResultHandle string `json:"resultHandle"`
	}
	if err := json.Unmarshal(first.Payload, &firstResponse); err != nil {
		t.Fatal(err)
	}

	sameBuild, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatalf("same-build transform was blocked by its retained result: %v", err)
	}
	var sameBuildResponse struct {
		ResultHandle string `json:"resultHandle"`
	}
	if err := json.Unmarshal(sameBuild.Payload, &sameBuildResponse); err != nil {
		t.Fatal(err)
	}
	secondCall := call
	secondCall.LeaseOwner = "second-build"
	type transformOutcome struct {
		result *SidecarDaemonResult
		err    error
	}
	secondDone := make(chan transformOutcome, 1)
	go func() {
		result, err := SidecarDaemonRoundTrip(context.Background(), secondCall)
		secondDone <- transformOutcome{result: result, err: err}
	}()
	select {
	case outcome := <-secondDone:
		t.Fatalf("second transform completed before release: %v", outcome.err)
	case <-time.After(100 * time.Millisecond):
	}

	release := call
	release.Payload, err = json.Marshal(map[string]any{
		"protocol": 2, "operation": "release",
		"resultHandle": firstResponse.ResultHandle, "outcome": "success",
	})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := SidecarDaemonRoundTrip(context.Background(), release); err != nil {
		t.Fatal(err)
	}
	select {
	case outcome := <-secondDone:
		t.Fatalf("second transform completed while the same build retained another result: %v", outcome.err)
	case <-time.After(50 * time.Millisecond):
	}
	release.Payload, err = json.Marshal(map[string]any{
		"protocol": 2, "operation": "release",
		"resultHandle": sameBuildResponse.ResultHandle, "outcome": "success",
	})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := SidecarDaemonRoundTrip(context.Background(), release); err != nil {
		t.Fatal(err)
	}
	var second transformOutcome
	select {
	case second = <-secondDone:
		if second.err != nil {
			t.Fatal(second.err)
		}
	case <-time.After(time.Second):
		t.Fatal("second transform remained blocked after release")
	}
	var secondResponse struct {
		ResultHandle string `json:"resultHandle"`
	}
	if err := json.Unmarshal(second.result.Payload, &secondResponse); err != nil {
		t.Fatal(err)
	}
	release.Payload, err = json.Marshal(map[string]any{
		"protocol": 2, "operation": "release",
		"resultHandle": secondResponse.ResultHandle, "outcome": "success",
	})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := SidecarDaemonRoundTrip(context.Background(), release); err != nil {
		t.Fatal(err)
	}
}

// A client can disappear after Node has accepted and completed a transform but
// before the result handle reaches the caller. The next build must still make
// progress because nobody can release a handle that was never delivered.
func TestSidecarDaemonReleasesAnUndeliveredResult(t *testing.T) {
	projectDir := t.TempDir()
	server := &sidecarDaemonServer{
		id: "test", idleTimeout: time.Minute,
		lastActive: time.Now(), idleDeadline: time.Now().Add(time.Minute),
		workers: map[string]*sidecarDaemonWorker{},
	}
	t.Cleanup(server.closeWorkers)
	serverConn, clientConn := net.Pipe()
	handled := make(chan struct{})
	go func() {
		server.handleConnection(serverConn)
		close(handled)
	}()

	request := sidecarDaemonMessage{
		Protocol:   sidecarDaemonProtocol,
		DaemonID:   "test",
		Kind:       "roundTrip",
		WorkerKey:  "tsconfig",
		ProjectDir: projectDir,
		SidecarDir: t.TempDir(),
		NodePath:   writeResultHandleNode(t),
		ChildEnv:   sidecarEnv(projectDir, t.TempDir()),
		LeaseOwner: "first-build",
		Payload:    json.RawMessage(`{"protocol":2,"operation":"transform"}`),
	}
	if err := json.NewEncoder(clientConn).Encode(request); err != nil {
		t.Fatal(err)
	}
	deadline := time.Now().Add(time.Second)
	for {
		server.mu.Lock()
		retained := server.retainedResultCountLocked()
		server.mu.Unlock()
		if retained == 1 {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("daemon did not retain the completed transform result")
		}
		time.Sleep(time.Millisecond)
	}
	if err := clientConn.Close(); err != nil {
		t.Fatal(err)
	}
	select {
	case <-handled:
	case <-time.After(time.Second):
		t.Fatal("daemon did not finish cleaning up the disconnected client")
	}

	nextRequest := request
	nextRequest.LeaseOwner = "second-build"
	next := server.roundTrip(nextRequest, context.Background())
	if next.Error != "" {
		t.Fatal(next.Error)
	}
	var response struct {
		ResultHandle string `json:"resultHandle"`
	}
	if err := json.Unmarshal(next.Payload, &response); err != nil {
		t.Fatal(err)
	}
	if response.ResultHandle == "" {
		t.Fatal("later transform returned no result handle")
	}
	releasePayload, err := json.Marshal(map[string]any{
		"protocol": 2, "operation": "release",
		"resultHandle": response.ResultHandle, "outcome": "success",
	})
	if err != nil {
		t.Fatal(err)
	}
	nextRequest.Payload = releasePayload
	if reply := server.roundTrip(nextRequest, context.Background()); reply.Error != "" {
		t.Fatal(reply.Error)
	}
}

// Catches the daemon placing a user-private socket through an attacker-chosen
// directory symlink. The expected rejection is the Unix ownership boundary.
func TestSidecarDaemonRejectsSymlinkIPCRoot(t *testing.T) {
	parent := t.TempDir()
	root := filepath.Join(parent, "ipc")
	if err := os.Symlink(t.TempDir(), root); err != nil {
		t.Fatal(err)
	}
	if err := ensurePrivateSidecarIPCRoot(root); err == nil {
		t.Fatal("symlink IPC root was accepted")
	}
}

// Catches one workspace retaining an unbounded number of idle Node processes.
// The limit of two is the issue's required resource bound.
func TestSidecarDaemonKeepsAtMostTwoIdleWorkers(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	t.Setenv("ROTOR_NODE_PATH", writeEchoNode(t))
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)

	for _, workerKey := range []string{"one", "two", "three"} {
		workerProjectDir := t.TempDir()
		_, err := SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
			WorkspaceKey: projectDir, WorkerKey: workerKey,
			ProjectDir: workerProjectDir, SidecarDir: t.TempDir(),
			Payload: []byte(`{"protocol":2,"operation":"transform"}`),
		})
		if err != nil {
			t.Fatal(err)
		}
	}
	infos, err := SidecarDaemonStatus(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(infos) != 1 || infos[0].WorkerCount != 2 {
		t.Fatalf("daemon status = %+v, want one daemon with two workers", infos)
	}

	if mode := filePermissions(t, runtimeDir); mode != 0o700 {
		t.Fatalf("runtime directory permissions = %o, want 700", mode)
	}
	if mode := filePermissions(t, sidecarDaemonEndpoint(runtimeDir, id)); mode != 0o600 {
		t.Fatalf("socket permissions = %o, want 600", mode)
	}
	if _, err := StopSidecarDaemons(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := <-done; err != nil {
		t.Fatal(err)
	}
}

type echoedChange struct {
	FileName string `json:"fileName"`
	Text     string `json:"text"`
	Deleted  bool   `json:"deleted"`
}

func decodeEchoedChanges(t *testing.T, payload []byte) []echoedChange {
	t.Helper()
	var response struct {
		ChangedFiles []echoedChange `json:"changedFiles"`
	}
	if err := json.Unmarshal(payload, &response); err != nil {
		t.Fatal(err)
	}
	return response.ChangedFiles
}

func writeEchoNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	contents := "#!/bin/sh\nwhile IFS= read -r line; do\n  printf '%s\\n' \"$line\"\ndone\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	return path
}

func writeConcurrencyNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	contents := "#!/bin/sh\nwhile IFS= read -r line; do\n  if mkdir \"$SIDECAR_TEST_STATE/active\" 2>/dev/null; then\n    if [ \"$SIDECAR_TEST_REQUIRE_OVERLAP\" = 1 ]; then\n      attempts=0\n      while [ ! -e \"$SIDECAR_TEST_STATE/overlap\" ] && [ \"$attempts\" -lt 200 ]; do\n        attempts=$((attempts + 1))\n        sleep 0.01\n      done\n    fi\n  else\n    touch \"$SIDECAR_TEST_STATE/overlap\"\n  fi\n  sleep 0.05\n  rmdir \"$SIDECAR_TEST_STATE/active\" 2>/dev/null || true\n  printf '%s\\n' \"$line\"\ndone\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	return path
}

func writeResultHandleNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	contents := "#!/bin/sh\ncount=0\nwhile IFS= read -r line; do\n  case \"$line\" in\n    *'\"operation\":\"release\"'*) printf '%s\\n' '{\"protocol\":2,\"released\":true}' ;;\n    *) count=$((count + 1)); printf '{\"protocol\":2,\"diagnostics\":[],\"resultHandle\":\"result-%s\"}\\n' \"$count\" ;;\n  esac\ndone\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	return path
}

func runConcurrentCalls(t *testing.T, projectDir string, workerKeys ...string) {
	t.Helper()
	start := make(chan struct{})
	errors := make(chan error, len(workerKeys))
	for index, workerKey := range workerKeys {
		go func() {
			<-start
			payload, _ := json.Marshal(map[string]any{"protocol": 2, "operation": "transform", "request": index})
			result, err := SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
				WorkspaceKey: projectDir, WorkerKey: workerKey,
				ProjectDir: projectDir, SidecarDir: projectDir, Payload: payload,
			})
			if err == nil {
				var response struct {
					Request int `json:"request"`
				}
				err = json.Unmarshal(result.Payload, &response)
				if err == nil && response.Request != index {
					err = fmt.Errorf("response request = %d, want %d", response.Request, index)
				}
			}
			errors <- err
		}()
	}
	close(start)
	for range workerKeys {
		if err := <-errors; err != nil {
			t.Fatal(err)
		}
	}
}

func waitForSidecarDaemon(t *testing.T, runtimeDir, id string, done <-chan error) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if _, err := readSidecarDaemonMetadata(runtimeDir, id); err == nil {
			return
		}
		select {
		case err := <-done:
			t.Fatalf("sidecar daemon exited during startup: %v", err)
		case <-time.After(10 * time.Millisecond):
		}
	}
	t.Fatal("sidecar daemon did not publish metadata")
}

func filePermissions(t *testing.T, path string) os.FileMode {
	t.Helper()
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	return info.Mode().Perm()
}
