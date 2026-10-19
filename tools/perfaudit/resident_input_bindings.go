package main

import (
	"bytes"
	"crypto/sha1"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"path/filepath"
	"slices"
	"strings"
)

func (r *residentReader) inputDeclaration(plan object, role string) json.RawMessage {
	for _, raw := range r.arr(plan["inputs"], "conditions.inputs", 96) {
		o := r.obj(raw, "conditions.input", "role path sha256", "")
		if r.str(o, "role", "conditions.input") == role {
			return raw
		}
	}
	r.invalid("conditions.inputs", "missing shared declaration role "+role)
	return nil
}

func (r *residentReader) inputs(p object, before, after *residentSnapshot, ref residentBatchRef, require func(string, string, string)) map[string][]byte {
	copies := map[string][]byte{}
	rows := r.arr(before.data, before.Path, 128)
	last := r.arr(after.data, after.Path, 128)
	if len(rows) != len(last) {
		r.invalid(after.Path, "input after snapshot membership changed")
	}
	type descriptor struct{ path, hash, role string }
	expected := []descriptor{{ref.Plan.Path, ref.Plan.SHA256, "plan"}}
	// Capture/launcher original paths are bound by their input inventory and
	// checked against exact pinned bytes for recorded evidence below.
	for _, k := range []string{"fixtureOwner", "fixtureInput"} {
		v := r.ref(p[k], k)
		expected = append(expected, descriptor{v.Path, v.SHA256, k})
	}
	for _, raw := range r.arr(p["inputs"], "plan.inputs", 96) {
		v := r.obj(raw, "plan.inputs[]", "role path sha256", "")
		expected = append(expected, descriptor{r.str(v, "path", "input"), r.str(v, "sha256", "input"), r.str(v, "role", "input")})
	}
	arms := r.obj(p["arms"], "plan.arms", "A B", "")
	for _, name := range []string{"A", "B"} {
		a := r.obj(arms[name], "arm", "executable sha256 production buildSource productionManifestSha256 harness overlaySha256 buildEvidence runtime", "")
		expected = append(expected, descriptor{r.str(a, "executable", "arm"), r.str(a, "sha256", "arm"), "binary-" + name})
	}
	seen := map[string]int{}
	roleCounts := map[string]int{}
	for i, raw := range rows {
		q := fmtResidentIndex(before.Path, i)
		v := r.obj(raw, q, "path role sha256 bytes", "copy")
		path, role, hash := r.str(v, "path", q), r.str(v, "role", q), r.str(v, "sha256", q)
		n := r.uint(v, "bytes", q, 256<<20)
		seen[role+"\x00"+residentPathKey(path)]++
		roleCounts[role]++
		s := r.bound(residentFileRef{path, hash}, strings.HasPrefix(role, "binary-"))
		if uint64(s.Bytes) != n {
			r.invalid(q, "input original byte count changed")
		}
		if residentWithin(ref.Directory, path) {
			r.invalid(q, "original input is inside output evidence directory")
		}
		if role != "fixtureOwner" && residentWithin(r.str(p, "fixtureParent", q), path) {
			r.invalid(q, "input inside fixture parent")
		}
		if strings.HasPrefix(role, "binary-") {
			r.absent(v, q, "copy")
		} else {
			copyPath := r.str(v, "copy", q)
			if !residentWithin(filepath.Join(ref.Directory, "frozen-inputs"), copyPath) {
				r.invalid(q, "frozen copy outside original frozen-inputs directory")
			}
			copy := r.bound(residentFileRef{copyPath, hash}, false)
			if !bytes.Equal(copy.data, s.data) {
				r.invalid(q, "frozen copy differs from original snapshot")
			}
			require(copyPath, role, path)
			copies[residentPathKey(path)] = copy.data
		}
		if i < len(last) {
			a := r.obj(last[i], fmtResidentIndex(after.Path, i), "path role sha256", "")
			for _, k := range []string{"path", "role", "sha256"} {
				r.equal(v[k], a[k], q+".after."+k)
			}
		}
		if role == "capture" || role == "launcher" {
			expectedHash := r.str(p, role+"Sha256", "plan")
			if hash != expectedHash {
				r.invalid(q, "plan/source script hash mismatch")
			}
			if r.kind == "recorded-evidence" {
				pin := residentLauncherSHA
				if role == "capture" {
					pin = residentCaptureSHA
				}
				if hash != pin {
					r.invalid(q, "source script is not the final reviewed pin")
				}
			}
		}
	}
	for _, v := range expected {
		if seen[v.role+"\x00"+residentPathKey(v.path)] != 1 {
			r.invalid(v.path, "missing/duplicate original input inventory role "+v.role)
		}
		s := r.bound(residentFileRef{v.path, v.hash}, strings.HasPrefix(v.role, "binary-"))
		_ = s
	}
	if len(rows) != len(expected)+2 {
		r.invalid(before.Path, "input inventory contains omitted/extra descriptors")
	}
	for _, role := range []string{"plan", "capture", "launcher", "fixtureOwner", "fixtureInput", "policy", "semantic-admission", "collector-interface", "dependencies", "binary-A", "binary-B"} {
		if roleCounts[role] != 1 {
			r.invalid(before.Path, "exactly one required role: "+role)
		}
	}
	for i, raw := range rows {
		v := r.obj(raw, fmtResidentIndex(before.Path, i), "path role sha256 bytes", "copy")
		role := r.str(v, "role", "input")
		hash := r.str(v, "sha256", "input")
		if role == "policy" && hash != residentPolicySHA {
			r.invalid(before.Path, "frozen resident policy hash changed")
		}
		if role == "collector-interface" && r.kind == "recorded-evidence" && hash != residentInterfaceSHA {
			r.invalid(before.Path, "final collector interface binding changed")
		}
	}
	return copies
}

