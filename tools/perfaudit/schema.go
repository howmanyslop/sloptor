package main

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"maps"
	"math"
	"slices"
	"strings"
	"unicode/utf8"
)

const (
	cold     = "cold"
	noChange = "no_change"
)

type issue struct {
	Kind    string `json:"kind"`
	Path    string `json:"path"`
	Message string `json:"message"`
}

type record struct {
	Pair              int    `json:"pair"`
	Phase             string `json:"phase"`
	Order             string `json:"order"`
	Binary            string `json:"binary"`
	DurationMS        int64  `json:"duration_ms"`
	ExitCode          int    `json:"exit_code"`
	DiagnosticsDigest string `json:"diagnostics_digest"`
	OutputTreeDigest  string `json:"output_tree_digest"`
}

type pair struct {
	baseline, candidate float64
	order               string
}

type validation struct {
	issues []issue
}

func (v *validation) add(kind, path, message string) {
	v.issues = append(v.issues, issue{kind, path, message})
}

func (v *validation) invalid(path, message string) {
	v.add(invalidEvidence, path, message)
}

// Walk the entire document before decoding fields: encoding/json alone accepts
// duplicate keys, case-insensitive fields and replacement of invalid Unicode.
func (v *validation) strictJSON(input []byte) bool {
	if !utf8.Valid(input) {
		v.invalid("$", "JSON must be UTF-8")
		return false
	}
	if !v.unicodeEscapes(input) {
		return false
	}
	d := json.NewDecoder(bytes.NewReader(input))
	d.UseNumber()
	if err := v.walkJSON(d, "$", 0); err != nil {
		v.invalid("$", err.Error())
		return false
	}
	if _, err := d.Token(); err != io.EOF {
		v.invalid("$", "trailing content after JSON document")
		return false
	}
	return true
}

func (v *validation) walkJSON(d *json.Decoder, path string, depth int) error {
	if depth > 128 {
		return fmt.Errorf("JSON nesting exceeds 128")
	}
	token, err := d.Token()
	if err != nil {
		return err
	}
	switch token {
	case json.Delim('{'):
		seen := map[string]bool{}
		for d.More() {
			key, err := d.Token()
			if err != nil {
				return err
			}
			name, ok := key.(string)
			if !ok {
				return fmt.Errorf("object key is not a string")
			}
			child := path + "[" + fmt.Sprintf("%q", name) + "]"
			if seen[name] {
				v.invalid(child, "duplicate JSON field")
			}
			seen[name] = true
			if err := v.walkJSON(d, child, depth+1); err != nil {
				return err
			}
		}
		_, err = d.Token()
	case json.Delim('['):
		for i := 0; d.More(); i++ {
			if err := v.walkJSON(d, fmt.Sprintf("%s[%d]", path, i), depth+1); err != nil {
				return err
			}
		}
		_, err = d.Token()
	}
	return err
}

type object map[string]json.RawMessage

func (v *validation) object(raw json.RawMessage, path string, required, optional []string) object {
	var obj object
	trimmed := bytes.TrimSpace(raw)
	if json.Unmarshal(raw, &obj) != nil || obj == nil || trimmed == nil || len(trimmed) == 0 || trimmed[0] != '{' {
		v.invalid(path, "must be an object")
		return object{}
	}
	for _, key := range required {
		if _, ok := obj[key]; !ok {
			v.invalid(path+"."+key, "required field is missing")
		}
	}
	// Extensions remain in raw input, but case shadows of recognized fields are
	// ambiguous to other schema1 consumers and are rejected.
	for _, key := range slices.Sorted(maps.Keys(obj)) {
		for _, known := range append(append([]string{}, required...), optional...) {
			if key != known && strings.EqualFold(key, known) {
				v.invalid(path+"."+key, "case shadow of "+known)
			}
		}
	}
	return obj
}

func (v *validation) field(obj object, key, path string, target any) bool {
	raw, ok := obj[key]
	if !ok {
		return false
	}
	if bytes.Equal(bytes.TrimSpace(raw), []byte("null")) || json.Unmarshal(raw, target) != nil {
		v.invalid(path+"."+key, "invalid field type or numeric range")
		return false
	}
	return true
}

