package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"maps"
	"math"
	"path"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"time"
)

const resourceFormat = "windows-job-post-wall-v1"
const resourceBoundary = "owned-job-quiescence-and-spooled-stream-eof-v2"
const incompleteResource = "incomplete-unmeasured"

var resourceMetricNames = []string{"job_cpu_ms", "job_peak_bytes", "read_operations", "write_operations", "other_operations", "read_bytes", "write_bytes", "other_bytes"}

// Raw inputs and every journal event survive admission, including rejected and
// diagnostic events. Integers below are exact. Populations also have float64
// displays, while additive mean decisions consume the exact counters.
type resourceObservation struct {
	Line                int               `json:"line"`
	Raw                 []byte            `json:"raw_line_base64"`
	Decoded             json.RawMessage   `json:"decoded,omitempty"`
	Classification      string            `json:"classification"`
	Event               string            `json:"event"`
	Scored              bool              `json:"scored"`
	Pair                int               `json:"pair,omitempty"`
	Phase               string            `json:"phase"`
	Order               string            `json:"order,omitempty"`
	Binary              string            `json:"binary"`
	JobUserTicks100ns   *uint64           `json:"job_user_ticks_100ns,omitempty"`
	JobKernelTicks100ns *uint64           `json:"job_kernel_ticks_100ns,omitempty"`
	JobTotalTicks100ns  *uint64           `json:"job_total_ticks_100ns,omitempty"`
	CounterValues       map[string]uint64 `json:"exact_metric_values,omitempty"`
	ComponentStatuses   map[string]string `json:"component_statuses,omitempty"`
	ComponentCauses     map[string]string `json:"component_causes,omitempty"`
	metrics             map[string]float64
	entry               object
}

type resourceBatch struct {
	RawManifest    rawInput                     `json:"manifest"`
	RawJournal     rawInput                     `json:"resource_journal"`
	RawRun         rawInput                     `json:"run_metadata"`
	Issues         []issue                      `json:"issues"`
	Phases         map[string][]pair            `json:"-"`
	Resources      map[string]map[string][]pair `json:"-"`
	Observations   []resourceObservation        `json:"observations"`
	ResourceStatus string                       `json:"resource_status"`
	LegacyVerdict  string                       `json:"legacy_verdict"`
	run            object
	manifest       object
}

func resourceRaw(input []byte) rawInput {
	return rawInput{fmt.Sprintf("%x", sha256.Sum256(input)), bytes.Clone(input)}
}

