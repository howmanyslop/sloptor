package main

import (
	"crypto/sha1"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

type residentProtocolFixture struct {
	root string
	t    *testing.T
	plan residentReaderPlan
	raw  map[string]map[string]any
	refs []residentBatchRef
}

func residentProtocolRoot(t *testing.T) string {
	t.Helper()
	if p := os.Getenv("RESIDENT_READER_ARTIFACT_DIR"); p != "" {
		if err := os.MkdirAll(p, 0o755); err != nil {
			t.Fatal(err)
		}
		name := strings.Map(func(c rune) rune {
			if c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_' {
				return c
			}
			return '_'
		}, t.Name())
		dir, err := os.MkdirTemp(p, name+"-")
		if err != nil {
			t.Fatal(err)
		}
		return dir
	}
	return t.TempDir()
}
func residentTestHash(data []byte) string { return fmt.Sprintf("%x", sha256.Sum256(data)) }
func (f *residentProtocolFixture) write(path string, v any) residentFileRef {
	f.t.Helper()
	data, ok := v.([]byte)
	if !ok {
		var err error
		data, err = json.Marshal(v)
		if err != nil {
			f.t.Fatal(err)
		}
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		f.t.Fatal(err)
	}
	if err := os.WriteFile(path, data, 0o600); err != nil {
		f.t.Fatal(err)
	}
	return residentFileRef{path, residentTestHash(data)}
}

func (f *residentProtocolFixture) read(path string) map[string]any {
	f.t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		f.t.Fatal(err)
	}
	decoder := json.NewDecoder(strings.NewReader(string(data)))
	decoder.UseNumber()
	v := map[string]any{}
	if err := decoder.Decode(&v); err != nil {
		f.t.Fatal(err)
	}
	return v
}

func newResidentProtocolFixture(t *testing.T, key ResidentKey) *residentProtocolFixture {
	f := &residentProtocolFixture{root: residentProtocolRoot(t), t: t, raw: map[string]map[string]any{}}
	f.plan = residentReaderPlan{Schema: 1, EvidenceKind: "invented-protocol", PolicySHA256: residentPolicySHA, Campaigns: []residentCampaignInput{{Key: key, Calibration: []residentBatchRef{}, Comparison: []residentBatchRef{}}}}
	return f
}

