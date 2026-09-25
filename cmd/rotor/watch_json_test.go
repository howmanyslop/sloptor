package main

import (
	"errors"
	"testing"
	"time"

	"rotor/internal/compile"
)

func TestBuildJSONResultSuccessCountsOutputs(t *testing.T) {
	result := &compile.BuildResult{Outputs: map[string]string{"a.luau": "", "b.luau": ""}}

	res := buildJSONResult(result, nil, 142*time.Millisecond, nil)

	if !res.OK || res.Files != 2 || res.DurationMs != 142 || res.Version != version {
		t.Errorf("res = %+v, want ok, 2 files, 142 ms, version %q", res, version)
	}
	if res.Diagnostics == nil || len(res.Diagnostics) != 0 {
		t.Errorf("diagnostics = %#v, want empty non-nil", res.Diagnostics)
	}
}

func TestBuildJSONResultFailureWithoutDiagnosticsReportsError(t *testing.T) {
	res := buildJSONResult(nil, nil, 0, errors.New("config broke"))

	if res.OK {
		t.Error("ok = true on failure")
	}
	if len(res.Diagnostics) != 1 || res.Diagnostics[0].Message != "config broke" || res.Diagnostics[0].Severity != "error" {
		t.Errorf("diagnostics = %+v, want one error from err", res.Diagnostics)
	}
}

func TestBuildJSONResultFailureMapsDiagnostics(t *testing.T) {
	diags := []compile.DiagnosticInfo{{Code: "TS2322", Message: "bad"}, {Message: "meh", Warning: true}}

	res := buildJSONResult(nil, diags, 0, errors.New("build failed"))

	if len(res.Diagnostics) != 2 {
		t.Fatalf("diagnostics = %+v, want 2", res.Diagnostics)
	}
	if got := res.Diagnostics[0]; got.Code != "TS2322" || got.Severity != "error" || got.Message != "bad" {
		t.Errorf("diag[0] = %+v", got)
	}
	if got := res.Diagnostics[1]; got.Severity != "warning" {
		t.Errorf("diag[1] severity = %q, want warning", got.Severity)
	}
}