type residentFrozenFixture struct {
	Scenario       string            `json:"scenario"`
	Checkers       int               `json:"checkers"`
	Builders       int               `json:"builders"`
	Inputs         map[string]string `json:"initialInputs"`
	MutationPath   string            `json:"mutationPath"`
	ProducerStates []string          `json:"producerStates"`
}

func (r *residentReader) fixture(raw json.RawMessage, seal string, key ResidentKey, p string) object {
	o := r.obj(raw, p+".fixture", "scenario checkers builders initialInputs mutationPath producerStates", "")
	r.want(o, "scenario", p, key.Scenario)
	workers := uint64(1)
	if key.Settings == "C4B4" {
		workers = 4
	}
	r.number(o, "checkers", p, workers)
	r.number(o, "builders", p, workers)
	inputs := r.obj(o["initialInputs"], p+".initialInputs", "", "")
	if len(inputs) == 0 || len(inputs) > 10000 {
		r.invalid(p, "bounded nonempty fixture inputs required")
	}
	for k := range inputs {
		r.str(inputs, k, p)
		if k == "" || strings.HasPrefix(k, "/") || strings.Contains(k, "\\") || strings.Contains(k, "..") || strings.Contains(k, ":") {
			r.invalid(p, "fixture path must be a literal relative path")
		}
	}
	r.str(o, "mutationPath", p)
	states := r.arr(o["producerStates"], p+".states", 2)
	if len(states) != 2 {
		r.invalid(p, "two producer states required")
	}
	for i, state := range states {
		r.str(object{"state": state}, "state", fmtResidentIndex(p+".states", i))
	}
	var f residentFrozenFixture
	if json.Unmarshal(raw, &f) != nil {
		r.invalid(p, "fixture typed representation invalid")
	}
	canonical, err := json.Marshal(f)
	if err != nil {
		r.invalid(p, "fixture encoding failed")
	}
	if fmt.Sprintf("%x", sha256.Sum256(canonical)) != seal {
		r.invalid(p, "Go typed fixture seal mismatch")
	}
	return o
}

var residentProductionPins = map[string]string{"B": "7015f901e2fcb8adbef3ad29909ea2518a338483", "S": "4765bd7c865dd8a1c9678f6ee905f0e2aaa4788e", "C": "401fe12c760a2f176b472b1ea8a8a56d3be22238"}
var residentBinaryPins = map[string]string{"B": "edf843d18d38dca080db08084f40f9b99061a81ca62911e3c967578a5c1cc086", "S": "2cf843cb36d48e1fd8ad62d3fe4096300568be0b2f8137b34807575752151707", "C": "beb58e39abc34ff61b0c8b5bd89f4bceb2133854f31670d72c9a0cf63074b034"}
var residentHarnessPins = map[string]string{
	"bench":               "54fc8f6c161ad444114f2c33b91f63ea2b2a931e5cfb82bff8532b8c5d6ff11f",
	"clock_native":        "d9cb50cea1c2f9f34d2b3c1c12e4b195afb243aa7a29a3d858fb77744b9a8e1a",
	"clock":               "eb657bb5d58d73c1cac9bad894a08e1cb2d8cbba8d572bb3d9aa091d59fd7ba7",
	"clock_unit":          "2c2731b99db7e41dcd09f38fc7bf98488eeef0d1e61567f97238920fb219105e",
	"clock_unsupported":   "649bb5a4885544c0a3ddee4ce054bd5c29847fb147684966c005b4a95331be34",
	"clock_windows_amd64": "d28605f7e2efd12152b0a4d6ffa5918ec2fc67be2651d154b6c428fb5b46907d",
	"cycles":              "166f421cfe1260ef2ac6c6ce1cf683ae281aa1d8c346a8d776473287c22361ab",
	"fixture":             "1f74e15bc171db69b122361d2f860dca5a70aab3a51d232849356a56206d8d9a",
}