func (v *validation) text(obj object, key, path string) string {
	var value string
	if v.field(obj, key, path, &value) && strings.TrimSpace(value) == "" {
		v.invalid(path+"."+key, "must be nonempty")
	}
	return value
}

func validate(input []byte, cfg config) (map[string][]pair, []issue) {
	v := validation{issues: []issue{}}
	phases := map[string][]pair{cold: {}, noChange: {}}
	if !v.strictJSON(input) {
		return phases, v.issues
	}
	root := v.object(input, "$", []string{"schema", "machine", "baseline", "candidate", "records"}, []string{"tree_digest_format", "measurement_boundary"})
	var schema int
	if v.field(root, "schema", "$", &schema) && schema != 1 {
		v.invalid("$.schema", "must be 1")
	}
	v.machine(root["machine"])
	for _, arm := range []string{"baseline", "candidate"} {
		obj := v.object(root[arm], "$."+arm, []string{"revision", "command"}, nil)
		revision := v.text(obj, "revision", "$."+arm)
		v.text(obj, "command", "$."+arm)
		if strings.HasPrefix(strings.ToLower(revision), "sha256:") && !isSHA256(revision[len("sha256:"):]) {
			v.invalid("$."+arm+".revision", "sha256 identity must contain 64 hexadecimal digits")
		}
	}
	for _, key := range []string{"tree_digest_format", "measurement_boundary"} {
		if _, ok := root[key]; ok {
			v.text(root, key, "$")
		}
	}
	var rawRecords []json.RawMessage
	if !v.field(root, "records", "$", &rawRecords) {
		return phases, v.issues
	}
	if len(rawRecords)%2 != 0 {
		v.invalid("$.records", "incomplete pair: odd record count")
	}
	records := make([]record, len(rawRecords))
	valid := make([]bool, len(rawRecords))
	for i, raw := range rawRecords {
		path := fmt.Sprintf("$.records[%d]", i)
		before := len(v.issues)
		obj := v.object(raw, path, []string{"pair", "phase", "order", "binary", "duration_ms", "exit_code", "diagnostics_digest", "output_tree_digest"}, nil)
		r := &records[i]
		v.field(obj, "pair", path, &r.Pair)
		r.Phase = v.text(obj, "phase", path)
		r.Order = v.text(obj, "order", path)
		r.Binary = v.text(obj, "binary", path)
		v.field(obj, "duration_ms", path, &r.DurationMS)
		v.field(obj, "exit_code", path, &r.ExitCode)
		r.DiagnosticsDigest = v.text(obj, "diagnostics_digest", path)
		r.OutputTreeDigest = v.text(obj, "output_tree_digest", path)
		if r.Pair <= 0 {
			v.invalid(path+".pair", "must be positive")
		}
		if r.Phase != cold && r.Phase != noChange {
			v.invalid(path+".phase", "must be cold or no_change")
		}
		if r.Order != "AB" && r.Order != "BA" {
			v.invalid(path+".order", "must be AB or BA")
		}
		if r.Binary != "baseline" && r.Binary != "candidate" {
			v.invalid(path+".binary", "must be baseline or candidate")
		}
		// Matches legacy's integer-ms range and keeps all median sums exact in
		// int64. Statistical calculations use float64 and report that limitation.
		if r.DurationMS <= 0 || r.DurationMS > math.MaxInt64/2 {
			v.invalid(path+".duration_ms", "must be a positive integer <= MaxInt64/2")
		}
		v.digestIdentity(root, r.OutputTreeDigest, path+".output_tree_digest")
		valid[i] = len(v.issues) == before
		// A bad duration or missing digest must not hide a failed command.
		if _, ok := obj["exit_code"]; ok && r.ExitCode != 0 {
			v.add(correctnessFailure, path+".exit_code", fmt.Sprintf("successful workload requires exit 0, got %d", r.ExitCode))
		}
	}
	seen := map[string]bool{}
	phaseEnded := map[string]bool{}
	lastPhase := ""
	for i := 0; i+1 < len(records); i += 2 {
		a, b := records[i], records[i+1]
		path := fmt.Sprintf("$.records[%d:%d]", i, i+2)
		if a.Pair != b.Pair || a.Phase != b.Phase || a.Order != b.Order {
			v.invalid(path, "adjacent records must share pair, phase and order")
		}
		key := fmt.Sprintf("%s/%d", a.Phase, a.Pair)
		if seen[key] {
			v.invalid(path, "duplicate pair "+key)
		}
		seen[key] = true
		if a.Order == "AB" && (a.Binary != "baseline" || b.Binary != "candidate") || a.Order == "BA" && (a.Binary != "candidate" || b.Binary != "baseline") {
			v.invalid(path, "arms disagree with declared command order")
		}
		if a.Phase != lastPhase {
			phaseEnded[lastPhase] = true
			if phaseEnded[a.Phase] {
				v.invalid(path, "phase must be contiguous to define adjacent blocks")
			}
			lastPhase = a.Phase
		}
		if a.Pair != b.Pair || a.Phase != b.Phase {
			continue
		}
		if a.DiagnosticsDigest != b.DiagnosticsDigest {
			v.add(correctnessFailure, path, "diagnostics digests differ")
		}
		if a.OutputTreeDigest != b.OutputTreeDigest {
			v.add(correctnessFailure, path, "output-tree digests differ")
		}
		if !valid[i] || !valid[i+1] || a.Phase != cold && a.Phase != noChange {
			continue
		}
		baseline, candidate := a, b
		if a.Binary == "candidate" {
			baseline, candidate = b, a
		}
		phases[a.Phase] = append(phases[a.Phase], pair{float64(baseline.DurationMS), float64(candidate.DurationMS), a.Order})
	}
	for _, phase := range []string{cold, noChange} {
		pairs := phases[phase]
		want := cfg.ColdPairs
		if phase == noChange {
			want = cfg.NoChangePairs
		}
		if len(pairs) != want {
			v.invalid("$.records."+phase, fmt.Sprintf("complete valid pairs = %d; declared batch requires exactly %d", len(pairs), want))
		}
		if len(pairs)%2 != 0 {
			v.invalid("$.records."+phase, "incomplete four-command block")
		}
		for i := 1; i < len(pairs); i++ {
			if pairs[i].order == pairs[i-1].order {
				v.invalid("$.records."+phase, fmt.Sprintf("pairs at phase positions %d and %d must alternate AB/BA", i, i+1))
			}
		}
	}
	return phases, v.issues
}