func (f *residentProtocolFixture) add(stage string, chronology int, ratioNumerator uint64) residentBatchRef {
	t := f.t
	t.Helper()
	root := f.root
	key := f.plan.Campaigns[0].Key
	id := fmt.Sprintf("invented-%s-%d", stage, chronology)
	dir := filepath.Join(root, "evidence", id)
	input := filepath.Join(root, "inputs")
	parent := filepath.Join(root, "fixture")
	work := filepath.Join(root, "working")
	count := 20
	if strings.Contains(stage, "followup") {
		count = 40
	}
	for _, p := range []string{dir, input, parent, work} {
		if err := os.MkdirAll(p, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	type desc = map[string]any
	descriptors := []any{}
	addInput := func(role, path string, data any) residentFileRef {
		ref := f.write(path, data)
		descriptors = append(descriptors, desc{"role": role, "path": path, "sha256": ref.SHA256})
		return ref
	}
	addInput("policy", filepath.Join(input, "policy.txt"), residentTestPolicyBytes(t))
	addInput("semantic-admission", filepath.Join(input, "semantic.json"), desc{"kind": "INVENTED SYNTHETIC PROTOCOL semantic assertions"})
	addInput("collector-interface", filepath.Join(input, "interface.txt"), []byte("INVENTED SYNTHETIC PROTOCOL interface proof"))
	addInput("dependencies", filepath.Join(input, "dependencies.json"), desc{"kind": "INVENTED SYNTHETIC PROTOCOL dependencies"})
	owner := f.write(filepath.Join(parent, ".resident-owner.json"), desc{"owner": "INVENTED SYNTHETIC PROTOCOL"})
	fixture := residentFrozenFixture{Scenario: key.Scenario, Checkers: 1, Builders: 1, Inputs: map[string]string{"core/src/producer.ts": "export const x = 1;"}, MutationPath: "core/src/producer.ts", ProducerStates: []string{"export const x = 1;", "export const x = 2;"}}
	if key.Settings == "C4B4" {
		fixture.Checkers, fixture.Builders = 4, 4
	}
	fixtureHash := residentTestHash(mustJSON(fixture))
	fixtureRef := f.write(filepath.Join(input, "fixture.json"), desc{"fixture": fixture, "fixtureSha256": fixtureHash})
	launcher := f.write(filepath.Join(input, "launcher.ps1"), []byte("# INVENTED SYNTHETIC PROTOCOL launcher descriptor"))
	capture := f.write(filepath.Join(input, "capture.ps1"), []byte("# INVENTED SYNTHETIC PROTOCOL capture descriptor"))
	harness := desc{}
	replace := map[string]string{}
	for name := range residentHarnessPins {
		logical := "internal/compile/source_signature_resident_" + name + "_test.go"
		path := filepath.Join(input, name+".go")
		data := []byte("// INVENTED SYNTHETIC PROTOCOL " + name + "\n")
		ref := addInput("collector-"+name, path, data)
		blob := sha1.Sum(append([]byte(fmt.Sprintf("blob %d%c", len(data), 0)), data...))
		harness[logical] = desc{"sha256": ref.SHA256, "gitBlob": fmt.Sprintf("%x", blob)}
		replace[filepath.Join(root, "source", filepath.FromSlash(logical))] = path
	}
	overlay := desc{"Replace": replace}
	overlayRef := addInput("overlay", filepath.Join(input, "overlay.json"), overlay)
	runtime := desc{"goVersion": "go1.26.9", "goos": "windows", "goarch": "amd64", "gomaxprocs": 1, "numCPU": 1}
	refSymbol, target := "B", "C"
	if key.Comparator == "B:S" {
		target = "S"
	}
	if key.Comparator == "S:C" {
		refSymbol = "S"
	}
	calibration := strings.HasPrefix(stage, "calibration-")
	if calibration {
		target = refSymbol
	}
	arms := desc{}
	for i, name := range []string{"A", "B"} {
		symbol := []string{refSymbol, target}[i]
		exe := f.write(filepath.Join(root, "binary-"+symbol, "invented.exe"), []byte("INVENTED SYNTHETIC PROTOCOL executable "+symbol))
		lines := []string{"INVENTED SYNTHETIC PROTOCOL production graph " + symbol}
		mh := residentTestHash([]byte(strings.Join(lines, "\n") + "\n"))
		buildPath := filepath.Join(input, "build-"+symbol+".json")
		build := desc{"binary": exe.Path, "sha256": exe.SHA256, "buildRevision": residentCollector, "effectiveProductionRevision": residentProductionPins[symbol], "harnessRevision": residentCollector, "harness": harness, "overlay": overlay, "effectiveProductionGoManifest": lines, "effectiveProductionGoManifestSHA256": mh}
		found := false
		for _, v := range descriptors {
			if v.(desc)["path"] == buildPath {
				found = true
			}
		}
		if !found {
			addInput("build-"+symbol, buildPath, build)
		}
		arms[name] = desc{"executable": exe.Path, "sha256": exe.SHA256, "production": residentProductionPins[symbol], "buildSource": residentCollector, "productionManifestSha256": mh, "harness": residentCollector, "overlaySha256": overlayRef.SHA256, "buildEvidence": buildPath, "runtime": runtime}
	}
	order := []string{}
	for i := 0; i < count; i++ {
		order = append(order, []string{"A", "B", "B", "A"}[i%4])
	}
	comparator := key.Comparator
	if calibration {
		comparator = refSymbol + ":" + refSymbol
	}
	p := desc{"schema": 1, "batch": id, "mode": "session", "stage": stage, "comparator": comparator, "semanticLabel": "INVENTED SYNTHETIC PROTOCOL population assertions", "pairs": count / 2, "cycles": 30, "order": order, "fixtureParent": parent, "fixtureOwner": owner, "evidenceDir": dir, "workingDirectory": work, "timeoutMs": 240000, "captureBytes": residentFileLimit, "reportBytes": residentFileLimit, "captureSha256": capture.SHA256, "launcherSha256": launcher.SHA256, "fixtureInput": fixtureRef, "arms": arms, "inputs": descriptors, "protectedRoots": []string{filepath.Join(root, "source")}, "provenance": "INVENTED SYNTHETIC PROTOCOL; no compiler/collector process ran"}
	planRef := f.write(filepath.Join(root, "plans", id+".json"), p)
	f.write(filepath.Join(dir, "plan.raw.json"), mustReadTest(t, planRef.Path))
	ref := residentBatchRef{dir, planRef, chronology}
	f.refs = append(f.refs, ref)
	f.raw[id] = p
	support := desc{"status": "supported", "domain": "same-host-windows-qpc", "counter_api": "QueryPerformanceCounter", "frequency_api": "QueryPerformanceFrequency", "endpoint_observation": "managed return; native BOOL and lastError are not exposed", "high_resolution": true, "frequency_hz": 10000000, "framework": "INVENTED SYNTHETIC PROTOCOL .NET", "os": "INVENTED SYNTHETIC PROTOCOL Windows", "architecture": "X64", "powershell": "7.6.7", "corelib_path": filepath.Join(input, "invented-corelib.dll"), "corelib_sha256": "8dfc31dfb4783a981179036f492d2690303e6e6571c7f260901b1b9ebc9e64ae", "corelib_informational_version": "INVENTED SYNTHETIC PROTOCOL", "timestamp_il": "1200E028BD00000626062A", "frequency_il": "1200E028BE00000626062A", "authority": "INVENTED SYNTHETIC PROTOCOL; not observed physical support"}
	env := desc{}
	for _, k := range []string{"GOGC", "GOMAXPROCS", "GOMEMLIMIT", "GODEBUG"} {
		env[k] = residentTestHash(nil)
	}
	host := desc{"os": "INVENTED SYNTHETIC PROTOCOL Windows", "architecture": "X64", "processors": 1, "powershell": "7.6.7", "power_scheme": "INVENTED SYNTHETIC PROTOCOL", "environment": env, "qpc_support": support}
	f.write(filepath.Join(dir, "host.before.json"), host)
	f.write(filepath.Join(dir, "host.after.json"), host)
	files := []any{desc{"path": planRef.Path, "role": "plan", "sha256": planRef.SHA256}, desc{"path": capture.Path, "role": "capture", "sha256": capture.SHA256}, desc{"path": launcher.Path, "role": "launcher", "sha256": launcher.SHA256}, desc{"path": owner.Path, "role": "fixtureOwner", "sha256": owner.SHA256}, desc{"path": fixtureRef.Path, "role": "fixtureInput", "sha256": fixtureRef.SHA256}}
	files = append(files, descriptors...)
	for _, name := range []string{"A", "B"} {
		a := arms[name].(desc)
		files = append(files, desc{"path": a["executable"], "role": "binary-" + name, "sha256": a["sha256"]})
	}
	before, after := []any{}, []any{}
	retained := []any{}
	bind := func(path, role string, original any) {
		data := mustReadTest(t, path)
		identity := desc{"path": path, "sha256": residentTestHash(data), "bytes": len(data)}
		retained = append(retained, desc{"path": path, "role": role, "original": original, "expected_sha256": identity["sha256"], "expected_bytes": len(data), "observed": identity, "retention_error": nil, "error": nil})
	}
	bind(filepath.Join(dir, "plan.raw.json"), "plan.raw", planRef.Path)
	for i, value := range files {
		v := value.(desc)
		path, role := v["path"].(string), v["role"].(string)
		data := mustReadTest(t, path)
		row := desc{"path": path, "role": role, "sha256": v["sha256"], "bytes": len(data)}
		if !strings.HasPrefix(role, "binary-") {
			copyPath := filepath.Join(dir, "frozen-inputs", fmt.Sprintf("%03d.raw", i))
			f.write(copyPath, data)
			row["copy"] = copyPath
			bind(copyPath, role, path)
		}
		before = append(before, row)
		after = append(after, desc{"path": path, "role": role, "sha256": v["sha256"]})
	}
	f.write(filepath.Join(dir, "inputs.before.json"), before)
	f.write(filepath.Join(dir, "inputs.after.json"), after)
	started := desc{"schema": 1, "batch": id, "status": "incomplete", "mode": "session", "stage": stage, "planned": count, "finished": 0, "completed": 0, "failed": 0, "incomplete_unmeasured": 0, "error": nil, "promotion_ready": false}
	f.write(filepath.Join(dir, "batch.started.json"), started)
	lines := []byte{}
	for i := 0; i < count; i++ {
		name := order[i]
		a := arms[name].(desc)
		session := fmt.Sprintf("%s-%04d", id, i+1)
		pair := i/2 + 1
		pairOrder := "AB"
		if pair%2 == 0 {
			pairOrder = "BA"
		}
		orderLabel := fmt.Sprintf("batch=%s;block=%d;pair=%d;order=%s;position=%d;arm=%s", id, i/4+1, pair, pairOrder, i+1, name)
		rp := filepath.Join(dir, session+".report.json")
		entry := desc{"event": "started", "schema": 1, "session": session, "batch": id, "mode": "session", "stage": stage, "comparator": comparator, "semantic_label": p["semanticLabel"], "arm": name, "position": i + 1, "pair": pair, "block": i/4 + 1, "pair_order": pairOrder, "order": orderLabel, "at_utc": time.Date(2026, 10, 18, 12, chronology*3, i*2, 0, time.UTC).Format(time.RFC3339Nano), "status": "incomplete", "executable": a["executable"], "executable_sha256": a["sha256"], "production": a["production"], "harness": residentCollector, "working_directory": work, "report_path": rp, "stdout_path": filepath.Join(dir, session+".stdout.txt"), "stderr_path": filepath.Join(dir, session+".stderr.txt"), "fixture_leaf": filepath.Join(parent, key.Scenario, key.Settings), "resource_accounting_format": resourceFormat, "resource_units": desc{"cpu": "100ns ticks", "memory": "simultaneous peak job committed bytes", "io": "OS IO operations/bytes; all six class-8 counters"}, "resource_scope": "whole owned job: startup/setup/warm/mutation/cycles/report/cleanup; outer launcher excluded", "build_source": a["buildSource"], "production_manifest_sha256": a["productionManifestSha256"]}
		// Literal argv from the reviewed bcdf launcher contract. The fixture
		// intentionally does not call the reader's expected-argv constructor.
		entry["arguments"] = []string{
			"-test.run=^TestSourceSignatureResidentCycles$/^" + key.Scenario + "$/^" + key.Settings + "$", "-test.count=1", "-test.v",
			"-resident-cycle-collect", "-resident-cycle-leaf=" + key.Scenario + "/" + key.Settings, "-resident-cycle-count=30",
			"-resident-cycle-report=" + rp, "-resident-cycle-fixture-parent=" + parent, "-resident-cycle-production=" + a["production"].(string),
			"-resident-cycle-harness=" + residentCollector, "-resident-cycle-session=" + session, "-resident-cycle-order=" + orderLabel, "-resident-cycle-purpose=session",
		}
		lines = append(append(lines, mustJSON(entry)...), '\n')
		ns := uint64(10000000)
		if name == "B" && !calibration {
			ns = 100000 * ratioNumerator
		}
		report, assessment := residentInventedRawReport(key, fixture, fixtureHash, a, p, entry, support, ns)
		// Invent a serial same-host QPC schedule. The first session at ordinal
		// one retains the original zero origin for endpoint-boundary tests. These
		// generous synthetic spans are consistency fixtures, not measurements.
		qpcStart := int64(chronology-1)*1000000000000 + int64(i)*2000000000
		for _, raw := range report["records"].([]any) {
			for _, endpoint := range []string{"start", "end"} {
				ep := raw.(desc)[endpoint].(desc)
				ep["ticks"] = ep["ticks"].(int64) + qpcStart
			}
		}
		reportRef := f.write(rp, report)
		sp := strings.TrimSuffix(rp, ".json") + ".snapshot.raw"
		f.write(sp, mustReadTest(t, rp))
		bind(rp, "report", rp)
		bind(sp, "report", rp)
		stdout := []byte("INVENTED SYNTHETIC PROTOCOL stdout\n")
		f.write(entry["stdout_path"].(string), stdout)
		f.write(entry["stderr_path"].(string), []byte{})
		native := residentInventedNative(len(stdout))
		entry["event"], entry["status"], entry["at_utc"] = "finished", "completed", time.Date(2026, 10, 18, 12, chronology*3, i*2+1, 0, time.UTC).Format(time.RFC3339Nano)
		entry["validation_completed"], entry["owner_disposed"], entry["fixture_absent"] = true, true, true
		entry["report"] = desc{"path": rp, "snapshot_path": sp, "sha256": reportRef.SHA256, "bytes": len(mustReadTest(t, rp))}
		entry["result"], entry["native_result_json"] = native, string(mustJSON(native))
		entry["reasons"] = []any{}
		entry["clock_assessment"] = assessment
		entry["enclosure"] = desc{"schema": 1, "format": "resident-qpc-enclosure-v1", "support": support, "frequency_hz": 10000000, "scope": "immediately around unchanged Invoke-ProcessWithTimeout call; includes native launch/wait/drains and post-wall helper work; excludes report validation/disposal/retention; consistency only, not a scoring estimand", "call_status": "returned", "start": desc{"status": "managed-return", "ticks": qpcStart}, "end": desc{"status": "managed-return", "ticks": qpcStart + 1000000000}}
		entry["finalization_ticks"], entry["stopwatch_frequency"] = 10, 10000000
		lines = append(append(lines, mustJSON(entry)...), '\n')
	}
	f.write(filepath.Join(dir, "journal.ndjson"), lines)
	for _, name := range []string{"batch.started.json", "host.before.json", "inputs.before.json", "inputs.after.json", "host.after.json"} {
		bind(filepath.Join(dir, name), "metadata", nil)
	}
	journalPath := filepath.Join(dir, "journal.ndjson")
	manifest := desc{"schema": 1, "format": "resident-evidence-v1", "valid": true, "journal": desc{"path": journalPath, "bytes": len(lines), "sha256": residentTestHash(lines)}, "retained": retained, "errors": []any{}}
	manifestRef := f.write(filepath.Join(dir, "evidence.manifest.json"), manifest)
	finished := desc{}
	for k, v := range started {
		finished[k] = v
	}
	finished["status"], finished["finished"], finished["completed"] = "completed", count, count
	finished["evidence_manifest"] = desc{"path": manifestRef.Path, "sha256": manifestRef.SHA256, "bytes": len(mustReadTest(t, manifestRef.Path))}
	f.write(filepath.Join(dir, "batch.finished.json"), finished)
	if calibration {
		f.plan.Campaigns[0].Calibration = append(f.plan.Campaigns[0].Calibration, ref)
	} else {
		f.plan.Campaigns[0].Comparison = append(f.plan.Campaigns[0].Comparison, ref)
	}
	return ref
}

func mustReadTest(t *testing.T, p string) []byte {
	t.Helper()
	data, e := os.ReadFile(p)
	if e != nil {
		t.Fatal(e)
	}
	return data
}
func jsonObjectTest(v any) object { var o object; _ = json.Unmarshal(mustJSON(v), &o); return o }

func residentTestPolicyBytes(t *testing.T) []byte {
	t.Helper()
	path := os.Getenv("RESIDENT_READER_POLICY")
	if path == "" {
		return residentEmbeddedPolicy(t)
	}
	data := mustReadTest(t, path)
	if residentTestHash(data) != residentPolicySHA {
		t.Fatal("frozen policy identity mismatch")
	}
	return data
}

func residentInventedNative(stdout int) map[string]any {
	d := func(values map[string]any) map[string]any {
		return map[string]any{"Status": "available", "Error": nil, "Values": values}
	}
	job := map[string]any{"UserTicks100ns": 625001, "KernelTicks100ns": 0, "TotalProcesses": 1, "ActiveProcesses": 0, "LimitTerminatedProcesses": 0, "TotalPageFaults": 1, "ReadOperations": 0, "WriteOperations": 0, "OtherOperations": 0, "ReadBytes": 0, "WriteBytes": 0, "OtherBytes": 0, "StructureBytes": 96}
	cpu := map[string]any{"UserTicks100ns": 625001, "KernelTicks100ns": 0}
	other := map[string]any{"UserTicks100ns": 0, "KernelTicks100ns": 0}
	resources := map[string]any{"Schema": 1, "Status": "available", "Completion": "complete", "CompletionError": nil, "JobAccounting": d(job), "JobMemory": d(map[string]any{"PeakJobMemoryBytes": 1024, "PeakProcessMemoryBytes": 1024, "StructureBytes": 144}), "RootCpu": d(cpu), "OtherOwnedCpu": d(other), "QueryElapsedMS": 0}
	return map[string]any{"DurationMS": 1000, "ElapsedMS": 1000, "ExitCode": 0, "Status": "completed", "Error": nil, "EvidencePersistenceMS": 0, "CaptureLimitBytes": residentFileLimit, "ResourceAccounting": resources, "StdoutBytes": stdout, "StderrBytes": 0, "CaptureOverflow": false, "CaptureSpoolWriteMS": 0}
}

func residentInventedRawReport(key ResidentKey, fixture residentFrozenFixture, seal string, a, p, entry, support map[string]any, ns uint64) (map[string]any, map[string]any) {
	type d = map[string]any
	ep := func(ticks int64) d { return d{"status": "ok", "ticks": ticks, "bool": 1, "lastError": uint64(0)} }
	clock := d{"backend": "windows-qpc-syscalln-v1", "counterApi": "QueryPerformanceCounter", "frequencyApi": "QueryPerformanceFrequency", "counterEncoding": "signed-int64", "conversion": "floor-ticks-1e9-over-frequency-v1", "remainderRule": "durationNs + remainder/frequencyHz nanoseconds; 0 <= remainder < frequencyHz", "scope": "elapsed interval including blocking/scheduling; not UTC or process CPU; access cost retained; frequency is not calibrated precision", "support": "supported", "loadStatus": "ok", "counterResolveStatus": "ok", "frequencyResolveStatus": "ok", "initializationStatus": "ok", "frequencyHz": 10000000, "initialFrequency": ep(10000000), "finalFrequency": ep(10000000), "verificationStatus": "ok", "frequencyVerified": true}
	mutation := "const-enum-api"
	switch {
	case strings.HasPrefix(key.Scenario, "EmptyDrain"):
		mutation = "empty-drain"
	case strings.Contains(key.Scenario, "Invalidate"):
		mutation = "no-mutation-invalidation"
	case strings.HasPrefix(key.Scenario, "Rewrite"):
		mutation = "identical-rewrite-source-clock"
	case strings.Contains(key.Scenario, "Body"):
		mutation = "annotated-body"
	case key.Scenario == "Chain32":
		mutation = "inferred-export-api"
	}
	records := []any{}
	spans := []int64{}
	delta := int64(ns / 100)
	for i := 0; i < 30; i++ {
		start := int64(1000) + int64(i)*(delta+10)
		state := (i + 1) % 2
		if mutation == "empty-drain" || mutation == "no-mutation-invalidation" {
			state = 0
		}
		records = append(records, d{"ordinal": i + 1, "scenario": key.Scenario, "checkers": fixture.Checkers, "builders": fixture.Builders, "inputState": state, "mutation": mutation, "accepted": true, "durationNs": ns, "operationStatus": "ok", "start": ep(start), "end": ep(start + delta), "conversion": d{"status": "ok", "deltaTicks": delta, "durationNs": ns, "remainder": 0}})
		spans = append(spans, delta)
	}
	runtime := d{}
	for k, v := range a["runtime"].(d) {
		runtime[k] = v
	}
	runtime["inheritedEnv"] = d{"GOGC": "", "GOMAXPROCS": "", "GOMEMLIMIT": "", "GODEBUG": ""}
	options := d{"Collect": true, "Leaf": key.Scenario + "/" + key.Settings, "Report": entry["report_path"], "Parent": p["fixtureParent"], "Production": a["production"], "Harness": residentCollector, "Session": entry["session"], "Order": entry["order"], "Purpose": "session", "Count": 30, "TestRun": "^TestSourceSignatureResidentCycles$/^" + key.Scenario + "$/^" + key.Settings + "$", "TestCount": "1", "TestCPU": "", "TestBench": "", "Profile": false}
	report := d{"schema": 2, "declaredOptions": options, "identityBinding": "INVENTED SYNTHETIC PROTOCOL source assertions", "classification": "session", "boundary": "Invalidate (except EmptyDrain) -> Drain including required successful persistence -> WatchSets", "excluded": "INVENTED SYNTHETIC PROTOCOL; no physical operation measured", "fixture": fixture, "fixtureSha256": seal, "runtime": runtime, "clock": clock, "expectedCycles": 30, "attemptedCycles": 30, "successfulCycles": 30, "records": records, "status": "complete", "success": true, "stage": "complete", "fixtureReserved": true, "cleanupJoined": true}
	assessment := d{"status": "consistent", "score_authorized": false, "suitability": "requires-independent-precision-review", "calculations_validated": true, "frequency_hz": 10000000, "outer_span_ticks": 1000000000, "sum_ticks": delta * 30, "cycle_count": 30, "allowance_ticks": 31, "aggregate_bound_ticks": 1000000031, "endpoint_order_tolerance_ticks": 1, "represented_tick_spans": spans, "zero_tick_count": 0, "one_tick_count": 0, "two_tick_count": 0}
	return report, assessment
}

func (f *residentProtocolFixture) authorize() {
	rows := []any{}
	for _, ref := range f.refs {
		hb := mustReadTest(f.t, filepath.Join(ref.Directory, "host.before.json"))
		plan := f.read(ref.Plan.Path)
		fixture := f.read(plan["fixtureInput"].(map[string]any)["path"].(string))
		rows = append(rows, map[string]any{"plan_path": ref.Plan.Path, "plan_sha256": ref.Plan.SHA256, "batch": filepath.Base(ref.Directory), "chronology": ref.Chronology, "host_sha256": residentTestHash(hb), "fixture_sha256": fixture["fixtureSha256"], "authorized_utc": "2026-10-18T11:30:00Z"})
	}
	assertions := map[string]string{}
	for _, name := range residentPrerequisiteNames {
		assertions[name] = "admitted"
	}
	ref := f.write(filepath.Join(f.root, "invented-admission.json"), map[string]any{"schema": 1, "kind": "invented-root-assertions-v1", "policy_sha256": residentPolicySHA, "declared_utc": "2026-10-18T11:00:00Z", "prerequisites": assertions, "batches": rows})
	f.plan.Admission = &ref
}

func TestResidentReaderRawPositive(t *testing.T) {
	f := newResidentProtocolFixture(t, residentTestKey())
	f.add("calibration-initial", 1, 100)
	f.add("initial", 2, 80)
	f.authorize()
	path := f.write(filepath.Join(f.root, "reader.json"), f.plan)
	reader := newResidentReader()
	report := reader.reconstruct(path.Path)
	reader.decide(&report)
	if report.Outcome != criterionMet {
		for _, i := range report.Issues {
			t.Logf("%+v", i)
		}
		t.Fatalf("invented raw positive failed: %s", report.Outcome)
	}
	if report.PromotionReady || len(report.Batches) != 2 || report.Batches[1].Reconstructed.Blocks[0].Sessions[1].Cycles[0].Nanoseconds != 8000000 {
		t.Fatal("raw reconstruction/population changed")
	}
}
