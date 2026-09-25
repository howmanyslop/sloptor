package main

import (
	"encoding/json"
	"io"
	"path/filepath"
	"time"
)

// watchEventWriter prints the `--watch --json` NDJSON stream: one object per
// line, each written in a single Write so a consumer reading a pipe always
// sees whole lines. Every buildStart is followed by exactly one buildEnd
// before the next buildStart; a consumer tracks "build in progress" as a
// toggle.
type watchEventWriter struct {
	enc  *json.Encoder
	root string           // changed paths are reported relative to this dir
	now  func() time.Time // injectable clock for tests
}

func newWatchEventWriter(w io.Writer, root string) *watchEventWriter {
	enc := json.NewEncoder(w)
	enc.SetEscapeHTML(false)
	return &watchEventWriter{enc: enc, root: root, now: time.Now}
}

type watchBuildStartEvent struct {
	Event   string   `json:"event"`
	At      string   `json:"at"`
	Changed []string `json:"changed"`
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
		if r, err := filepath.Rel(w.root, path); err == nil {
			path = r
		}
		rel = append(rel, filepath.ToSlash(path))
	}
	_ = w.enc.Encode(watchBuildStartEvent{Event: "buildStart", At: w.stamp(), Changed: rel})
}

func (w *watchEventWriter) buildEnd(res jsonResult) {
	if res.Diagnostics == nil {
		res.Diagnostics = []jsonDiagnostic{}
	}
	_ = w.enc.Encode(watchBuildEndEvent{Event: "buildEnd", At: w.stamp(), jsonResult: res})
}

func (w *watchEventWriter) stamp() string {
	return w.now().UTC().Format("2006-01-02T15:04:05.000Z")
}