func (r *residentReader) builds(p, arms object, copies map[string][]byte, key ResidentKey, purpose, where string) {
	reference, target := "B", "C"
	if key.Comparator == "B:S" {
		target = "S"
	}
	if key.Comparator == "S:C" {
		reference = "S"
	}
	if purpose == "calibration" {
		target = reference
	}
	inputs := r.arr(p["inputs"], where+".inputs", 96)
	lookupHash := func(hash string) []byte {
		for _, raw := range inputs {
			o := r.obj(raw, "input", "role path sha256", "")
			if r.str(o, "sha256", "input") == hash {
				return copies[residentPathKey(r.str(o, "path", "input"))]
			}
		}
		r.invalid(where, "overlay SHA256 is not bound to a listed input")
		return nil
	}
	for i, name := range []string{"A", "B"} {
		symbol := []string{reference, target}[i]
		a := r.obj(arms[name], where+".arm", "executable sha256 production buildSource productionManifestSha256 harness overlaySha256 buildEvidence runtime", "")
		r.want(a, "production", where, residentProductionPins[symbol])
		r.want(a, "harness", where, residentCollector)
		if r.kind == "recorded-evidence" {
			r.want(a, "sha256", where, residentBinaryPins[symbol])
		}
		binaryRoot := filepath.Dir(r.str(a, "executable", where))
		for _, field := range []string{"evidenceDir", "fixtureParent"} {
			root := r.str(p, field, where)
			if residentWithin(root, binaryRoot) || residentWithin(binaryRoot, root) {
				r.invalid(where, "binary root overlaps evidence/fixture root")
			}
		}
		runtime := r.obj(a["runtime"], where+".runtime", "goVersion goos goarch gomaxprocs numCPU", "")
		r.want(runtime, "goos", where, "windows")
		r.want(runtime, "goarch", where, "amd64")
		r.str(runtime, "goVersion", where)
		for _, k := range []string{"gomaxprocs", "numCPU"} {
			if r.uint(runtime, k, where, math.MaxInt32) == 0 {
				r.invalid(where, "positive runtime worker/CPU count required")
			}
		}
		buildPath := r.str(a, "buildEvidence", where)
		data := copies[residentPathKey(buildPath)]
		build := r.obj(data, buildPath, "binary sha256 buildRevision effectiveProductionRevision harnessRevision harness overlay effectiveProductionGoManifest effectiveProductionGoManifestSHA256", "sourceRevision")
		for bk, ak := range map[string]string{"binary": "executable", "sha256": "sha256", "buildRevision": "buildSource", "effectiveProductionRevision": "production", "harnessRevision": "harness", "effectiveProductionGoManifestSHA256": "productionManifestSha256"} {
			r.equal(build[bk], a[ak], buildPath+"."+bk)
		}
		if v, ok := build["sourceRevision"]; ok {
			r.equal(v, a["buildSource"], buildPath+".sourceRevision")
		}
		if !resourceGitSHA(r.str(a, "buildSource", where)) {
			r.invalid(where, "invalid actual build source revision")
		}
		if r.kind == "recorded-evidence" {
			expected := residentCollector
			if symbol == "C" {
				expected = residentProductionPins["C"]
			}
			r.want(a, "buildSource", where, expected)
		}
		lines := r.arr(build["effectiveProductionGoManifest"], buildPath+".manifest", 10000)
		text := []string{}
		for i, raw := range lines {
			s := r.str(object{"line": raw}, "line", fmtResidentIndex(buildPath+".manifest", i))
			if strings.ContainsAny(s, "\r\n") {
				r.invalid(buildPath, "literal production manifest lines required")
			}
			text = append(text, s)
		}
		manifestHash := fmt.Sprintf("%x", sha256.Sum256([]byte(strings.Join(text, "\n")+"\n")))
		r.want(build, "effectiveProductionGoManifestSHA256", buildPath, manifestHash)
		if len(lines) == 0 {
			r.invalid(buildPath, "complete effective production manifest required")
		}
		overlayData := lookupHash(r.str(a, "overlaySha256", where))
		r.equal(build["overlay"], overlayData, buildPath+".overlay")
		overlay := r.obj(build["overlay"], buildPath+".overlay", "Replace", "")
		replace := r.obj(overlay["Replace"], buildPath+".Replace", "", "")
		harness := r.obj(build["harness"], buildPath+".harness", "", "")
		if len(harness) != 8 {
			r.invalid(buildPath, "exact complete eight-file harness required")
		}
		for short, pin := range residentHarnessPins {
			logical := "internal/compile/source_signature_resident_" + short + "_test.go"
			source := r.obj(harness[logical], buildPath+".harness."+logical, "gitBlob sha256", "")
			hash := r.str(source, "sha256", buildPath)
			if r.kind == "recorded-evidence" && hash != pin {
				r.invalid(buildPath, "collector source does not match final reviewed helper")
			}
			matches := []string{}
			for original, value := range replace {
				if strings.HasSuffix(strings.ReplaceAll(original, "\\", "/"), "/"+logical) {
					var path string
					if json.Unmarshal(value, &path) != nil {
						r.invalid(buildPath, "overlay replacement must be string")
					}
					matches = append(matches, path)
				}
			}
			if len(matches) != 1 {
				r.invalid(buildPath, "each collector source must appear once in overlay")
				continue
			}
			sourceData, ok := copies[residentPathKey(matches[0])]
			if !ok {
				r.invalid(buildPath, "overlay source bytes lack frozen input copy")
				continue
			}
			actual := fmt.Sprintf("%x", sha256.Sum256(sourceData))
			if actual != hash {
				r.invalid(buildPath, "overlay source SHA256 mismatch")
			}
			blob := sha1.New()
			_, _ = fmt.Fprintf(blob, "blob %d%c", len(sourceData), 0)
			_, _ = blob.Write(sourceData)
			if fmt.Sprintf("%x", blob.Sum(nil)) != r.str(source, "gitBlob", buildPath) {
				r.invalid(buildPath, "overlay source Git blob identity mismatch")
			}
		}
	}
	a := r.obj(arms["A"], "A", "", "")
	b := r.obj(arms["B"], "B", "", "")
	if purpose == "calibration" || bytes.Equal(a["sha256"], b["sha256"]) {
		for _, k := range []string{"production", "buildSource", "productionManifestSha256", "harness", "runtime"} {
			r.equal(a[k], b[k], where+".identical_binary."+k)
		}
	}
	if purpose == "calibration" {
		r.equal(a["executable"], b["executable"], where+".calibration_path")
		r.equal(a["sha256"], b["sha256"], where+".calibration_sha")
	}
	roots := r.arr(p["protectedRoots"], where+".protectedRoots", 256)
	for _, raw := range roots {
		var root string
		if json.Unmarshal(raw, &root) != nil || !resourceAbsolute(root) {
			r.invalid(where, "absolute protected root required")
		}
		for _, k := range []string{"evidenceDir", "fixtureParent"} {
			v := r.str(p, k, where)
			if residentWithin(root, v) || residentWithin(v, root) {
				r.invalid(where, "protected-root overlap")
			}
		}
	}
	parent, dir := r.str(p, "fixtureParent", where), r.str(p, "evidenceDir", where)
	if residentWithin(parent, dir) || residentWithin(dir, parent) {
		r.invalid(where, "fixture/evidence roots overlap")
	}
	owner := r.ref(p["fixtureOwner"], where+".fixtureOwner")
	r.pathEqual(owner.Path, filepath.Join(parent, ".resident-owner.json"), where)
	// Parsed semantic/dependency declarations bind bytes, not their physical truth.
	for _, raw := range inputs {
		o := r.obj(raw, "input", "role path sha256", "")
		if slices.Contains([]string{"semantic-admission", "dependencies"}, r.str(o, "role", "input")) {
			r.obj(copies[residentPathKey(r.str(o, "path", "input"))], where+".trust_input", "", "")
		}
	}
}