func admitResourceBatch(manifest, journal, run []byte, cfg config) resourceBatch {
	b := resourceBatch{RawManifest: resourceRaw(manifest), RawJournal: resourceRaw(journal), RawRun: resourceRaw(run),
		Resources: map[string]map[string][]pair{}, Observations: []resourceObservation{}, ResourceStatus: "complete", LegacyVerdict: "unverified"}
	b.Phases, b.Issues = validate(manifest, cfg)
	v := validation{}
	if resourceDocument(&v, manifest, "$.manifest") {
		b.manifest = v.object(manifest, "$.manifest", []string{"schema", "machine", "baseline", "candidate", "records", "tree_digest_format", "measurement_boundary"}, nil)
		resourceExpectText(&v, b.manifest, "tree_digest_format", "$.manifest", "sha256-tree-v2")
		resourceExpectText(&v, b.manifest, "measurement_boundary", "$.manifest", resourceBoundary)
	}
	if resourceDocument(&v, run, "$.run") {
		b.run = v.object(run, "$.run", []string{"schema", "status", "started_utc", "finished_utc", "error", "before", "after", "evaluation", "invocation_journal", "tree_digest_format", "diagnostics_digest_format", "project_mode", "project_state", "resource_accounting", "measurement_boundary", "capture_limit_bytes_per_stream", "machine"}, []string{"cleanup_error", "scope", "tools"})
		b.validateRun(&v)
	}
	var records []record
	if b.manifest != nil {
		_ = json.Unmarshal(b.manifest["records"], &records) // Original validator reports all malformed fields.
	}
	lines := bytes.Split(journal, []byte{'\n'})
	if len(lines) > 1 && len(lines[len(lines)-1]) == 0 {
		lines = lines[:len(lines)-1]
	}
	var previousTime time.Time
	for i, raw := range lines {
		p := fmt.Sprintf("$.journal[%d]", i)
		o := resourceObservation{Line: i + 1, Raw: bytes.Clone(raw), Classification: "unclassified"}
		if resourceDocument(&v, raw, p) {
			o.Decoded = bytes.Clone(raw)
			o.entry = v.object(raw, p, []string{"event", "invocation", "at_utc", "status", "executable", "working_directory", "stdout", "stderr", "diagnostics_digest_format", "resource_accounting_format"}, []string{"error", "validation_completed", "result"})
			o.Event = v.text(o.entry, "event", p)
			resourceExpectText(&v, o.entry, "diagnostics_digest_format", p, "sha256-diagnostics-v2")
			resourceExpectText(&v, o.entry, "resource_accounting_format", p, resourceFormat)
			stamp := resourceTime(&v, o.entry, "at_utc", p)
			if !stamp.IsZero() && !previousTime.IsZero() && stamp.Before(previousTime) {
				v.invalid(p+".at_utc", "journal timestamps must be chronological")
			}
			if !stamp.IsZero() {
				previousTime = stamp
				var startText, finishText string
				_ = json.Unmarshal(b.run["started_utc"], &startText)
				_ = json.Unmarshal(b.run["finished_utc"], &finishText)
				start, se := time.Parse(time.RFC3339Nano, startText)
				finish, fe := time.Parse(time.RFC3339Nano, finishText)
				if se == nil && fe == nil && (stamp.Before(start) || stamp.After(finish)) {
					v.invalid(p+".at_utc", "invocation event is outside recorded run start/finish")
				}
			}
			inv := v.object(o.entry["invocation"], p+".invocation", []string{"phase", "binary", "scored"}, []string{"pair", "order"})
			v.field(inv, "scored", p+".invocation", &o.Scored)
			o.Phase, o.Binary = v.text(inv, "phase", p+".invocation"), v.text(inv, "binary", p+".invocation")
			if o.Scored {
				o.Classification = "scored"
				inv = v.object(o.entry["invocation"], p+".invocation", []string{"phase", "binary", "scored", "pair", "order"}, nil)
				v.field(inv, "pair", p+".invocation", &o.Pair)
				o.Order = v.text(inv, "order", p+".invocation")
				if o.Pair < 1 || o.Phase != cold && o.Phase != noChange || o.Order != "AB" && o.Order != "BA" || o.Binary != "baseline" && o.Binary != "candidate" {
					v.invalid(p+".invocation", "invalid scored pair/phase/order/arm")
				}
			} else {
				o.Classification = "post-sample-timings-diagnostic"
				_, hasPair := inv["pair"]
				_, hasOrder := inv["order"]
				if o.Phase != "diagnostic" || o.Binary != "candidate" || hasPair || hasOrder {
					v.invalid(p+".invocation", "unscored invocation must be the separate candidate timing diagnostic without pair/order")
				}
			}
			switch o.Event {
			case "started":
				resourceExpectText(&v, o.entry, "status", p, "incomplete")
				for _, key := range []string{"result", "validation_completed", "error"} {
					if _, ok := o.entry[key]; ok {
						v.invalid(p+"."+key, "started event cannot contain a finished-only field")
					}
				}
			case "finished":
				resourceExpectText(&v, o.entry, "status", p, "completed")
				v.object(raw, p, []string{"error", "validation_completed", "result"}, nil)
				resourceNull(&v, o.entry, "error", p)
				var completed bool
				if v.field(o.entry, "validation_completed", p, &completed) && !completed {
					v.invalid(p+".validation_completed", "invocation validation must have completed")
				}
				b.validateResult(&v, &o, p)
			default:
				v.invalid(p+".event", "must be started or finished")
			}
			b.validateInvocationPaths(&v, &o, p)
		}
		b.Observations = append(b.Observations, o)
	}
	if len(b.Observations) != 2*(len(records)+1) {
		v.invalid("$.journal", fmt.Sprintf("must contain exactly one started/finished pair for each of %d manifest records and one trailing timing diagnostic; got %d events", len(records), len(b.Observations)))
	}
	var scored []resourceObservation
	for i := 0; i < len(b.Observations); i++ {
		o := b.Observations[i]
		if i%2 == 0 {
			if o.Event != "started" {
				v.invalid(fmt.Sprintf("$.journal[%d]", i), "expected started event")
			}
			continue
		}
		a := b.Observations[i-1]
		p := fmt.Sprintf("$.journal[%d]", i)
		if o.Event != "finished" {
			v.invalid(p, "expected finished event")
		}
		for _, key := range []string{"invocation", "executable", "working_directory", "stdout", "stderr", "diagnostics_digest_format", "resource_accounting_format"} {
			if !resourceJSONEqual(a.entry[key], o.entry[key]) {
				v.invalid(p+"."+key, "started/finished metadata mismatch")
			}
		}
		sample := i / 2
		if sample < len(records) {
			if !a.Scored || !o.Scored {
				v.invalid(p, "scored manifest records must precede the single diagnostic")
			}
			r := records[sample]
			phase, phaseStart := cold, 0
			if sample >= 2*cfg.ColdPairs {
				phase, phaseStart = noChange, 2*cfg.ColdPairs
			}
			number := (sample-phaseStart)/2 + 1
			order := "AB"
			if number%2 == 0 {
				order = "BA"
			}
			if r.Phase != phase || r.Pair != number || r.Order != order {
				v.invalid(p+".invocation", "final harness protocol requires cold then no_change, consecutive pair numbers, starting AB and alternating BA")
			}
			if o.Pair != r.Pair || o.Phase != r.Phase || o.Order != r.Order || o.Binary != r.Binary {
				v.invalid(p+".invocation", fmt.Sprintf("does not correspond in order to manifest record %d", sample))
			}
			result := o.entry["result"]
			var retained struct {
				DurationMS                          int64
				ExitCode                            int
				DiagnosticsDigest, OutputTreeDigest string
			}
			_ = json.Unmarshal(result, &retained)
			if retained.DurationMS != r.DurationMS || retained.ExitCode != r.ExitCode || retained.DiagnosticsDigest != r.DiagnosticsDigest || retained.OutputTreeDigest != r.OutputTreeDigest {
				v.invalid(p+".result", "duration, exit or digest differs from corresponding manifest record")
			}
			scored = append(scored, o)
		} else if sample != len(records) || a.Scored || o.Scored || a.Classification != "post-sample-timings-diagnostic" || o.Classification != "post-sample-timings-diagnostic" {
			v.invalid(p, "exactly one post-sample timing diagnostic is permitted")
		}
	}
	b.Issues = append(b.Issues, v.issues...)
	for _, problem := range b.Issues {
		if problem.Kind == invalidEvidence {
			return b
		}
	}
	// A missing required observation suppresses the whole metric, never a row.
	for _, metric := range resourceMetricNames {
		complete := len(scored) == len(records)
		for _, o := range scored {
			if _, ok := o.metrics[metric]; !ok {
				complete = false
			}
		}
		if !complete {
			b.ResourceStatus = incompleteResource
			continue
		}
		phases := map[string][]pair{cold: {}, noChange: {}}
		for i := 0; i+1 < len(scored); i += 2 {
			a, c := scored[i], scored[i+1]
			if a.Binary == "candidate" {
				a, c = c, a
			}
			phases[a.Phase] = append(phases[a.Phase], pair{a.metrics[metric], c.metrics[metric], a.Order})
		}
		b.Resources[metric] = phases
	}
	return b
}

