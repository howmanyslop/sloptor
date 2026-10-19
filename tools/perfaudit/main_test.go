package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestCLIResidentModeRejectsMixedFlags(t *testing.T) {
	for _, args := range [][]string{
		{"--resident-plan", "x", "--resource-plan", "y", "--output", "z"},
		{"--resource-plan", "x", "--resident-plan=", "--output", "z"},
		{"--resident-plan", "x", "--input", "y", "--output", "z"},
		{"--resident-plan", "x", "--primary", cold, "--output", "z"},
		{"--resident-plan", "x", "--max-primary-ratio", ".95", "--output", "z"},
		{"--resident-plan", "x", "--cold-pairs", "30", "--output", "z"},
		{"--resident-plan", "x", "--no-change-pairs", "100", "--output", "z"},
		{"--resident-plan=", "--output", "z"},
		{"--resident-plan", "x"},
		{"--resident-plan", "x", "--output", "z", "extra"},
	} {
		var stdout, stderr bytes.Buffer
		if code := run(args, &stdout, &stderr); code != 2 || !strings.Contains(stderr.String(), "usage:") {
			t.Fatalf("mixed resident mode accepted: %v exit=%d %s", args, code, &stderr)
		}
	}
}

func TestCLIContractPreservesEvidence(t *testing.T) {
	dir := t.TempDir()
	input, output := filepath.Join(dir, "manifest.json"), filepath.Join(dir, "audit.json")
	data := fixture(t, 950, 1000, 30, 100)
	if err := os.WriteFile(input, data, 0o600); err != nil {
		t.Fatal(err)
	}
	args := []string{"--input", input, "--output", output, "--primary", cold, "--max-primary-ratio", "0.95"}
	var stdout, stderr bytes.Buffer
	if code := run(args, &stdout, &stderr); code != 0 {
		t.Fatalf("code %d: %s %s", code, &stdout, &stderr)
	}
	result, err := os.ReadFile(output)
	if err != nil {
		t.Fatal(err)
	}
	var r report
	if err := json.Unmarshal(result, &r); err != nil {
		t.Fatal(err)
	}
	if r.Outcome != criterionMet || !bytes.Equal(data, r.Input.Bytes) || len(r.Input.SHA256) != 64 || !strings.Contains(stdout.String(), "Legacy verdict required separately") {
		t.Fatalf("bad CLI contract: %s, %s", r.Outcome, &stdout)
	}
	if code := run(args, &stdout, &stderr); code != 2 {
		t.Fatal("existing output overwritten")
	}
	got, err := os.ReadFile(output)
	if err != nil || !bytes.Equal(got, result) {
		t.Fatal("previous report modified")
	}
	args[3] = input
	if code := run(args, &stdout, &stderr); code != 2 {
		t.Fatal("input overwritten")
	}
	got, err = os.ReadFile(input)
	if err != nil || !bytes.Equal(got, data) {
		t.Fatal("raw input modified")
	}
}

func TestCLIRequiresExplicitValidPolicy(t *testing.T) {
	for _, args := range [][]string{
		{"--input", "x", "--output", "y", "--primary", cold},
		{"--input", "x", "--output", "y", "--max-primary-ratio", ".95"},
		{"--input", "x", "--output", "y", "--primary", cold, "--max-primary-ratio", "NaN"},
		{"--input", "x", "--output", "y", "--primary", cold, "--max-primary-ratio", ".95", "--cold-pairs", "8"},
		{"--input", "x", "--output", "y", "--primary", cold, "--max-primary-ratio", ".95", "--no-change-pairs", "19"},
		{"--input", "x", "--output", "y", "--primary", cold, "--max-primary-ratio", ".95", "--cold-pairs", "31"},
	} {
		var stdout, stderr bytes.Buffer
		if code := run(args, &stdout, &stderr); code != 2 || !strings.Contains(stderr.String(), "usage:") {
			t.Fatalf("policy args accepted %v: %d %s", args, code, &stderr)
		}
	}
}

func TestCLIMalformedEvidenceProducesJSONAndFailure(t *testing.T) {
	dir := t.TempDir()
	input, output := filepath.Join(dir, "bad.json"), filepath.Join(dir, "audit.json")
	data := []byte(`{"schema":1} trailing`)
	if err := os.WriteFile(input, data, 0o600); err != nil {
		t.Fatal(err)
	}
	var stdout, stderr bytes.Buffer
	if code := run([]string{"--input", input, "--output", output, "--primary", cold, "--max-primary-ratio", ".95"}, &stdout, &stderr); code != 1 {
		t.Fatalf("code %d: %s", code, &stderr)
	}
	result, err := os.ReadFile(output)
	if err != nil {
		t.Fatal(err)
	}
	var r report
	if err := json.Unmarshal(result, &r); err != nil {
		t.Fatal(err)
	}
	if r.Outcome != invalidEvidence || !bytes.Equal(r.Input.Bytes, data) {
		t.Fatalf("malformed input lost: %+v", r)
	}
}
