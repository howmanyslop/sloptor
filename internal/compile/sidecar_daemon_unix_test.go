//go:build !windows

package compile

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
	"unicode/utf16"

	"rotor/internal/logservice"
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
	sourceText := "export const value = 1;\n"
	removedText := "export const removed = true;\n"
	if err := os.WriteFile(sourcePath, utf16LESource(sourceText), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(removedFromProjectPath, []byte(removedText), 0o600); err != nil {
		t.Fatal(err)
	}
	identities := map[string]string{
		sourcePath:             "5d8f65d2774e206bc9f7a7a4ad39ca2dc563b5c31e46ab57ef4874961237ce29",
		removedFromProjectPath: "d8b0cb855c04ec4bf715854d923a4735a033752635c60721d37758700057c36b",
	}
	call := SidecarDaemonCall{
		WorkspaceKey: projectDir,
		WorkerKey:    "tsconfig",
		ProjectDir:   projectDir,
		SidecarDir:   sidecarDir,
		Payload:      daemonTransformPayload(t, identities),
		StampFileNames: []string{
			sourcePath,
			removedFromProjectPath,
		},
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
	if err := os.Chtimes(sourcePath, beforeRewrite.ModTime().Add(-time.Hour), beforeRewrite.ModTime()); err != nil {
		t.Fatal(err)
	}
	unchanged, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	if changed := decodeEchoedChanges(t, unchanged.Payload); len(changed) != 0 {
		t.Fatalf("access-time-only change dirtied source files: %+v", changed)
	}

	wantText := "export const value = 2;\n"
	if err := os.WriteFile(sourcePath, utf16LESource(wantText), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Chtimes(sourcePath, beforeRewrite.ModTime(), beforeRewrite.ModTime()); err != nil {
		t.Fatal(err)
	}
	identities[sourcePath] = "f4918c8ac9858f83b2c0307536179d6bd283bc7c20ba34b53074721f43611f4a"
	call.Payload = daemonTransformPayload(t, identities)
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
	delete(identities, sourcePath)
	call.StampFileNames = []string{removedFromProjectPath}
	call.Payload = daemonTransformPayload(t, identities)
	third, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	changed = decodeEchoedChanges(t, third.Payload)
	if len(changed) != 1 || changed[0].FileName != sourcePath || !changed[0].Deleted {
		t.Fatalf("changed files = %+v, want on-disk deletion record", changed)
	}

	call.StampFileNames = nil
	delete(identities, removedFromProjectPath)
	call.Payload = daemonTransformPayload(t, identities)
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

func TestSidecarDaemonRetriesUnacceptedDiskChanges(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	sidecarDir := t.TempDir()
	t.Setenv("ROTOR_NODE_PATH", writeEchoNode(t))
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		<-done
	})

	overlayPath := filepath.Join(projectDir, "overlay.ts")
	if err := os.WriteFile(overlayPath, []byte("disk-one\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	call := SidecarDaemonCall{
		WorkspaceKey:   projectDir,
		WorkerKey:      "transactional-retry",
		ProjectDir:     projectDir,
		SidecarDir:     sidecarDir,
		StampFileNames: []string{overlayPath},
		Overlays:       map[string]string{overlayPath: "overlay!\n"},
		Payload: daemonTransformPayload(t, map[string]string{
			overlayPath: sidecarTextContentIdentity("overlay!\n"),
		}),
	}
	if _, err := SidecarDaemonRoundTrip(context.Background(), call); err != nil {
		t.Fatal(err)
	}

	call.Overlays = nil
	call.Payload = daemonTransformPayload(t, map[string]string{
		overlayPath: sidecarTextContentIdentity("disk-two\n"),
	})
	if _, err := SidecarDaemonRoundTrip(context.Background(), call); err == nil {
		t.Fatal("dropped overlay with mismatched disk text succeeded")
	}
	if err := os.WriteFile(overlayPath, []byte("disk-two\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	retried, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	changed := decodeEchoedChanges(t, retried.Payload)
	if len(changed) != 1 || changed[0].FileName != overlayPath || changed[0].Text != "disk-two\n" {
		t.Fatalf("dropped overlay retry changes = %+v", changed)
	}

	firstPath := filepath.Join(projectDir, "first.ts")
	lastPath := filepath.Join(projectDir, "last.ts")
	for _, fileName := range []string{firstPath, lastPath} {
		if err := os.WriteFile(fileName, []byte("before\n"), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	identities := map[string]string{
		firstPath: sidecarTextContentIdentity("before\n"),
		lastPath:  sidecarTextContentIdentity("before\n"),
	}
	call.WorkerKey = "later-mismatch"
	call.StampFileNames = []string{firstPath, lastPath}
	call.Payload = daemonTransformPayload(t, identities)
	if _, err := SidecarDaemonRoundTrip(context.Background(), call); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(firstPath, []byte("first!\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(lastPath, []byte("last!!\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	identities[firstPath] = sidecarTextContentIdentity("first!\n")
	identities[lastPath] = sidecarTextContentIdentity("wanted\n")
	call.Payload = daemonTransformPayload(t, identities)
	if _, err := SidecarDaemonRoundTrip(context.Background(), call); err == nil {
		t.Fatal("later mismatched source succeeded")
	}
	identities[lastPath] = sidecarTextContentIdentity("last!!\n")
	call.Payload = daemonTransformPayload(t, identities)
	retried, err = SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	changed = decodeEchoedChanges(t, retried.Payload)
	if len(changed) != 2 || changed[0].FileName != firstPath || changed[0].Text != "first!\n" || changed[1].FileName != lastPath || changed[1].Text != "last!!\n" {
		t.Fatalf("later mismatch retry changes = %+v", changed)
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
		Payload:      []byte(`{"protocol":3,"operation":"warm"}`),
	}
	if _, err := SidecarDaemonRoundTrip(context.Background(), call); err != nil {
		t.Fatal(err)
	}
	call.Payload = daemonTransformPayload(t, map[string]string{
		sourcePath: "5d8f65d2774e206bc9f7a7a4ad39ca2dc563b5c31e46ab57ef4874961237ce29",
	})
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
			Payload:      []byte(`{"protocol":3,"operation":"transform"}`),
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

// Catches a later compiler invocation with a different TMPDIR treating a
// daemon in the shared runtime registry as corrupt. The runtime directory is
// the cross-process contract, so status must find the daemon regardless of a
// caller's private temporary directory.
func TestSidecarDaemonSharesRuntimeRegistryAcrossTMPDIRs(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	t.Setenv("TMPDIR", t.TempDir())
	projectDir := t.TempDir()
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	t.Setenv("TMPDIR", t.TempDir())
	infos, err := SidecarDaemonStatus(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(infos) != 1 || infos[0].ID != id {
		t.Fatalf("daemon status = %+v, want the registered daemon", infos)
	}

	stopped, err := StopSidecarDaemons(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if stopped != 1 {
		t.Fatalf("stopped = %d, want 1", stopped)
	}
}

// Catches daemon stop reporting success while a worker keeps transforming.
// A successful stop must cancel active client work and remove the daemon
// registry before the command counts it as stopped.
func TestSidecarDaemonStopCancelsActiveWorkerBeforeReportingSuccess(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 30*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	stateDir := t.TempDir()
	sidecarDir := t.TempDir()
	nodePath := writeBlockingNode(t)
	childEnv := append(sidecarEnv(projectDir, sidecarDir), "SIDECAR_TEST_STATE="+stateDir)
	requestDone := make(chan error, 1)
	go func() {
		_, callErr := SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
			WorkspaceKey: projectDir,
			WorkerKey:    "blocking-worker",
			ProjectDir:   projectDir,
			SidecarDir:   sidecarDir,
			NodePath:     nodePath,
			ChildEnv:     childEnv,
			Payload:      []byte(`{"operation":"transform"}`),
		})
		requestDone <- callErr
	}()
	startedPath := filepath.Join(stateDir, "started")
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if _, err := os.Stat(startedPath); err == nil {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	if _, err := os.Stat(startedPath); err != nil {
		t.Fatalf("blocking worker did not start: %v", err)
	}

	stopContext, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	stopped, err := StopSidecarDaemons(stopContext)
	if err != nil {
		t.Fatal(err)
	}
	if stopped != 1 {
		t.Fatalf("stopped = %d, want 1", stopped)
	}
	select {
	case requestErr := <-requestDone:
		if requestErr == nil {
			t.Fatal("active sidecar request succeeded after daemon stop")
		}
	case <-time.After(time.Second):
		t.Fatal("active sidecar request did not finish after daemon stop")
	}
	if _, err := os.Stat(sidecarDaemonMetadataPath(runtimeDir, id)); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("stopped daemon metadata remains: %v", err)
	}
}

// Catches a type-incompatible transform response leaving its Node stream alive
// because it has no result handle. The abandoned request must recycle the
// generation that produced it, while a delayed repeat must not close the
// replacement generation serving later requests.
func TestSidecarDaemonAbandonsTypeIncompatibleTransformWithoutKillingReplacement(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	call := SidecarDaemonCall{
		WorkspaceKey: projectDir,
		WorkerKey:    "malformed-worker",
		ProjectDir:   projectDir,
		SidecarDir:   t.TempDir(),
		NodePath:     writeInvalidTypedResponseThenGenerationNode(t),
		LeaseOwner:   "malformed-response-client",
		Payload:      []byte(`{"operation":"transform"}`),
	}
	typeIncompatible, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	typeIncompatible.abandon()

	replacement, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	if generation := workerGenerationFromResponse(t, replacement.Payload); generation != 2 {
		t.Fatalf("replacement worker generation = %d, want 2", generation)
	}

	typeIncompatible.abandon()
	continued, err := SidecarDaemonRoundTrip(context.Background(), call)
	if err != nil {
		t.Fatal(err)
	}
	if generation := workerGenerationFromResponse(t, continued.Payload); generation != 2 {
		t.Fatalf("stale abandon restarted replacement worker at generation %d", generation)
	}
}

// Catches invalid Node JSON being embedded in the daemon envelope. The daemon
// must return a protocol error and recycle the worker before a later request
// can reuse it.
func TestSidecarDaemonRecyclesInvalidJSONResponse(t *testing.T) {
	projectDir := t.TempDir()
	server := &sidecarDaemonServer{
		id: "test", idleTimeout: time.Minute,
		lastActive: time.Now(), idleDeadline: time.Now().Add(time.Minute),
		context: context.Background(), workers: map[string]*sidecarDaemonWorker{},
	}
	t.Cleanup(server.closeWorkers)
	call := sidecarDaemonMessage{
		Protocol:   sidecarDaemonProtocol,
		DaemonID:   "test",
		Kind:       "roundTrip",
		WorkerKey:  "invalid-json-worker",
		ProjectDir: projectDir,
		SidecarDir: t.TempDir(),
		NodePath:   writeMalformedThenGenerationNode(t),
		ChildEnv:   sidecarEnv(projectDir, t.TempDir()),
		RequestID:  "invalid-json-request",
		Payload:    json.RawMessage(`{"operation":"transform"}`),
	}
	reply := server.roundTrip(call, context.Background())
	if reply.Error == "" {
		t.Fatal("invalid Node JSON completed successfully")
	}
	var envelope bytes.Buffer
	if err := writeSidecarDaemonReply(&envelope, reply); err != nil {
		t.Fatalf("encode daemon protocol error: %v", err)
	}
	var decoded sidecarDaemonReply
	if err := json.Unmarshal(envelope.Bytes(), &decoded); err != nil || decoded.Error == "" {
		t.Fatalf("decode daemon protocol error = %+v, %v", decoded, err)
	}
	next := server.roundTrip(call, context.Background())
	if next.Error != "" {
		t.Fatal(next.Error)
	}
	if generation := workerGenerationFromResponse(t, next.Payload); generation != 2 {
		t.Fatalf("replacement worker generation = %d, want 2", generation)
	}
}

// Catches typed response decoding leaving a type-incompatible transform's
// stream in service. The next transform must start a replacement worker after
// the decoder abandons the unusable response.
func TestPersistentSidecarRequestRecyclesTypeIncompatibleTransformResponse(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	sidecarDir := t.TempDir()
	nodePath := writeInvalidTypedResponseThenGenerationNode(t)
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	identity := sidecarWorkerIdentity{
		WorkspaceKey: projectDir,
		WorkerKey:    "typed-malformed-worker",
		ProjectDir:   projectDir,
		SidecarDir:   sidecarDir,
		NodePath:     nodePath,
		ChildEnv:     sidecarEnv(projectDir, sidecarDir),
	}
	request := sidecarRequest{
		Protocol:   sidecarNodeProtocolVersion,
		Operation:  "transform",
		leaseOwner: "typed-malformed-client",
	}
	if _, _, err := persistentSidecarRequest(identity, request, nil, nil, time.Second); err == nil {
		t.Fatal("type-incompatible transform response decoded successfully")
	}

	replacement, err := SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
		WorkspaceKey: identity.WorkspaceKey,
		WorkerKey:    identity.WorkerKey,
		ProjectDir:   identity.ProjectDir,
		SidecarDir:   identity.SidecarDir,
		NodePath:     identity.NodePath,
		ChildEnv:     identity.ChildEnv,
		LeaseOwner:   request.leaseOwner,
		Payload:      []byte(`{"operation":"transform"}`),
	})
	if err != nil {
		t.Fatal(err)
	}
	if generation := workerGenerationFromResponse(t, replacement.Payload); generation != 2 {
		t.Fatalf("replacement worker generation = %d, want 2", generation)
	}
}

// Catches validation leaving its type-incompatible worker response in the
// shared stream. Validation has no result lease, but its next worker operation
// must still use a replacement after typed decoding rejects the response.
func TestPersistentSidecarRequestRecyclesTypeIncompatibleValidationResponse(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	sidecarDir := t.TempDir()
	nodePath := writeInvalidTypedResponseThenGenerationNode(t)
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	identity := sidecarWorkerIdentity{
		WorkspaceKey: projectDir,
		WorkerKey:    "typed-malformed-validation-worker",
		ProjectDir:   projectDir,
		SidecarDir:   sidecarDir,
		NodePath:     nodePath,
		ChildEnv:     sidecarEnv(projectDir, sidecarDir),
	}
	request := sidecarRequest{
		Protocol:  sidecarNodeProtocolVersion,
		Operation: "validate",
	}
	if _, _, err := persistentSidecarRequest(identity, request, nil, nil, time.Second); err == nil {
		t.Fatal("type-incompatible validation response decoded successfully")
	}

	replacement, err := SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
		WorkspaceKey: identity.WorkspaceKey,
		WorkerKey:    identity.WorkerKey,
		ProjectDir:   identity.ProjectDir,
		SidecarDir:   identity.SidecarDir,
		NodePath:     identity.NodePath,
		ChildEnv:     identity.ChildEnv,
		Payload:      []byte(`{"operation":"transform"}`),
	})
	if err != nil {
		t.Fatal(err)
	}
	if generation := workerGenerationFromResponse(t, replacement.Payload); generation != 2 {
		t.Fatalf("replacement worker generation = %d, want 2", generation)
	}
}

// Catches a type-incompatible release response leaving a worker alive after
// its result handle has already been removed. A leased control request owns
// its own cleanup token, so the next transform must use a replacement worker.
func TestPersistentSidecarRequestRecyclesTypeIncompatibleReleaseResponse(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	sidecarDir := t.TempDir()
	nodePath := writeMalformedReleaseThenGenerationNode(t)
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	identity := sidecarWorkerIdentity{
		WorkspaceKey: projectDir,
		WorkerKey:    "typed-malformed-release-worker",
		ProjectDir:   projectDir,
		SidecarDir:   sidecarDir,
		NodePath:     nodePath,
		ChildEnv:     sidecarEnv(projectDir, sidecarDir),
	}
	transform := sidecarRequest{
		Protocol:   sidecarNodeProtocolVersion,
		Operation:  "transform",
		leaseOwner: "typed-malformed-release-client",
	}
	response, _, err := persistentSidecarRequest(identity, transform, nil, nil, time.Second)
	if err != nil {
		t.Fatal(err)
	}
	if response.ResultHandle != "leased" {
		t.Fatalf("transform result handle = %q, want leased", response.ResultHandle)
	}
	release := sidecarRequest{
		Protocol:     sidecarNodeProtocolVersion,
		Operation:    "release",
		ResultHandle: response.ResultHandle,
		leaseOwner:   transform.leaseOwner,
	}
	if _, _, err := persistentSidecarRequest(identity, release, nil, nil, time.Second); err == nil {
		t.Fatal("type-incompatible release response decoded successfully")
	}

	replacement, err := SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
		WorkspaceKey: identity.WorkspaceKey,
		WorkerKey:    identity.WorkerKey,
		ProjectDir:   identity.ProjectDir,
		SidecarDir:   identity.SidecarDir,
		NodePath:     identity.NodePath,
		ChildEnv:     identity.ChildEnv,
		LeaseOwner:   transform.leaseOwner,
		Payload:      []byte(`{"operation":"transform"}`),
	})
	if err != nil {
		t.Fatal(err)
	}
	if generation := workerGenerationFromResponse(t, replacement.Payload); generation != 2 {
		t.Fatalf("replacement worker generation = %d, want 2", generation)
	}
}

// Catches daemon status observing a half-reset worker while a worker exits
// between requests. Status is a live command contract and must remain usable
// while clients restart failed workers.
func TestSidecarDaemonStatusDuringWorkerRestart(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	statusDone := make(chan error, 1)
	go func() {
		for range 100 {
			infos, statusErr := SidecarDaemonStatus(context.Background())
			if statusErr != nil {
				statusDone <- statusErr
				return
			}
			if len(infos) != 1 || infos[0].ID != id || infos[0].WorkerCount > 1 {
				statusDone <- fmt.Errorf("status = %+v", infos)
				return
			}
		}
		statusDone <- nil
	}()
	sidecarDir := t.TempDir()
	nodePath := writeOneShotNode(t)
	for range 100 {
		_, _ = SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
			WorkspaceKey: projectDir,
			WorkerKey:    "restart-worker",
			ProjectDir:   projectDir,
			SidecarDir:   sidecarDir,
			NodePath:     nodePath,
			Payload:      []byte(`{"operation":"transform"}`),
		})
	}
	if err := <-statusDone; err != nil {
		t.Fatal(err)
	}
}

// Catches a persistent worker's raw stderr being discarded by the broker.
// The requesting compiler owns the log channel, so its log output must include
// the diagnostic emitted before the worker's protocol response.
func TestSidecarDaemonForwardsRawWorkerStderrToRequestClient(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	var output bytes.Buffer
	previousOutput := logservice.Output
	logservice.Output = &output
	t.Cleanup(func() { logservice.Output = previousOutput })
	_, err = SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
		WorkspaceKey: projectDir,
		WorkerKey:    "stderr-worker",
		ProjectDir:   projectDir,
		SidecarDir:   t.TempDir(),
		NodePath:     writeStderrNode(t),
		Payload:      []byte(`{"operation":"transform"}`),
	})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), "worker raw diagnostic\n") {
		t.Fatalf("request log output = %q, want worker diagnostic", output.String())
	}
}