func resourceDocument(v *validation, input []byte, p string) bool {
	local := validation{}
	ok := local.strictJSON(input)
	for _, problem := range local.issues {
		problem.Path = p + strings.TrimPrefix(problem.Path, "$")
		v.issues = append(v.issues, problem)
	}
	return ok
}

func resourceExpectText(v *validation, obj object, key, p, want string) {
	if got := v.text(obj, key, p); got != want {
		v.invalid(p+"."+key, fmt.Sprintf("must be %q", want))
	}
}

func resourceNull(v *validation, obj object, key, p string) {
	if !bytes.Equal(bytes.TrimSpace(obj[key]), []byte("null")) {
		v.invalid(p+"."+key, "must be null for a completed available observation")
	}
}

func resourceTime(v *validation, obj object, key, p string) time.Time {
	t, err := time.Parse(time.RFC3339Nano, v.text(obj, key, p))
	if err != nil {
		v.invalid(p+"."+key, "must be an RFC3339 timestamp")
	}
	return t
}

func resourceJSONEqual(a, b []byte) bool {
	decode := func(input []byte) (any, error) {
		d := json.NewDecoder(bytes.NewReader(input))
		d.UseNumber()
		var value any
		err := d.Decode(&value)
		return value, err
	}
	x, xe := decode(a)
	y, ye := decode(b)
	return xe == nil && ye == nil && reflect.DeepEqual(x, y)
}

