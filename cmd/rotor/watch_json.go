package main

import (
	"encoding/json"
	"io"
	"time"

	"rotor/internal/compile"
)

// watchEventWriter prints the `--watch --json` NDJSON stream: one object per
// line, each written in a single Write so a consumer reading a pipe always
// sees whole lines. Every buildStart is followed by exactly one buildEnd
// before the next buildStart; a consumer tracks "build in progress" as a
// toggle. The first event carries the sloptor version, and a single
// `watching` event follows the initial buildEnd.
type watchEventWriter struct {
	enc     *json.Encoder
	root    string           // changed paths are reported relative to this dir
	now     func() time.Time // injectable clock for tests
	started bool             // the first event (carrying version) was written
}

func newWatchEventWriter(w io.Writer, root string) *watchEventWriter {
	enc := json.NewEncoder(w)
	enc.SetEscapeHTML(false)
	return &watchEventWriter{enc: enc, root: root, now: time.Now}
}

type watchBuildStartEvent struct {
	Event   string   `json:"event"`
	At      string   `json:"at"`
	Version string   `json:"version,omitempty"`
	Changed []string `json:"changed"`
}

type watchWatchingEvent struct {
	Event string `json:"event"`
	At    string `json:"at"`
	Files int    `json:"files"`
}

// watchBuildEndEvent embeds jsonResult so a watch consumer and a one-shot
// `--json` consumer share one result and diagnostic shape.
type watchBuildEndEvent struct {
	Event string `json:"event"`
	At    string `json:"at"`
	jsonResult
}

// buildStart announces a build. changed is nil for the initial build, which
// reports an empty list.
func (w *watchEventWriter) buildStart(changed []string) {
	rel := make([]string, 0, len(changed))
	for _, path := range changed {
		rel = append(rel, relDisplay(w.root, path))
	}
	event := watchBuildStartEvent{Event: "buildStart", At: w.stamp(), Changed: rel}
	if !w.started {
		event.Version = version
		w.started = true
	}
	_ = w.enc.Encode(event)
}

func (w *watchEventWriter) buildEnd(res jsonResult) {
	_ = w.enc.Encode(watchBuildEndEvent{Event: "buildEnd", At: w.stamp(), jsonResult: res})
}

// watching reports that startup is done and how many files are watched.
func (w *watchEventWriter) watching(files func() int) {
	_ = w.enc.Encode(watchWatchingEvent{Event: "watching", At: w.stamp(), Files: files()})
}

func (w *watchEventWriter) stamp() string {
	return w.now().UTC().Format("2006-01-02T15:04:05.000Z")
}

// jsonBuildWatchReporter is the `build --watch --json` reporter.
type jsonBuildWatchReporter struct{ *watchEventWriter }

func newBuildWatchJSONReporter(w io.Writer, dir string) buildWatchReporter {
	return &jsonBuildWatchReporter{newWatchEventWriter(w, dir)}
}

func (r *jsonBuildWatchReporter) buildEnd(result *compile.BuildResult, diags []compile.DiagnosticInfo, elapsed time.Duration, err error) {
	r.watchEventWriter.buildEnd(buildJSONResult(r.root, result, diags, elapsed, err))
}

// jsonCheckWatchReporter is the `check --watch --json` reporter.
type jsonCheckWatchReporter struct{ *watchEventWriter }

func newCheckWatchJSONReporter(w io.Writer, dir string) checkWatchReporter {
	return &jsonCheckWatchReporter{newWatchEventWriter(w, dir)}
}

func (r *jsonCheckWatchReporter) buildEnd(core checkCore) {
	r.watchEventWriter.buildEnd(checkJSONResult(core))
}