// Catches the request log buffer silently dropping ordinary plugin output once
// it exceeds the diagnostic tail. Every raw line produced before one protocol
// response belongs to that requesting compiler, including the first and last
// entries in a long request.
func TestSidecarDaemonForwardsEveryRawWorkerLogForOneRequest(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	var output bytes.Buffer
	previousOutput := logservice.Output
	logservice.Output = &output
	t.Cleanup(func() { logservice.Output = previousOutput })
	_, err = SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
		WorkspaceKey: projectDir,
		WorkerKey:    "many-stderr-worker",
		ProjectDir:   projectDir,
		SidecarDir:   t.TempDir(),
		NodePath:     writeManyStderrNode(t),
		Payload:      []byte(`{"operation":"transform"}`),
	})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), "worker log 1\n") || !strings.Contains(output.String(), "worker log 75\n") {
		t.Fatalf("request log output did not preserve the first and last raw worker logs: %q", output.String())
	}
}

// Catches a failed worker reply discarding diagnostics that were emitted
// before the protocol failure. Error replies are still replies to the
// requesting compiler and must drain their raw stderr exactly once.
func TestSidecarDaemonForwardsRawWorkerStderrOnRequestFailure(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	var output bytes.Buffer
	previousOutput := logservice.Output
	logservice.Output = &output
	t.Cleanup(func() { logservice.Output = previousOutput })
	_, err = SidecarDaemonRoundTrip(context.Background(), SidecarDaemonCall{
		WorkspaceKey: projectDir,
		WorkerKey:    "failing-stderr-worker",
		ProjectDir:   projectDir,
		SidecarDir:   t.TempDir(),
		NodePath:     writeFailingStderrNode(t),
		Payload:      []byte(`{"operation":"transform"}`),
	})
	if err == nil {
		t.Fatal("failed worker request succeeded")
	}
	if !strings.Contains(output.String(), "worker failure diagnostic\n") {
		t.Fatalf("failed request log output = %q, want worker diagnostic", output.String())
	}
}