func (b *resourceBatch) validateRun(v *validation) {
	r := b.run
	var schema int
	if v.field(r, "schema", "$.run", &schema) && schema != 1 {
		v.invalid("$.run.schema", "must be 1")
	}
	resourceExpectText(v, r, "status", "$.run", "evaluated")
	started, finished := resourceTime(v, r, "started_utc", "$.run"), resourceTime(v, r, "finished_utc", "$.run")
	if !started.IsZero() && !finished.IsZero() && finished.Before(started) {
		v.invalid("$.run.finished_utc", "precedes run start")
	}
	resourceExpectText(v, r, "tree_digest_format", "$.run", "sha256-tree-v2")
	resourceExpectText(v, r, "diagnostics_digest_format", "$.run", "sha256-diagnostics-v2")
	resourceExpectText(v, r, "measurement_boundary", "$.run", resourceBoundary)
	if _, ok := r["cleanup_error"]; ok {
		resourceNull(v, r, "cleanup_error", "$.run")
	}
	if _, ok := r["scope"]; ok {
		v.text(r, "scope", "$.run")
	}
	if raw, ok := r["tools"]; ok {
		resourceStringMap(v, raw, "$.run.tools", false)
		v.object(raw, "$.run.tools", nil, []string{"powershell", "go", "git", "node"})
	}
	v.text(r, "invocation_journal", "$.run")
	accounting := v.object(r["resource_accounting"], "$.run.resource_accounting", []string{"format", "invocation_journal", "boundary", "cpu_units", "memory", "io"}, nil)
	resourceExpectText(v, accounting, "format", "$.run.resource_accounting", resourceFormat)
	// The final native descriptors and the original evaluator's unambiguous
	// shorthand are supported. Arbitrary prose cannot redefine this format.
	for _, descriptor := range []struct {
		key    string
		values []string
	}{
		{"boundary", []string{"getters after wall endpoint and owned cleanup, before retained handle disposal", "post-wall"}},
		{"cpu_units", []string{"100ns ticks; observed resolution may be coarse", "100ns ticks"}},
		{"memory", []string{"Windows job/process peak memory bytes, not RSS", "job peak bytes, not RSS", "job committed peak bytes"}},
		{"io", []string{"OS-accounted read/write/other operations and bytes; not filesystem-only or durable writes", "OS I/O, not filesystem-only", "OS counters, not filesystem-only"}},
	} {
		if !slices.Contains(descriptor.values, v.text(accounting, descriptor.key, "$.run.resource_accounting")) {
			v.invalid("$.run.resource_accounting."+descriptor.key, "unsupported descriptor for "+resourceFormat)
		}
	}
	if !resourceJSONEqual(r["invocation_journal"], accounting["invocation_journal"]) {
		v.invalid("$.run.resource_accounting.invocation_journal", "differs from run invocation_journal")
	}
	if !resourceJSONEqual(r["before"], r["after"]) {
		v.invalid("$.run.after", "input provenance changed from before snapshot")
	}
	before := v.object(r["before"], "$.run.before", []string{"baseline", "candidate", "fixture", "repositories"}, nil)
	v.object(r["after"], "$.run.after", []string{"baseline", "candidate", "fixture", "repositories"}, nil)
	for _, arm := range []string{"baseline", "candidate"} {
		p := "$.run.before." + arm
		identity := v.object(before[arm], p, []string{"path", "sha256"}, []string{"provided_source_sha", "observed_build", "build_setting_hashes", "build_info_sha256"})
		executable, hash := v.text(identity, "path", p), v.text(identity, "sha256", p)
		if !resourceAbsolute(executable) {
			v.invalid(p+".path", "must be an absolute executable path")
		}
		if !isSHA256(hash) {
			v.invalid(p+".sha256", "must contain 64 hexadecimal digits")
		}
		resourceBinaryMetadata(v, identity, p)
		manifestArm := v.object(b.manifest[arm], "$.manifest."+arm, []string{"revision", "command"}, nil)
		if !strings.EqualFold(v.text(manifestArm, "revision", "$.manifest."+arm), "sha256:"+hash) {
			v.invalid("$.manifest."+arm+".revision", "does not match retained binary SHA256")
		}
		project := "<work-copy>"
		var mode string
		_ = json.Unmarshal(r["project_mode"], &mode)
		if mode == "stable-project-path-v1" {
			state := v.object(r["project_state"], "$.run.project_state", []string{"active_path"}, nil)
			project = v.text(state, "active_path", "$.run.project_state")
		}
		if command := v.text(manifestArm, "command", "$.manifest."+arm); command != executable+" build --project "+project+" --json" {
			v.invalid("$.manifest."+arm+".command", "does not match retained executable, project mode and harness command")
		}
	}
	fixture := v.object(before["fixture"], "$.run.before.fixture", []string{"path", "sha256"}, []string{"provided_identity", "provenance"})
	if !resourceAbsolute(v.text(fixture, "path", "$.run.before.fixture")) {
		v.invalid("$.run.before.fixture.path", "must be an absolute fixture path")
	}
	hash := v.text(fixture, "sha256", "$.run.before.fixture")
	if !strings.HasPrefix(hash, "sha256-tree-v2:") || !isSHA256(strings.TrimPrefix(hash, "sha256-tree-v2:")) {
		v.invalid("$.run.before.fixture.sha256", "must be a sha256-tree-v2 identity")
	}
	resourceFixtureMetadata(v, fixture, "$.run.before.fixture")
	resourceRepositories(v, before, "$.run.before")
	if !resourceJSONEqual(r["machine"], b.manifest["machine"]) {
		v.invalid("$.manifest.machine", "differs from retained run machine/environment metadata")
	}
	eval := v.object(r["evaluation"], "$.run.evaluation", []string{"ExitCode", "Status", "Output"}, nil)
	var exit int
	v.field(eval, "ExitCode", "$.run.evaluation", &exit)
	status := v.text(eval, "Status", "$.run.evaluation")
	var output string
	v.field(eval, "Output", "$.run.evaluation", &output)
	if status != "PASS" && status != "FAIL" || status == "PASS" && exit != 0 || status == "FAIL" && exit != 1 {
		v.invalid("$.run.evaluation", "legacy PASS/FAIL and exit code must agree (0/1)")
	} else {
		b.LegacyVerdict = status
	}
	switch status {
	case "PASS":
		resourceNull(v, r, "error", "$.run")
	case "FAIL":
		want := "perfcompare rejected benchmark evidence: " + output
		if got := v.text(r, "error", "$.run"); got != want {
			v.invalid("$.run.error", "evaluated legacy failure must retain its exact evaluator cause")
		}
	}
	resourceUint(v, r, "capture_limit_bytes_per_stream", "$.run", math.MaxInt64)
	mode := v.text(r, "project_mode", "$.run")
	switch mode {
	case "separate-work-copies":
		resourceNull(v, r, "project_state", "$.run")
	case "stable-project-path-v1":
		state := v.object(r["project_state"], "$.run.project_state", []string{"mode", "evidence_root", "work_root", "active_path", "arms", "active_owner", "pending_arm", "transition", "preserve_work", "cleanup_completed", "locations", "error"}, []string{"state_file", "journal", "canonical_root", "input_metadata_file", "input_metadata_sha256"})
		resourceExpectText(v, state, "mode", "$.run.project_state", mode)
		resourceExpectText(v, state, "transition", "$.run.project_state", "cleaned")
		for _, key := range []string{"active_owner", "pending_arm", "error"} {
			resourceNull(v, state, key, "$.run.project_state")
		}
		for key, want := range map[string]bool{"cleanup_completed": true, "preserve_work": false} {
			var got bool
			if v.field(state, key, "$.run.project_state", &got) && got != want {
				v.invalid("$.run.project_state."+key, "stable project cleanup did not complete")
			}
		}
		resourceStableMetadata(v, state, fixture, v.text(r, "invocation_journal", "$.run"))
	default:
		v.invalid("$.run.project_mode", "unsupported project mode")
	}
}

// These are shape and internal correspondence checks on recorded metadata.
// They do not establish that source assertions, host state or package claims
// were independently verified. Empty optional assertions remain empty strings.
func resourceGitSHA(value string) bool {
	_, err := hex.DecodeString(value)
	return len(value) == 40 && err == nil
}

func resourceStringMap(v *validation, raw json.RawMessage, p string, hashes bool) object {
	obj := v.object(raw, p, nil, nil)
	for _, key := range slices.Sorted(maps.Keys(obj)) {
		var value string
		if v.field(obj, key, p, &value) && hashes && !isSHA256(value) {
			v.invalid(p+"."+key, "must contain 64 hexadecimal digits")
		}
	}
	return obj
}