func (r *residentReader) report(data []byte, p, a, entry, fixture object, key ResidentKey, id, order, where string) object {
	o := r.obj(data, where+".report", "schema declaredOptions identityBinding classification boundary excluded fixture fixtureSha256 runtime clock expectedCycles attemptedCycles successfulCycles records status success stage fixtureReserved cleanupJoined", "error firstFailure")
	r.number(o, "schema", where, 2)
	for _, k := range []string{"expectedCycles", "attemptedCycles", "successfulCycles"} {
		r.number(o, k, where, 30)
	}
	for _, k := range []string{"success", "fixtureReserved", "cleanupJoined"} {
		r.flag(o, k, where, true)
	}
	for _, k := range []string{"status", "stage"} {
		r.want(o, k, where, "complete")
	}
	r.want(o, "classification", where, "session")
	r.want(o, "boundary", where, "Invalidate (except EmptyDrain) -> Drain including required successful persistence -> WatchSets")
	r.noError(o, where, "error", "firstFailure")
	if r.str(o, "identityBinding", where) == "" || r.str(o, "excluded", where) == "" {
		r.invalid(where, "missing boundary/source limitation labels")
	}
	r.equal(o["fixture"], mustJSON(fixture), where+".fixture")
	var f residentFrozenFixture
	_ = json.Unmarshal(mustJSON(fixture), &f)
	canonical, _ := json.Marshal(f)
	r.want(o, "fixtureSha256", where, fmt.Sprintf("%x", sha256.Sum256(canonical)))
	opts := r.obj(o["declaredOptions"], where+".options", "Collect Leaf Report Parent Production Harness Session Order Purpose Count TestRun TestCount TestCPU TestBench Profile", "")
	for k, v := range map[string]string{"Leaf": key.Scenario + "/" + key.Settings, "Report": r.str(entry, "report_path", where), "Parent": r.str(p, "fixtureParent", where), "Production": r.str(a, "production", where), "Harness": residentCollector, "Session": id, "Order": order, "Purpose": "session", "TestRun": "^TestSourceSignatureResidentCycles$/^" + key.Scenario + "$/^" + key.Settings + "$", "TestCount": "1", "TestCPU": "", "TestBench": ""} {
		r.want(opts, k, where, v)
	}
	r.flag(opts, "Collect", where, true)
	r.flag(opts, "Profile", where, false)
	r.number(opts, "Count", where, 30)
	runtime := r.obj(o["runtime"], where+".runtime", "goVersion goos goarch gomaxprocs numCPU inheritedEnv", "")
	declared := r.obj(a["runtime"], where+".arm.runtime", "goVersion goos goarch gomaxprocs numCPU", "")
	for _, k := range []string{"goVersion", "goos", "goarch", "gomaxprocs", "numCPU"} {
		r.equal(runtime[k], declared[k], where+".runtime."+k)
	}
	env := r.obj(runtime["inheritedEnv"], where+".inheritedEnv", "GOGC GOMAXPROCS GOMEMLIMIT GODEBUG", "")
	for _, k := range []string{"GOGC", "GOMAXPROCS", "GOMEMLIMIT", "GODEBUG"} {
		r.str(env, k, where)
	}
	records := r.arr(o["records"], where+".records", 30)
	if len(records) != 30 {
		r.invalid(where, "exactly thirty cycles required")
	}
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
	sum := new(big.Int)
	bound := new(big.Int).Mul(new(big.Int).SetUint64(r.uint(p, "timeoutMs", where, 86400000)), big.NewInt(1000000))
	workers := uint64(1)
	if key.Settings == "C4B4" {
		workers = 4
	}
	for i, raw := range records {
		rec := r.obj(raw, where+".record", "ordinal scenario checkers builders inputState mutation accepted operationStatus start end conversion", "durationNs error")
		r.number(rec, "ordinal", where, uint64(i+1))
		r.want(rec, "scenario", where, key.Scenario)
		r.number(rec, "checkers", where, workers)
		r.number(rec, "builders", where, workers)
		state := uint64((i + 1) % 2)
		if mutation == "empty-drain" || mutation == "no-mutation-invalidation" {
			state = 0
		}
		r.number(rec, "inputState", where, state)
		r.want(rec, "mutation", where, mutation)
		ns := r.uint(rec, "durationNs", where, math.MaxInt64)
		sum.Add(sum, new(big.Int).SetUint64(ns))
	}
	if sum.Cmp(bound) > 0 {
		r.invalid(where, "exact sequential nanosecond sum exceeds declared invocation timeout")
	}
	return o
}
func mustJSON(v any) []byte { data, _ := json.Marshal(v); return data }