// Catches one oversized raw diagnostic stopping the stderr reader and blocking
// the worker before it can return its protocol response. The later marker is a
// separate worker diagnostic, so its presence proves the whole raw stream was
// drained by the requesting compiler.
func TestSidecarDaemonDrainsOversizedRawWorkerStderr(t *testing.T) {
	runtimeDir := t.TempDir()
	t.Setenv(sidecarDaemonRuntimeEnv, runtimeDir)
	projectDir := t.TempDir()
	id, err := sidecarDaemonID(projectDir)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- runSidecarDaemon(runtimeDir, id, 2*time.Second) }()
	waitForSidecarDaemon(t, runtimeDir, id, done)
	t.Cleanup(func() {
		_, _ = StopSidecarDaemons(context.Background())
		select {
		case <-done:
		case <-time.After(3 * time.Second):
		}
	})

	var output bytes.Buffer
	previousOutput := logservice.Output
	logservice.Output = &output
	t.Cleanup(func() { logservice.Output = previousOutput })
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_, err = SidecarDaemonRoundTrip(ctx, SidecarDaemonCall{
		WorkspaceKey: projectDir,
		WorkerKey:    "oversized-stderr-worker",
		ProjectDir:   projectDir,
		SidecarDir:   t.TempDir(),
		NodePath:     writeOversizedStderrNode(t),
		Payload:      []byte(`{"operation":"transform"}`),
	})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), "worker oversized diagnostic\n") {
		t.Fatal("request log output did not contain the post-diagnostic marker")
	}
}