func resourceBinaryMetadata(v *validation, identity object, p string) {
	var provided string
	if _, ok := identity["provided_source_sha"]; ok {
		if v.field(identity, "provided_source_sha", p, &provided) && provided != "" && !resourceGitSHA(provided) {
			v.invalid(p+".provided_source_sha", "must be empty or a full 40-character hexadecimal Git SHA")
		}
	}
	if raw, ok := identity["observed_build"]; ok {
		observed := resourceStringMap(v, raw, p+".observed_build", false)
		// Recognize fixed Go build fields so case shadows are rejected, while
		// retaining later string-valued build fields as dictionary extensions.
		v.object(raw, p+".observed_build", nil, []string{"go_toolchain", "vcs.revision", "vcs.time", "vcs.modified", "GOOS", "GOARCH", "CGO_ENABLED", "-compiler", "-buildmode", "-trimpath"})
		if _, ok := observed["vcs.revision"]; ok {
			var revision string
			if v.field(observed, "vcs.revision", p+".observed_build", &revision) {
				if !resourceGitSHA(revision) {
					v.invalid(p+".observed_build.vcs.revision", "must be a full 40-character hexadecimal Git SHA")
				}
				if provided != "" && !strings.EqualFold(provided, revision) {
					v.invalid(p+".provided_source_sha", "disagrees with recorded observed_build.vcs.revision")
				}
			}
		}
	}
	if raw, ok := identity["build_setting_hashes"]; ok {
		resourceStringMap(v, raw, p+".build_setting_hashes", true)
	}
	if _, ok := identity["build_info_sha256"]; ok {
		if !isSHA256(v.text(identity, "build_info_sha256", p)) {
			v.invalid(p+".build_info_sha256", "must contain 64 hexadecimal digits")
		}
	}
}

func resourceFixtureMetadata(v *validation, fixture object, p string) {
	if _, ok := fixture["provided_identity"]; ok {
		var provided string
		v.field(fixture, "provided_identity", p, &provided)
	}
	if raw, ok := fixture["provenance"]; ok {
		p += ".provenance"
		provenance := v.object(raw, p, []string{"input_file_hashes", "observed_package_versions"}, nil)
		resourceStringMap(v, provenance["input_file_hashes"], p+".input_file_hashes", true)
		packages := v.object(provenance["observed_package_versions"], p+".observed_package_versions", nil, nil)
		for _, name := range slices.Sorted(maps.Keys(packages)) {
			pp := p + ".observed_package_versions." + name
			entry := v.object(packages[name], pp, []string{"version", "manifest_sha256"}, nil)
			var version string
			v.field(entry, "version", pp, &version)
			if !isSHA256(v.text(entry, "manifest_sha256", pp)) {
				v.invalid(pp+".manifest_sha256", "must contain 64 hexadecimal digits")
			}
		}
	}
}

func resourceRepositories(v *validation, snapshot object, p string) {
	var repositories []json.RawMessage
	if !v.field(snapshot, "repositories", p, &repositories) {
		return
	}
	for i, raw := range repositories {
		rp := fmt.Sprintf("%s.repositories[%d]", p, i)
		entry := v.object(raw, rp, []string{"root", "identity"}, nil)
		root := v.text(entry, "root", rp)
		if !resourceAbsolute(root) {
			v.invalid(rp+".root", "must be an absolute repository/input root path")
		}
		if bytes.Equal(bytes.TrimSpace(entry["identity"]), []byte("null")) {
			continue // Native harness records non-Git input roots this way.
		}
		identity := v.object(entry["identity"], rp+".identity", []string{"root", "sha", "status"}, nil)
		if observed := v.text(identity, "root", rp+".identity"); !resourceAbsolute(observed) || !resourcePathEqual(root, observed) {
			v.invalid(rp+".identity.root", "must match the absolute recorded repository root")
		}
		if !resourceGitSHA(v.text(identity, "sha", rp+".identity")) {
			v.invalid(rp+".identity.sha", "must be a full 40-character hexadecimal Git SHA")
		}
		var status string
		v.field(identity, "status", rp+".identity", &status)
	}
}

func resourceStableMetadata(v *validation, state, fixture object, invocationJournal string) {
	p := "$.run.project_state"
	root, work, active := v.text(state, "evidence_root", p), v.text(state, "work_root", p), v.text(state, "active_path", p)
	if !resourceAbsolute(root) || !resourcePathEqual(root, path.Dir(resourcePath(invocationJournal))) || !resourceAbsolute(work) || !resourcePathEqual(path.Dir(resourcePath(work)), root) || !resourceWorkName(path.Base(resourcePath(work))) || !resourceAbsolute(active) || !resourcePathEqual(active, path.Join(resourcePath(work), "active")) {
		v.invalid(p, "stable root/work/active slots must match the recorded evidence directory and exact owned work root")
	}
	arms := v.object(state["arms"], p+".arms", []string{"baseline", "candidate"}, nil)
	for _, arm := range []string{"baseline", "candidate"} {
		if !resourcePathEqual(v.text(arms, arm, p+".arms"), path.Join(resourcePath(work), arm)) {
			v.invalid(p+".arms."+arm, "must use the exact owned arm slot")
		}
	}
	locations := v.object(state["locations"], p+".locations", []string{"active_exists", "baseline_exists", "candidate_exists"}, nil)
	for _, key := range []string{"active_exists", "baseline_exists", "candidate_exists"} {
		var exists bool
		if v.field(locations, key, p+".locations", &exists) && exists {
			v.invalid(p+".locations."+key, "cleaned final run must record all owned work slots absent")
		}
	}
	for key, name := range map[string]string{"state_file": "project-state.json", "journal": "project-state.ndjson", "input_metadata_file": "project-input-metadata.json"} {
		if _, ok := state[key]; ok {
			if !resourcePathEqual(v.text(state, key, p), path.Join(resourcePath(root), name)) {
				v.invalid(p+"."+key, "must match its fixed path beneath the recorded evidence directory")
			}
		}
	}
	if _, ok := state["canonical_root"]; ok {
		var fixturePath string
		_ = json.Unmarshal(fixture["path"], &fixturePath)
		canonical := v.text(state, "canonical_root", p)
		if !resourceAbsolute(canonical) || !resourcePathEqual(canonical, fixturePath) {
			v.invalid(p+".canonical_root", "must match the absolute fixture path in recorded input provenance")
		}
	}
	if _, ok := state["input_metadata_sha256"]; ok {
		hash := v.text(state, "input_metadata_sha256", p)
		if !strings.HasPrefix(hash, "sha256-project-state-v1:") || !isSHA256(strings.TrimPrefix(hash, "sha256-project-state-v1:")) {
			v.invalid(p+".input_metadata_sha256", "must be a sha256-project-state-v1 identity")
		}
	}
}