func (v *validation) machine(raw json.RawMessage) {
	path := "$.machine"
	obj := v.object(raw, path, []string{"os", "version", "cpu", "ram_bytes", "storage", "power", "run_order", "sidecar_timeout", "environment"}, nil)
	for _, key := range []string{"os", "version", "cpu", "storage", "power", "sidecar_timeout"} {
		v.text(obj, key, path)
	}
	var ram int64
	if v.field(obj, "ram_bytes", path, &ram) && ram <= 0 {
		v.invalid(path+".ram_bytes", "must be positive")
	}
	var orders []string
	if v.field(obj, "run_order", path, &orders) {
		validOrder := len(orders) == 2 && (orders[0] == "AB" && orders[1] == "BA" || orders[0] == "BA" && orders[1] == "AB")
		if !validOrder {
			v.invalid(path+".run_order", "must contain exactly AB and BA")
		}
	}
	env := v.object(obj["environment"], path+".environment", nil, nil)
	if len(env) == 0 {
		v.invalid(path+".environment", "must not be empty")
	}
	for _, key := range slices.Sorted(maps.Keys(env)) {
		var value string
		v.field(env, key, path+".environment", &value)
	}
}

func (v *validation) digestIdentity(root object, digest, path string) {
	var format string
	if raw, ok := root["tree_digest_format"]; ok {
		_ = json.Unmarshal(raw, &format) // Its type is validated separately.
	}
	if format != "" && !strings.HasPrefix(digest, format+":") {
		v.invalid(path, "digest disagrees with declared tree_digest_format")
	}
	if strings.HasPrefix(digest, "sha256-tree-v2:") && !isSHA256(strings.TrimPrefix(digest, "sha256-tree-v2:")) {
		v.invalid(path, "sha256-tree-v2 identity must contain 64 hexadecimal digits")
	}
}

func isSHA256(value string) bool {
	_, err := hex.DecodeString(value)
	return len(value) == 64 && err == nil
}