// Catches a valid relative Node override being resolved after the child
// changes into the project directory. The caller's relative executable must
// still start and return the worker's independent echo response.
func TestSpawnSidecarSessionResolvesRelativeNodeOverrideBeforeChangingDirectory(t *testing.T) {
	nodePath := writeEchoNode(t)
	workingDir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	relativeNodePath, err := filepath.Rel(workingDir, nodePath)
	if err != nil {
		t.Fatal(err)
	}
	t.Setenv("ROTOR_NODE_PATH", relativeNodePath)
	session, err := spawnSidecarSession(t.TempDir(), t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	defer session.close()
	line, err := session.writeAndRead(context.Background(), []byte(`{"request":"works"}`))
	if err != nil {
		t.Fatal(err)
	}
	if string(line) != "{\"request\":\"works\"}\n" {
		t.Fatalf("worker response = %q, want echoed request", line)
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
		Payload:      []byte(`{"protocol":3,"operation":"transform"}`),
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
		"protocol": 3, "operation": "release",
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
		Payload:      []byte(`{"protocol":3,"operation":"transform"}`),
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
		"protocol": 3, "operation": "release",
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
		"protocol": 3, "operation": "release",
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
		"protocol": 3, "operation": "release",
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
		context: context.Background(), workers: map[string]*sidecarDaemonWorker{},
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
		RequestID:  "undelivered-result",
		Payload:    json.RawMessage(`{"protocol":3,"operation":"transform"}`),
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
		"protocol": 3, "operation": "release",
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
			Payload: []byte(`{"protocol":3,"operation":"transform"}`),
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

func daemonTransformPayload(t *testing.T, identities map[string]string) []byte {
	t.Helper()
	payload, err := json.Marshal(map[string]any{
		"protocol":              sidecarNodeProtocolVersion,
		"operation":             "transform",
		"fileContentIdentities": identities,
	})
	if err != nil {
		t.Fatal(err)
	}
	return payload
}

func utf16LESource(text string) []byte {
	units := utf16.Encode([]rune(text))
	contents := make([]byte, 2+len(units)*2)
	contents[0], contents[1] = 0xff, 0xfe
	for index, unit := range units {
		binary.LittleEndian.PutUint16(contents[2+index*2:], unit)
	}
	return contents
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

func writeBlockingNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	contents := "#!/bin/sh\ntouch \"$SIDECAR_TEST_STATE/started\"\nwhile :; do sleep 1; done\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	return path
}

func writeStderrNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	contents := "#!/bin/sh\nwhile IFS= read -r line; do\n  printf '%s\\n' 'worker raw diagnostic' >&2\n  sleep 0.05\n  printf '%s\\n' \"$line\"\ndone\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	return path
}

func writeOneShotNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	contents := "#!/bin/sh\nIFS= read -r line\nprintf '%s\\n' \"$line\"\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	return path
}

func writeMalformedThenGenerationNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	statePath := filepath.Join(t.TempDir(), "generation")
	contents := "#!/bin/sh\ngeneration=0\nif [ -f \"$SIDECAR_TEST_GENERATION\" ]; then generation=$(cat \"$SIDECAR_TEST_GENERATION\"); fi\ngeneration=$((generation + 1))\nprintf '%s' \"$generation\" > \"$SIDECAR_TEST_GENERATION\"\nwhile IFS= read -r line; do\n  if [ \"$generation\" -eq 1 ]; then\n    printf '%s\\n' 'not-json'\n  else\n    printf '{\"generation\":%s}\\n' \"$generation\"\n  fi\ndone\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("SIDECAR_TEST_GENERATION", statePath)
	return path
}

func writeInvalidTypedResponseThenGenerationNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	statePath := filepath.Join(t.TempDir(), "generation")
	contents := "#!/bin/sh\ngeneration=0\nif [ -f \"$SIDECAR_TEST_GENERATION\" ]; then generation=$(cat \"$SIDECAR_TEST_GENERATION\"); fi\ngeneration=$((generation + 1))\nprintf '%s' \"$generation\" > \"$SIDECAR_TEST_GENERATION\"\nwhile IFS= read -r line; do\n  if [ \"$generation\" -eq 1 ]; then\n    printf '%s\\n' '{\"transformed\":false}'\n  else\n    printf '{\"generation\":%s}\\n' \"$generation\"\n  fi\ndone\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("SIDECAR_TEST_GENERATION", statePath)
	return path
}

func writeMalformedReleaseThenGenerationNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	statePath := filepath.Join(t.TempDir(), "generation")
	contents := "#!/bin/sh\ngeneration=0\nif [ -f \"$SIDECAR_TEST_GENERATION\" ]; then generation=$(cat \"$SIDECAR_TEST_GENERATION\"); fi\ngeneration=$((generation + 1))\nprintf '%s' \"$generation\" > \"$SIDECAR_TEST_GENERATION\"\nrequest=0\nwhile IFS= read -r line; do\n  request=$((request + 1))\n  if [ \"$generation\" -eq 1 ] && [ \"$request\" -eq 1 ]; then\n    printf '%s\\n' '{\"resultHandle\":\"leased\"}'\n  elif [ \"$generation\" -eq 1 ]; then\n    printf '%s\\n' '{\"transformed\":false}'\n  else\n    printf '{\"generation\":%s}\\n' \"$generation\"\n  fi\ndone\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("SIDECAR_TEST_GENERATION", statePath)
	return path
}