func resourceUint(v *validation, obj object, key, p string, max uint64) (uint64, bool) {
	var n uint64
	if !v.field(obj, key, p, &n) {
		return 0, false
	}
	if n > max {
		v.invalid(p+"."+key, "counter exceeds native field range")
		return 0, false
	}
	return n, true
}

func resourceNonnegative(v *validation, obj object, key, p string, nullable bool) {
	if nullable && bytes.Equal(bytes.TrimSpace(obj[key]), []byte("null")) {
		return
	}
	// Syntax is already checked. Inspect the significand before binary64 can
	// underflow a mathematically negative value into an accepted negative zero.
	lexeme := strings.TrimSpace(string(obj[key]))
	if strings.HasPrefix(lexeme, "-") {
		significand, _, _ := strings.Cut(strings.ToLower(lexeme), "e")
		if strings.ContainsAny(significand, "123456789") {
			v.invalid(p+"."+key, "must be finite and nonnegative")
			return
		}
	}
	var n float64
	if v.field(obj, key, p, &n) && (n < 0 || math.IsNaN(n) || math.IsInf(n, 0)) {
		v.invalid(p+"."+key, "must be finite and nonnegative")
	}
}

func (b *resourceBatch) validateResult(v *validation, o *resourceObservation, p string) {
	p += ".result"
	r := v.object(o.entry["result"], p, []string{"DurationMS", "ElapsedMS", "ExitCode", "Status", "Error", "EvidencePersistenceMS", "CaptureLimitBytes", "ResourceAccounting", "StdoutBytes", "StderrBytes", "CaptureOverflow", "CaptureSpoolWriteMS", "DiagnosticsDigest", "OutputTreeDigest"}, nil)
	resourceExpectText(v, r, "Status", p, "completed")
	resourceNull(v, r, "Error", p)
	var duration int64
	if v.field(r, "DurationMS", p, &duration) && (duration <= 0 || duration > math.MaxInt64/2) {
		v.invalid(p+".DurationMS", "must be positive and <= MaxInt64/2")
	}
	var exit int
	if v.field(r, "ExitCode", p, &exit) && exit != 0 {
		v.invalid(p+".ExitCode", "completed invocation must exit 0")
	}
	for _, key := range []string{"ElapsedMS", "EvidencePersistenceMS", "CaptureSpoolWriteMS"} {
		resourceNonnegative(v, r, key, p, false)
	}
	var elapsed float64
	if json.Unmarshal(r["ElapsedMS"], &elapsed) == nil {
		rounded := math.Max(1, math.RoundToEven(elapsed))
		if math.IsNaN(rounded) || math.IsInf(rounded, 0) || rounded > float64(math.MaxInt64/2) || int64(rounded) != duration {
			v.invalid(p+".DurationMS", "must equal max(1, round-to-even(ElapsedMS)) from the final harness")
		}
	}
	limit, _ := resourceUint(v, r, "CaptureLimitBytes", p, math.MaxInt64)
	var declared uint64
	_ = json.Unmarshal(b.run["capture_limit_bytes_per_stream"], &declared)
	if limit == 0 || limit != declared {
		v.invalid(p+".CaptureLimitBytes", "must match positive run capture limit")
	}
	for _, key := range []string{"StdoutBytes", "StderrBytes"} {
		if n, ok := resourceUint(v, r, key, p, math.MaxInt64); ok && n > limit {
			v.invalid(p+"."+key, "completed capture exceeds retained limit")
		}
	}
	var overflow bool
	if v.field(r, "CaptureOverflow", p, &overflow) && overflow {
		v.invalid(p+".CaptureOverflow", "overflowed invocation is incomplete")
	}
	for key, format := range map[string]string{"DiagnosticsDigest": "sha256-diagnostics-v2:", "OutputTreeDigest": "sha256-tree-v2:"} {
		hash := v.text(r, key, p)
		if !strings.HasPrefix(hash, format) || !isSHA256(strings.TrimPrefix(hash, format)) {
			v.invalid(p+"."+key, "must contain the final format and 64 hexadecimal digits")
		}
	}
	b.validateResources(v, o, r["ResourceAccounting"], p+".ResourceAccounting")
}

