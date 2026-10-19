package main

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
)

func TestStrictWholeJSON(t *testing.T) {
	input := fixture(t, 900, 1000, 10, 20)
	for name, raw := range map[string][]byte{
		"trailing document":           append(bytes.Clone(input), []byte(` {}`)...),
		"trailing garbage":            append(bytes.Clone(input), '!'),
		"duplicate schema":            bytes.Replace(input, []byte(`"schema":1`), []byte(`"schema":1,"schema":1`), 1),
		"escaped duplicate":           bytes.Replace(input, []byte(`"schema":1`), []byte(`"schema":1,"\u0073chema":1`), 1),
		"case shadow":                 bytes.Replace(input, []byte(`"schema":1`), []byte(`"schema":1,"Schema":1`), 1),
		"nested duplicate":            bytes.Replace(input, []byte(`"environment":{"x":"y"}`), []byte(`"environment":{"x":"y","x":"z"}`), 1),
		"unknown extension duplicate": bytes.Replace(input, []byte(`"schema":1`), []byte(`"extension":{"a":1,"a":2},"schema":1`), 1),
		"comment":                     append([]byte("/* comment */"), input...),
		"invalid UTF8":                bytes.Replace(input, []byte("fixture"), []byte{0xff}, 1),
		"null root":                   []byte("null"),
		"array root":                  []byte("[]"),
		"NaN":                         bytes.Replace(input, []byte(`"duration_ms":1000`), []byte(`"duration_ms":NaN`), 1),
		"infinite duration":           bytes.Replace(input, []byte(`"duration_ms":1000`), []byte(`"duration_ms":1e309`), 1),
	} {
		t.Run(name, func(t *testing.T) {
			r := evaluate(raw, config{cold, 0.95, 10, 20})
			if r.Outcome != invalidEvidence || len(r.Issues) == 0 || r.Statistics != nil {
				t.Fatalf("accepted malformed document: %s %+v", r.Outcome, r.Issues)
			}
			if !bytes.Equal(r.Input.Bytes, raw) {
				t.Fatal("raw malformed evidence lost")
			}
		})
	}
}

func TestMalformedRecordsAndPairStructure(t *testing.T) {
	for _, tc := range []struct {
		name   string
		change func([]record) []record
	}{
		{"negative duration", func(r []record) []record { r[0].DurationMS = -1; return r }},
		{"zero duration", func(r []record) []record { r[0].DurationMS = 0; return r }},
		{"phase", func(r []record) []record { r[0].Phase = "watch"; return r }},
		{"order", func(r []record) []record { r[0].Order = "AA"; return r }},
		{"arm", func(r []record) []record { r[0].Binary = "unknown"; return r }},
		{"duplicate pair", func(r []record) []record { r[4].Pair = 1; r[5].Pair = 1; return r }},
		{"incomplete pair", func(r []record) []record { return r[1:] }},
		{"incomplete block", func(r []record) []record { return r[2:] }},
		{"unbalanced", func(r []record) []record { r[2].Order, r[3].Order = "AB", "AB"; r[2], r[3] = r[3], r[2]; return r }},
		{"arms reversed", func(r []record) []record { r[0], r[1] = r[1], r[0]; return r }},
		{"phase not contiguous", func(r []record) []record {
			first := append([]record{}, r[:4]...)
			copy(r[:4], r[20:24])
			copy(r[20:24], first)
			return r
		}},
		{"missing digest", func(r []record) []record { r[0].OutputTreeDigest = ""; return r }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var root map[string]json.RawMessage
			if err := json.Unmarshal(fixture(t, 900, 1000, 10, 20), &root); err != nil {
				t.Fatal(err)
			}
			var records []record
			if err := json.Unmarshal(root["records"], &records); err != nil {
				t.Fatal(err)
			}
			root["records"], _ = json.Marshal(tc.change(records))
			data, _ := json.Marshal(root)
			r := evaluate(data, config{cold, 0.95, 10, 20})
			if r.Outcome != invalidEvidence || r.Statistics != nil {
				t.Fatalf("accepted %s: %s %+v", tc.name, r.Outcome, r.Issues)
			}
		})
	}
}

func TestAllMalformedRecordsAndMissingExitAreReported(t *testing.T) {
	input := fixture(t, 900, 1000, 10, 20)
	input = bytes.ReplaceAll(input, []byte(`"duration_ms":1000`), []byte(`"duration_ms":0`))
	input = bytes.Replace(input, []byte(`"exit_code":0,`), nil, 1)
	_, issues := validate(input, config{cold, 0.95, 10, 20})
	badDurations, missingExit := 0, false
	for _, problem := range issues {
		if strings.HasSuffix(problem.Path, ".duration_ms") {
			badDurations++
		}
		if strings.HasSuffix(problem.Path, ".exit_code") && strings.Contains(problem.Message, "missing") {
			missingExit = true
		}
	}
	if badDurations != 50 || !missingExit {
		t.Fatalf("want all 50 bad durations and missing exit, got %d/%t", badDurations, missingExit)
	}
}

func TestDefaultBatchCountsAndAvailableIdentities(t *testing.T) {
	_, issues := validate(fixture(t, 900, 1000, 10, 20), config{cold, 0.95, 30, 100})
	if len(issues) != 2 {
		t.Fatalf("default counts must reject 10/20: %+v", issues)
	}
	_, issues = validate(fixture(t, 900, 1000, 30, 100), config{cold, 0.95, 30, 100})
	if len(issues) != 0 {
		t.Fatalf("default batch rejected: %+v", issues)
	}
	input := fixture(t, 900, 1000, 10, 20)
	input = bytes.Replace(input, []byte(`"schema":1`), []byte(`"schema":1,"tree_digest_format":"sha256-tree-v2","measurement_boundary":"owned-job-quiescence-and-spooled-stream-eof-v2"`), 1)
	input = bytes.ReplaceAll(input, []byte(`"output_tree_digest":"output"`), []byte(`"output_tree_digest":"sha256-tree-v2:`+strings.Repeat("a", 64)+`"`))
	_, issues = validate(input, config{cold, 0.95, 10, 20})
	if len(issues) != 0 {
		t.Fatalf("R2 schema1 extensions rejected: %+v", issues)
	}
	input = bytes.Replace(input, []byte(strings.Repeat("a", 64)), []byte("bad-hash"), 1)
	_, issues = validate(input, config{cold, 0.95, 10, 20})
	if len(issues) == 0 {
		t.Fatal("malformed supplied identity ignored")
	}
}
