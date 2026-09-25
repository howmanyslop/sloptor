package forkparity

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os/exec"
	"regexp"
	"strings"
)

func (r MatrixRunner) verifyUpstreamCorrection(ctx context.Context, ledger DivergenceRow, row *MatrixRowResult) error {
	if err := VerifyBehavioralTest(ctx, r.RepoRoot, ledger.BehavioralTest); err != nil {
		return fmt.Errorf("verify upstream correction %q: %w", ledger.ID, err)
	}
	drifts := row.Drifts
	row.Drifts = []MatrixDrift{}
	for _, drift := range drifts {
		if drift.Surface == MatrixSurfaceByte {
			row.ArchiveDifferences = append(row.ArchiveDifferences, drift)
		} else {
			row.Drifts = append(row.Drifts, drift)
		}
	}
	if len(row.Drifts) == 0 {
		row.Status = string(DivergenceUpstreamCorrected)
	}
	return nil
}

// VerifyBehavioralTest requires an actual passing test event. An exit-zero
// invocation with a missing or skipped test does not verify compatibility.
func VerifyBehavioralTest(ctx context.Context, repoRoot, test string) error {
	parts := strings.Split(test, "/")
	for i, part := range parts {
		parts[i] = "^" + regexp.QuoteMeta(part) + "$"
	}
	cmd := exec.CommandContext(ctx, "go", "test", "-json", "./internal/conformance",
		"-run", strings.Join(parts, "/"), "-count=1")
	cmd.Dir = repoRoot
	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("run behavioral test %q: %w\n%s", test, err, output)
	}

	passed := false
	decoder := json.NewDecoder(bytes.NewReader(output))
	for {
		var event struct {
			Action string
			Test   string
		}
		if err := decoder.Decode(&event); err == io.EOF {
			break
		} else if err != nil {
			return fmt.Errorf("read behavioral verification for %q: %w", test, err)
		}
		if event.Test == test && event.Action == "pass" {
			passed = true
		}
	}
	if !passed {
		return fmt.Errorf("missing pass event for %q; missing or skipped tests do not verify behavior", test)
	}
	return nil
}