func (b *resourceBatch) validateResources(v *validation, o *resourceObservation, raw json.RawMessage, p string) {
	r := v.object(raw, p, []string{"Schema", "Status", "Completion", "CompletionError", "JobAccounting", "JobMemory", "RootCpu", "OtherOwnedCpu", "QueryElapsedMS"}, nil)
	var schema int
	if v.field(r, "Schema", p, &schema) && schema != 1 {
		v.invalid(p+".Schema", "must be 1")
	}
	resourceExpectText(v, r, "Completion", p, "complete")
	resourceNull(v, r, "CompletionError", p)
	resourceNonnegative(v, r, "QueryElapsedMS", p, true)
	o.metrics, o.CounterValues = map[string]float64{}, map[string]uint64{}
	o.ComponentStatuses, o.ComponentCauses = map[string]string{}, map[string]string{}
	allAvailable, anyAvailable, allUnsupported, allUnavailable := true, false, true, true
	components := map[string]object{}
	for _, name := range []string{"JobAccounting", "JobMemory", "RootCpu", "OtherOwnedCpu"} {
		before := len(v.issues)
		cp := p + "." + name
		component := v.object(r[name], cp, []string{"Status", "Error", "Values"}, nil)
		status := v.text(component, "Status", cp)
		o.ComponentStatuses[name] = status
		allAvailable, anyAvailable = allAvailable && status == "available", anyAvailable || status == "available"
		allUnsupported, allUnavailable = allUnsupported && status == "unsupported", allUnavailable && status == "unavailable"
		if status == "available" {
			resourceNull(v, component, "Error", cp)
			keys := []string{"UserTicks100ns", "KernelTicks100ns"}
			switch name {
			case "JobAccounting":
				keys = append(keys, "TotalProcesses", "ActiveProcesses", "LimitTerminatedProcesses", "TotalPageFaults", "ReadOperations", "WriteOperations", "OtherOperations", "ReadBytes", "WriteBytes", "OtherBytes", "StructureBytes")
			case "JobMemory":
				keys = []string{"PeakJobMemoryBytes", "PeakProcessMemoryBytes", "StructureBytes"}
			}
			values := v.object(component["Values"], cp+".Values", keys, nil)
			for _, key := range keys {
				max := uint64(math.MaxUint64)
				switch key {
				case "UserTicks100ns", "KernelTicks100ns":
					max = math.MaxInt64
				case "TotalProcesses", "ActiveProcesses", "LimitTerminatedProcesses", "TotalPageFaults":
					max = math.MaxUint32
				case "StructureBytes":
					max = math.MaxInt32
				}
				if n, ok := resourceUint(v, values, key, cp+".Values", max); ok && key == "StructureBytes" && n == 0 {
					v.invalid(cp+".Values."+key, "available native structure size must be positive")
				}
			}
			if name == "JobAccounting" {
				var active, total uint64
				_ = json.Unmarshal(values["ActiveProcesses"], &active)
				_ = json.Unmarshal(values["TotalProcesses"], &total)
				if active != 0 || total == 0 {
					v.invalid(cp+".Values", "completed owned job requires no active members and a positive lifetime count")
				}
			}
			if len(v.issues) == before {
				components[name] = values
			}
		} else {
			if status != "failed" && status != "unsupported" && status != "unavailable" {
				v.invalid(cp+".Status", "must be available, failed, unsupported or unavailable")
			}
			cause := v.text(component, "Error", cp)
			o.ComponentCauses[name] = cause
			resourceNull(v, component, "Values", cp)
			if o.Scored && (name == "JobAccounting" || name == "JobMemory") {
				v.add(incompleteResource, cp, "required component "+status+": "+cause+"; whole affected metric population remains unmeasured")
			}
		}
	}
	if anyAvailable && bytes.Equal(bytes.TrimSpace(r["QueryElapsedMS"]), []byte("null")) {
		v.invalid(p+".QueryElapsedMS", "queried available components require a nonnegative getter duration")
	}
	wantStatus := "failed"
	switch {
	case allAvailable:
		wantStatus = "available"
	case anyAvailable:
		wantStatus = "partial"
	case allUnsupported:
		wantStatus = "unsupported"
	case allUnavailable:
		wantStatus = "unavailable"
	}
	resourceExpectText(v, r, "Status", p, wantStatus)
	get := func(obj object, key string) uint64 {
		var n uint64
		_ = json.Unmarshal(obj[key], &n)
		return n
	}
	if values, ok := components["JobAccounting"]; ok {
		user, kernel := get(values, "UserTicks100ns"), get(values, "KernelTicks100ns")
		if user > math.MaxUint64-kernel {
			v.invalid(p+".JobAccounting.Values", "CPU sum overflows uint64")
		} else {
			total := user + kernel
			o.JobUserTicks100ns, o.JobKernelTicks100ns, o.JobTotalTicks100ns = &user, &kernel, &total
			o.CounterValues["job_cpu_ticks_100ns"], o.metrics["job_cpu_ms"] = total, float64(total)/10000
		}
		for key, metric := range map[string]string{"ReadOperations": "read_operations", "WriteOperations": "write_operations", "OtherOperations": "other_operations", "ReadBytes": "read_bytes", "WriteBytes": "write_bytes", "OtherBytes": "other_bytes"} {
			n := get(values, key)
			o.CounterValues[metric], o.metrics[metric] = n, float64(n)
		}
	}
	if values, ok := components["JobMemory"]; ok {
		n := get(values, "PeakJobMemoryBytes")
		o.CounterValues["job_peak_bytes"], o.metrics["job_peak_bytes"] = n, float64(n)
	}
	if other, ok := components["OtherOwnedCpu"]; ok {
		job, jobOK := components["JobAccounting"]
		root, rootOK := components["RootCpu"]
		if !jobOK || !rootOK {
			v.invalid(p+".OtherOwnedCpu", "available derived CPU requires available job and root CPU")
		} else {
			for _, key := range []string{"UserTicks100ns", "KernelTicks100ns"} {
				j, r, o := get(job, key), get(root, key), get(other, key)
				if j < r || o != j-r {
					v.invalid(p+".OtherOwnedCpu.Values."+key, "does not equal nonnegative exact job minus root CPU")
				}
			}
		}
	}
}