func workerGenerationFromResponse(t *testing.T, payload []byte) int {
	t.Helper()
	var response struct {
		Generation int `json:"generation"`
	}
	if err := json.Unmarshal(payload, &response); err != nil {
		t.Fatal(err)
	}
	return response.Generation
}

func writeOversizedStderrNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	contents := "#!/bin/sh\nwhile IFS= read -r line; do\n  dd if=/dev/zero bs=1024 count=1100 2>/dev/null | tr '\\000' x >&2\n  printf '\\nworker oversized diagnostic\\n' >&2\n  sleep 0.05\n  printf '%s\\n' \"$line\"\ndone\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	return path
}

func writeManyStderrNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	contents := "#!/bin/sh\nwhile IFS= read -r line; do\n  count=1\n  while [ \"$count\" -le 75 ]; do\n    printf 'worker log %s\\n' \"$count\" >&2\n    count=$((count + 1))\n  done\n  sleep 0.05\n  printf '%s\\n' \"$line\"\ndone\n"
	if err := os.WriteFile(path, []byte(contents), 0o700); err != nil {
		t.Fatal(err)
	}
	return path
}

func writeFailingStderrNode(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "node")
	contents := "#!/bin/sh\nIFS= read -r line\nprintf '%s\\n' 'worker failure diagnostic' >&2\nsleep 0.05\nexit 1\n"
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
			payload, _ := json.Marshal(map[string]any{"protocol": 3, "operation": "transform", "request": index})
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

func filePermissions(t *testing.T, path string) os.FileMode {
	t.Helper()
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	return info.Mode().Perm()
}
