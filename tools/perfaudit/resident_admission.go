package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math"
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"time"
)

type residentRawBatch struct {
	Ref           residentBatchRef           `json:"binding"`
	Reconstructed ResidentBatch              `json:"reconstructed"`
	Issues        []issue                    `json:"issues"`
	Clocks        []residentClockObservation `json:"clock_observations"`
	Status        string                     `json:"admission_status"`
	HostSHA256    string                     `json:"host_sha256"`
	FixtureSHA256 string                     `json:"fixture_sha256"`
	OneTick       bool                       `json:"one_tick_unresolved"`
	Start         time.Time                  `json:"first_start"`
	Finish        time.Time                  `json:"last_finish"`
	plan          object
	host          object
	hostValid     bool
}

var regexpResidentBatch = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9_-]{0,79}$`)

func (r *residentReader) batch(ref residentBatchRef, key ResidentKey, purpose string) residentRawBatch {
	begin := len(r.issues)
	b := residentRawBatch{Ref: ref}
	dir := ref.Directory
	read := func(name string) *residentSnapshot { return r.snapshot(filepath.Join(dir, name), false) }
	planSnap := r.bound(ref.Plan, false)
	copySnap := read("plan.raw.json")
	if !bytes.Equal(planSnap.data, copySnap.data) {
		r.invalid(dir, "original launcher plan differs from retained plan.raw.json")
	}
	p := r.obj(planSnap.data, ref.Plan.Path, "schema batch mode stage comparator semanticLabel pairs cycles order fixtureParent fixtureOwner evidenceDir workingDirectory timeoutMs captureBytes reportBytes captureSha256 launcherSha256 fixtureInput arms inputs protectedRoots provenance", "")
	b.plan = p
	for _, k := range []string{"fixtureParent", "evidenceDir", "workingDirectory"} {
		if !residentLocalPath(r.str(p, k, dir)) {
			r.invalid(dir+"."+k, "literal absolute local path required")
		}
	}
	if r.str(p, "semanticLabel", dir) == "" || r.str(p, "provenance", dir) == "" {
		r.invalid(dir, "nonempty semantic and provenance declarations required")
	}
	r.number(p, "schema", dir, 1)
	r.want(p, "mode", dir, "session")
	batch := r.str(p, "batch", dir)
	if !regexpResidentBatch.MatchString(batch) {
		r.invalid(dir, "invalid frozen batch ID")
	}
	b.Reconstructed = ResidentBatch{Label: residentRecorded, ID: batch, Chronology: ref.Chronology, Key: key, Purpose: purpose}
	if r.kind == "invented-protocol" {
		b.Reconstructed.Label = residentSynthetic
		if !strings.HasPrefix(r.str(p, "provenance", dir), "INVENTED SYNTHETIC PROTOCOL") {
			r.invalid(dir, "invented input requires an explicit invented source declaration")
		}
	}
	stage := r.str(p, "stage", dir)
	mapped := map[string]string{"initial": "initial", "followup": "follow_up", "confirmation": "confirmation", "calibration-initial": "initial", "calibration-followup": "follow_up"}
	b.Reconstructed.Stage = mapped[stage]
	if b.Reconstructed.Stage == "" || (purpose == "calibration") != strings.HasPrefix(stage, "calibration-") {
		r.invalid(dir, "purpose/stage mapping is inconsistent")
	}
	count := 20
	if b.Reconstructed.Stage == "follow_up" {
		count = 40
	}
	r.number(p, "pairs", dir, uint64(count/2))
	r.number(p, "cycles", dir, 30)
	expectedComparator := key.Comparator
	if purpose == "calibration" {
		expectedComparator = "B:B"
		if key.Comparator == "S:C" {
			expectedComparator = "S:S"
		}
	}
	r.want(p, "comparator", dir, expectedComparator)
	r.pathEqual(r.str(p, "evidenceDir", dir), dir, dir+".evidenceDir")
	orders := r.arr(p["order"], dir+".order", 40)
	if len(orders) != count {
		r.invalid(dir, "frozen ABBA launch count inconsistent")
	}
	for i, v := range orders {
		var arm string
		if json.Unmarshal(v, &arm) != nil || arm != []string{"A", "B", "B", "A"}[i%4] {
			r.invalid(dir, "frozen order is not complete ABBA")
		}
	}
	for k, max := range map[string]uint64{"timeoutMs": 86400000, "reportBytes": 1 << 30, "captureBytes": 1 << 30} {
		if r.uint(p, k, dir, max) == 0 {
			r.invalid(dir+"."+k, "positive declared bound required")
		}
	}
	before := read("inputs.before.json")
	after := read("inputs.after.json")
	hostBegin := len(r.issues)
	hb := read("host.before.json")
	ha := read("host.after.json")
	b.HostSHA256 = hb.SHA256
	r.equal(hb.data, ha.data, dir+".host.after")
	host := r.obj(hb.data, dir+".host", "os architecture processors powershell power_scheme environment qpc_support", "")
	b.host = host
	r.hostIdentity(host, dir)
	b.hostValid = len(r.issues) == hostBegin
	manifest := read("evidence.manifest.json")
	m := r.obj(manifest.data, manifest.Path, "schema format valid journal retained errors", "")
	r.number(m, "schema", dir, 1)
	r.want(m, "format", dir, "resident-evidence-v1")
	r.flag(m, "valid", dir, true)
	if len(r.arr(m["errors"], dir+".manifest.errors", 512)) != 0 {
		r.invalid(dir, "retention errors cannot be erased by a valid flag")
	}
	bindings := map[string]object{}
	bindingKeys := map[string]bool{}
	for i, raw := range r.arr(m["retained"], dir+".retained", 512) {
		q := fmtResidentIndex(dir+".retained", i)
		v := r.obj(raw, q, "path role original expected_sha256 expected_bytes observed retention_error error", "")
		path, role := r.str(v, "path", q), r.str(v, "role", q)
		original := ""
		if role == "metadata" {
			resourceNull(&r.validation, v, "original", q)
		} else {
			original = r.str(v, "original", q)
		}
		hash := r.str(v, "expected_sha256", q)
		n := r.uint(v, "expected_bytes", q, 256<<20)
		if !residentWithin(dir, path) {
			r.invalid(q, "retained binding is outside original evidence directory")
		}
		if _, seen := bindings[residentPathKey(path)]; seen {
			r.invalid(q, "duplicate retained path")
		}
		bindings[residentPathKey(path)] = v
		resourceNull(&r.validation, v, "retention_error", q)
		resourceNull(&r.validation, v, "error", q)
		s := r.bound(residentFileRef{path, hash}, false)
		if uint64(s.Bytes) != n {
			r.invalid(q, "retained byte count mismatch")
		}
		observed := r.obj(v["observed"], q+".observed", "path sha256 bytes", "")
		r.pathEqual(r.str(observed, "path", q), path, q)
		r.want(observed, "sha256", q, s.SHA256)
		r.number(observed, "bytes", q, uint64(s.Bytes))
		bindingKeys[role+"\x00"+residentPathKey(original)+"\x00"+residentPathKey(path)] = true
	}
	require := func(path, role, original string) {
		k := role + "\x00" + residentPathKey(original) + "\x00" + residentPathKey(path)
		if !bindingKeys[k] {
			r.invalid(path, "required retained binding missing: "+role)
		}
	}
	require(copySnap.Path, "plan.raw", ref.Plan.Path)
	for _, name := range []string{"batch.started.json", "host.before.json", "inputs.before.json", "inputs.after.json", "host.after.json"} {
		path := filepath.Join(dir, name)
		require(path, "metadata", "")
	}
	inputCopies := r.inputs(p, before, after, ref, require)
	fixtureRef := r.ref(p["fixtureInput"], dir+".fixtureInput")
	fixtureData := inputCopies[residentPathKey(fixtureRef.Path)]
	frozen := r.obj(fixtureData, dir+".frozen_fixture", "fixture fixtureSha256", "")
	b.FixtureSHA256 = r.str(frozen, "fixtureSha256", dir)
	fixture := r.fixture(frozen["fixture"], b.FixtureSHA256, key, dir)
	arms := r.obj(p["arms"], dir+".arms", "A B", "")
	r.builds(p, arms, inputCopies, key, purpose, dir)
	started := r.obj(read("batch.started.json").data, dir+".started", "schema batch status mode stage planned finished completed failed incomplete_unmeasured error promotion_ready", "")
	r.number(started, "schema", dir, 1)
	r.want(started, "batch", dir, batch)
	r.want(started, "status", dir, "incomplete")
	r.want(started, "mode", dir, "session")
	r.want(started, "stage", dir, stage)
	r.number(started, "planned", dir, uint64(count))
	for _, k := range []string{"finished", "completed", "failed", "incomplete_unmeasured"} {
		r.number(started, k, dir, 0)
	}
	resourceNull(&r.validation, started, "error", dir)
	r.flag(started, "promotion_ready", dir, false)
	finishedSnap := read("batch.finished.json")
	finished := r.obj(finishedSnap.data, dir+".finished", "schema batch status mode stage planned finished completed failed incomplete_unmeasured error promotion_ready evidence_manifest", "")
	if !bytes.Equal(bytes.TrimSpace(finished["error"]), []byte("null")) {
		r.str(finished, "error", dir+".finished")
	}
	for _, k := range []string{"schema", "batch", "mode", "stage", "planned", "promotion_ready"} {
		r.equal(started[k], finished[k], dir+".finished."+k)
	}
	identity := r.obj(finished["evidence_manifest"], dir+".manifest_identity", "path sha256 bytes", "")
	r.pathEqual(r.str(identity, "path", dir), manifest.Path, dir)
	r.want(identity, "sha256", dir, manifest.SHA256)
	r.number(identity, "bytes", dir, uint64(manifest.Bytes))
	journal := read("journal.ndjson")
	jid := r.obj(m["journal"], dir+".journal_identity", "path sha256 bytes", "")
	r.pathEqual(r.str(jid, "path", dir), journal.Path, dir)
	r.want(jid, "sha256", dir, journal.SHA256)
	r.number(jid, "bytes", dir, uint64(journal.Bytes))
	lines := bytes.Split(journal.data, []byte{'\n'})
	if len(lines) > 0 && len(lines[len(lines)-1]) == 0 {
		lines = lines[:len(lines)-1]
	} else {
		r.invalid(dir, "journal lacks its persisted final newline")
	}
	if len(lines) > 2*count {
		r.invalid(dir, "journal exceeds planned prefix")
		lines = lines[:2*count]
	}
	if len(lines) != 2*count {
		r.add(residentIncomplete, dir, "complete session population unavailable; journal prefix retained")
	}
	var previous time.Time
	var start object
	var layout uint64
	completed, failed, missing := 0, 0, 0
	for i, raw := range lines {
		q := fmtResidentIndex(dir+".journal", i)
		e := r.obj(raw, q, residentJournalKeys, residentFinishedKeys)
		position := i/2 + 1
		arm := []string{"A", "B", "B", "A"}[(position-1)%4]
		pair := (position-1)/2 + 1
		block := (position-1)/4 + 1
		order := "AB"
		if pair%2 == 0 {
			order = "BA"
		}
		id := fmt.Sprintf("%s-%04d", batch, position)
		orderLabel := fmt.Sprintf("batch=%s;block=%d;pair=%d;order=%s;position=%d;arm=%s", batch, block, pair, order, position, arm)
		r.number(e, "schema", q, 1)
		for k, v := range map[string]string{"session": id, "batch": batch, "mode": "session", "stage": stage, "comparator": expectedComparator, "semantic_label": r.str(p, "semanticLabel", dir), "arm": arm, "pair_order": order, "order": orderLabel, "resource_accounting_format": resourceFormat, "resource_scope": "whole owned job: startup/setup/warm/mutation/cycles/report/cleanup; outer launcher excluded"} {
			r.want(e, k, q, v)
		}
		for k, v := range map[string]int{"position": position, "pair": pair, "block": block} {
			r.number(e, k, q, uint64(v))
		}
		stamp := r.stamp(e, "at_utc", q)
		if !previous.IsZero() && stamp.Before(previous) {
			r.invalid(q, "journal time moved backwards")
		}
		previous = stamp
		if i == 0 {
			b.Start = stamp
		}
		b.Finish = stamp
		a := r.obj(arms[arm], q+".arm", "executable sha256 production buildSource productionManifestSha256 harness overlaySha256 buildEvidence runtime", "")
		for ek, ak := range map[string]string{"executable": "executable", "executable_sha256": "sha256", "production": "production", "harness": "harness", "build_source": "buildSource", "production_manifest_sha256": "productionManifestSha256"} {
			r.equal(e[ek], a[ak], q+"."+ek)
		}
		for k, v := range map[string]string{"report_path": filepath.Join(dir, id+".report.json"), "stdout_path": filepath.Join(dir, id+".stdout.txt"), "stderr_path": filepath.Join(dir, id+".stderr.txt"), "working_directory": r.str(p, "workingDirectory", dir), "fixture_leaf": filepath.Join(r.str(p, "fixtureParent", dir), key.Scenario, key.Settings)} {
			r.pathEqual(r.str(e, k, q), v, q+"."+k)
		}
		units := r.obj(e["resource_units"], q+".units", "cpu memory io", "")
		for k, v := range map[string]string{"cpu": "100ns ticks", "memory": "simultaneous peak job committed bytes", "io": "OS IO operations/bytes; all six class-8 counters"} {
			r.want(units, k, q, v)
		}
		args := r.arguments(p, a, key, id, orderLabel, r.str(e, "report_path", q))
		encoded, _ := json.Marshal(args)
		r.equal(e["arguments"], encoded, q+".arguments")
		if i%2 == 0 {
			if failed+missing > 0 {
				r.invalid(q, "launch after failed prefix is forbidden")
			}
			r.want(e, "event", q, "started")
			r.want(e, "status", q, "incomplete")
			r.absent(e, q, strings.Fields(residentFinishedKeys)...)
			start = e
			continue
		}
		r.want(e, "event", q, "finished")
		for _, k := range strings.Fields(residentJournalKeys) {
			if !slices.Contains([]string{"event", "at_utc", "status"}, k) {
				r.equal(start[k], e[k], q+".started/finished."+k)
			}
		}
		status := r.str(e, "status", q)
		switch status {
		case "completed":
			completed++
		case residentIncomplete:
			missing++
			r.add(residentIncomplete, q, "required native measurement unavailable")
		default:
			failed++
			r.invalid(q, "failed collector/session transport cannot populate statistics")
		}
		for _, k := range []string{"validation_completed", "owner_disposed", "fixture_absent"} {
			r.flag(e, k, q, true)
		}
		reasons := r.arr(e["reasons"], q+".reasons", 128)
		for j, reason := range reasons {
			r.str(object{"reason": reason}, "reason", fmtResidentIndex(q+".reasons", j))
		}
		if status == "completed" && len(reasons) != 0 {
			r.invalid(q, "completed session carries failure reasons")
		}
		r.uint(e, "finalization_ticks", q, math.MaxInt64)
		r.uint(e, "stopwatch_frequency", q, math.MaxInt64)
		reportIdentity := r.obj(e["report"], q+".report", "path sha256 bytes snapshot_path", "")
		reportPath := r.str(e, "report_path", q)
		snapshotPath := strings.TrimSuffix(reportPath, ".json") + ".snapshot.raw"
		r.pathEqual(r.str(reportIdentity, "path", q), reportPath, q)
		r.pathEqual(r.str(reportIdentity, "snapshot_path", q), snapshotPath, q)
		rr := r.bound(residentFileRef{reportPath, r.str(reportIdentity, "sha256", q)}, false)
		if uint64(rr.Bytes) > r.uint(p, "reportBytes", q, 1<<30) {
			r.invalid(q, "report exceeds its frozen byte bound")
		}
		rs := r.bound(residentFileRef{snapshotPath, rr.SHA256}, false)
		r.number(reportIdentity, "bytes", q, uint64(rr.Bytes))
		if !bytes.Equal(rr.data, rs.data) {
			r.invalid(q, "canonical/snapshot bytes differ")
		}
		require(reportPath, "report", reportPath)
		require(snapshotPath, "report", reportPath)
		report := r.report(rr.data, p, a, e, fixture, key, id, orderLabel, q)
		r.runtimeEnvironment(report, host, q)
		observation, cycles := r.clock(report, e, host, q)
		if len(b.Clocks) > 0 && observation.serialValid && b.Clocks[len(b.Clocks)-1].serialValid {
			r.serialClock(b.Clocks[len(b.Clocks)-1], observation, q)
		}
		b.Clocks = append(b.Clocks, observation)
		b.OneTick = b.OneTick || observation.OneTick
		resources := r.native(e, p, q, &layout)
		if position%4 == 1 {
			b.Reconstructed.Blocks = append(b.Reconstructed.Blocks, ResidentBlock{Position: block})
		}
		if len(b.Reconstructed.Blocks) == 0 {
			r.invalid(q, "missing block prefix")
			continue
		}
		semantic := "same-output"
		if key.Role == residentCost && purpose == "comparison" {
			semantic = "expected-stale-B"
		}
		s := ResidentSession{ID: id, Launch: position, Pair: pair, Order: order, Arm: arm, Semantic: semantic, Cycles: cycles, Resources: resources}
		last := len(b.Reconstructed.Blocks) - 1
		b.Reconstructed.Blocks[last].Sessions = append(b.Reconstructed.Blocks[last].Sessions, s)
	}
	for k, n := range map[string]int{"finished": len(lines) / 2, "completed": completed, "failed": failed, "incomplete_unmeasured": missing} {
		r.number(finished, k, dir, uint64(n))
	}
	finalStatus := r.str(finished, "status", dir)
	if !slices.Contains([]string{"completed", "incomplete", "failed"}, finalStatus) {
		r.invalid(dir, "unknown final batch status")
	}
	if finalStatus == "failed" {
		r.invalid(dir, "failed batch cannot populate statistics")
	}
	if finalStatus != "completed" {
		r.add(residentIncomplete, dir, "batch is not complete; no successful subset used")
	} else {
		resourceNull(&r.validation, finished, "error", dir)
		if completed != count {
			r.invalid(dir, "completed batch has missing/failed invocations")
		}
	}
	b.Issues = residentOrderedIssues(r.issues[begin:])
	b.Status = residentIssueStatus(b.Issues)
	return b
}

const residentJournalKeys = "event schema session batch mode stage comparator semantic_label arm position pair block pair_order order at_utc status executable executable_sha256 production harness arguments working_directory report_path stdout_path stderr_path fixture_leaf resource_accounting_format resource_units resource_scope build_source production_manifest_sha256"
const residentFinishedKeys = "validation_completed owner_disposed fixture_absent report result reasons enclosure clock_assessment native_result_json finalization_ticks stopwatch_frequency"

func (r *residentReader) arguments(p, a object, key ResidentKey, id, order, report string) []string {
	return []string{"-test.run=^TestSourceSignatureResidentCycles$/^" + key.Scenario + "$/^" + key.Settings + "$", "-test.count=1", "-test.v", "-resident-cycle-collect", "-resident-cycle-leaf=" + key.Scenario + "/" + key.Settings, "-resident-cycle-count=30", "-resident-cycle-report=" + report, "-resident-cycle-fixture-parent=" + r.str(p, "fixtureParent", "plan"), "-resident-cycle-production=" + r.str(a, "production", "arm"), "-resident-cycle-harness=" + residentCollector, "-resident-cycle-session=" + id, "-resident-cycle-order=" + order, "-resident-cycle-purpose=session"}
}