// The harness retains Windows paths. Normalize them lexically without reading
// or rewriting evidence; moving raw files does not rewrite their provenance.
func resourcePath(s string) string { return path.Clean(strings.ReplaceAll(s, "\\", "/")) }
func resourcePathEqual(a, b string) bool {
	return strings.EqualFold(resourcePath(a), resourcePath(b))
}
func resourceAbsolute(s string) bool {
	s = strings.ReplaceAll(s, "\\", "/")
	return strings.HasPrefix(s, "/") || len(s) >= 3 && s[1] == ':' && s[2] == '/'
}

func resourceWorkName(name string) bool {
	if !strings.HasPrefix(name, "work-") || len(name) != 37 {
		return false
	}
	for _, c := range name[5:] {
		if (c < '0' || c > '9') && (c < 'a' || c > 'f') {
			return false
		}
	}
	return true
}

func (b *resourceBatch) validateInvocationPaths(v *validation, o *resourceObservation, p string) {
	get := func(obj object, key string) string {
		var s string
		_ = json.Unmarshal(obj[key], &s)
		return s
	}
	before := v.object(b.run["before"], "$.run.before", []string{"baseline", "candidate"}, nil)
	identity := v.object(before[o.Binary], "$.run.before."+o.Binary, []string{"path"}, nil)
	if !resourcePathEqual(v.text(o.entry, "executable", p), get(identity, "path")) {
		v.invalid(p+".executable", "does not match retained arm executable")
	}
	journal := resourcePath(get(b.run, "invocation_journal"))
	dir := path.Dir(journal)
	prefix := "candidate-timings"
	if o.Scored {
		prefix = fmt.Sprintf("%s-%d-%s", o.Phase, o.Pair, o.Binary)
	}
	for key, suffix := range map[string]string{"stdout": ".stdout.json", "stderr": ".stderr.txt"} {
		if !resourcePathEqual(v.text(o.entry, key, p), path.Join(dir, prefix+suffix)) {
			v.invalid(p+"."+key, "does not match invocation evidence path")
		}
	}
	work := resourcePath(v.text(o.entry, "working_directory", p))
	mode := get(b.run, "project_mode")
	if mode == "stable-project-path-v1" {
		var state object
		_ = json.Unmarshal(b.run["project_state"], &state)
		if !resourcePathEqual(work, get(state, "active_path")) {
			v.invalid(p+".working_directory", "stable invocation must use the declared active slot")
		}
	} else {
		root := path.Dir(work)
		name := path.Base(root)
		if !resourcePathEqual(path.Dir(root), dir) || path.Base(work) != o.Binary || !resourceWorkName(name) {
			v.invalid(p+".working_directory", "must use its owned work-<guid>/arm slot beneath evidence root")
		}
		if len(b.Observations) > 0 {
			first := resourcePath(get(b.Observations[0].entry, "working_directory"))
			if !resourcePathEqual(path.Dir(first), root) {
				v.invalid(p+".working_directory", "work root changed within one run")
			}
		}
	}
}

func validateResourcePaths(b *resourceBatch, manifestPath, journalPath, runPath string) {
	v := validation{}
	paths := []string{manifestPath, journalPath, runPath}
	want := []string{"windows-build-performance-manifest.json", "invocations.ndjson", "run.json"}
	for i := range paths {
		resolved, err := filepath.Abs(paths[i])
		if err != nil {
			v.invalid("$.input_paths", "cannot resolve input path: "+err.Error())
			continue
		}
		paths[i] = resourcePath(resolved)
		if path.Base(paths[i]) != want[i] {
			v.invalid("$.input_paths", "final protocol requires published "+want[i]+" filename")
		}
	}
	if !resourcePathEqual(path.Dir(paths[0]), path.Dir(paths[1])) || !resourcePathEqual(path.Dir(paths[0]), path.Dir(paths[2])) {
		v.invalid("$.input_paths", "published manifest, journal and run metadata must share one evidence directory")
	}
	var declared string
	_ = json.Unmarshal(b.run["invocation_journal"], &declared)
	if !resourcePathEqual(declared, paths[1]) {
		v.invalid("$.run.invocation_journal", "does not identify supplied resource journal path")
	}
	var mode string
	_ = json.Unmarshal(b.run["project_mode"], &mode)
	if mode == "stable-project-path-v1" {
		var state object
		_ = json.Unmarshal(b.run["project_state"], &state)
		var root, workRoot, active string
		_ = json.Unmarshal(state["evidence_root"], &root)
		_ = json.Unmarshal(state["work_root"], &workRoot)
		_ = json.Unmarshal(state["active_path"], &active)
		if !resourcePathEqual(root, path.Dir(paths[0])) || !resourcePathEqual(path.Dir(resourcePath(workRoot)), root) || !resourceWorkName(path.Base(resourcePath(workRoot))) || !resourcePathEqual(active, path.Join(resourcePath(workRoot), "active")) {
			v.invalid("$.run.project_state", "stable slots do not correspond to supplied evidence directory")
		}
		arms := v.object(state["arms"], "$.run.project_state.arms", []string{"baseline", "candidate"}, nil)
		for _, arm := range []string{"baseline", "candidate"} {
			if !resourcePathEqual(v.text(arms, arm, "$.run.project_state.arms"), path.Join(resourcePath(workRoot), arm)) {
				v.invalid("$.run.project_state.arms."+arm, "must use the exact owned arm slot")
			}
		}
	}
	b.Issues = append(b.Issues, v.issues...)
	if len(v.issues) != 0 {
		b.Resources = map[string]map[string][]pair{}
	}
}
