package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"time"
)

const (
	residentLauncherSHA    = "319031bc2a089609293c10bbd5091842204b73075aa300ea126c1e8f209d0dc5"
	residentCaptureSHA     = "ffceeefd821c409c2081253bce6af2986b00e16a3c780422c7a42b722795eb21"
	residentCollector      = "518a2f4b0bc3ea1cf021ad4ba281e210abc64fd9"
	residentInterfaceSHA   = "243194446103a787e3ef7cbd8655738bd6a33e8bfbd67b036ccfb4f10d77b6f3"
	residentFileLimit      = 16 << 20
	residentAggregateLimit = 512 << 20
)

type residentFileRef struct {
	Path   string `json:"path"`
	SHA256 string `json:"sha256"`
}
type residentBatchRef struct {
	Directory  string          `json:"directory"`
	Plan       residentFileRef `json:"plan"`
	Chronology int             `json:"chronology"`
}
type residentCampaignInput struct {
	Key         ResidentKey        `json:"key"`
	Calibration []residentBatchRef `json:"calibration"`
	Comparison  []residentBatchRef `json:"comparison"`
}
type residentReaderPlan struct {
	Schema       int                     `json:"schema"`
	EvidenceKind string                  `json:"evidence_kind"`
	PolicySHA256 string                  `json:"policy_sha256"`
	Admission    *residentFileRef        `json:"admission,omitempty"`
	Campaigns    []residentCampaignInput `json:"campaigns"`
}
type residentSnapshot struct {
	Path   string `json:"path"`
	SHA256 string `json:"sha256"`
	Bytes  int    `json:"bytes"`
	Raw    []byte `json:"raw_base64,omitempty"`
	Binary bool   `json:"binary,omitempty"`
	Error  string `json:"error,omitempty"`
	data   []byte
}

// One bounded immutable snapshot per original path, including failed reads.
// Binary bytes are held privately during analysis; reports retain their identity.
type residentReader struct {
	validation
	files map[string]*residentSnapshot
	total int
	kind  string
}

func newResidentReader() *residentReader {
	return &residentReader{files: map[string]*residentSnapshot{}}
}

// Map-backed validation may discover independent faults in any order. Only
// diagnostic copies are sorted; original bytes, journal order and observations
// are never reordered or normalized.
func residentOrderedIssues(input []issue) []issue {
	result := append([]issue(nil), input...)
	slices.SortFunc(result, func(a, b issue) int {
		if n := strings.Compare(a.Path, b.Path); n != 0 {
			return n
		}
		if n := strings.Compare(a.Kind, b.Kind); n != 0 {
			return n
		}
		return strings.Compare(a.Message, b.Message)
	})
	return result
}
func fmtResidentIndex(p string, i int) string { return fmt.Sprintf("%s[%d]", p, i) }
func residentPathKey(p string) string         { return strings.ToLower(resourcePath(p)) }
func residentWithin(root, p string) bool {
	a, b := residentPathKey(root), residentPathKey(p)
	return a == b || strings.HasPrefix(b, strings.TrimRight(a, "/")+"/")
}

func (r *residentReader) snapshot(p string, binary bool) *residentSnapshot {
	key := residentPathKey(p)
	if s, ok := r.files[key]; ok {
		return s
	}
	s := &residentSnapshot{Path: p, Binary: binary}
	r.files[key] = s
	fail := func(err error) *residentSnapshot {
		s.Error = err.Error()
		r.invalid(p, "snapshot: "+err.Error())
		return s
	}
	if !residentLocalPath(p) {
		return fail(fmt.Errorf("original absolute local path required"))
	}
	// Reject links in every existing ancestor. This is consistency at read time,
	// not a filesystem sandbox or attestation of stable exclusive ownership.
	for q := filepath.Clean(p); ; q = filepath.Dir(q) {
		st, err := os.Lstat(q)
		if err == nil && st.Mode()&os.ModeSymlink != 0 {
			return fail(fmt.Errorf("link/reparse path boundary"))
		}
		if err != nil && !os.IsNotExist(err) {
			return fail(err)
		}
		next := filepath.Dir(q)
		if q == next {
			break
		}
	}
	f, err := os.Open(p)
	if err != nil {
		return fail(err)
	}
	before, err := f.Stat()
	if err != nil {
		_ = f.Close()
		return fail(err)
	}
	limit := int64(residentFileLimit)
	if binary {
		limit = 256 << 20
	}
	if !before.Mode().IsRegular() || before.Size() > limit || before.Size() > int64(residentAggregateLimit-r.total) {
		_ = f.Close()
		return fail(fmt.Errorf("file/aggregate snapshot bound exceeded or nonregular file"))
	}
	data, err := io.ReadAll(io.LimitReader(f, limit+1))
	after, se := f.Stat()
	ce := f.Close()
	if err != nil {
		return fail(err)
	}
	if se != nil {
		return fail(se)
	}
	if ce != nil {
		return fail(ce)
	}
	if int64(len(data)) > limit || int64(len(data)) != before.Size() || after.Size() != before.Size() || !after.ModTime().Equal(before.ModTime()) {
		return fail(fmt.Errorf("snapshot changed or exceeded bound"))
	}
	s.data = bytes.Clone(data)
	s.Bytes = len(data)
	s.SHA256 = fmt.Sprintf("%x", sha256.Sum256(data))
	if !binary {
		s.Raw = bytes.Clone(data)
	}
	r.total += len(data)
	return s
}

func residentLocalPath(p string) bool {
	if !filepath.IsAbs(p) || strings.ContainsAny(p, "\x00<>\"|?*") || strings.HasPrefix(strings.ReplaceAll(p, `\`, "/"), "//") {
		return false
	}
	volume := filepath.VolumeName(p)
	for _, part := range strings.FieldsFunc(strings.TrimPrefix(p, volume), func(c rune) bool { return c == '/' || c == '\\' }) {
		if part == "." || part == ".." || strings.HasSuffix(part, ".") || strings.HasSuffix(part, " ") || strings.Contains(part, ":") {
			return false
		}
		stem := strings.ToUpper(strings.SplitN(part, ".", 2)[0])
		if slices.Contains([]string{"CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"}, stem) {
			return false
		}
	}
	return true
}

func (r *residentReader) bound(ref residentFileRef, binary bool) *residentSnapshot {
	s := r.snapshot(ref.Path, binary)
	if !isSHA256(ref.SHA256) || s.SHA256 != ref.SHA256 {
		r.invalid(ref.Path, "declared file SHA256 differs from captured bytes")
	}
	return s
}

func (r *residentReader) obj(data []byte, p, required, optional string) object {
	if !resourceDocument(&r.validation, data, p) {
		return nil
	}
	return r.object(data, p, strings.Fields(required), strings.Fields(optional))
}

func (r *residentReader) arr(raw json.RawMessage, p string, max int) []json.RawMessage {
	var a []json.RawMessage
	if len(bytes.TrimSpace(raw)) == 0 || bytes.TrimSpace(raw)[0] != '[' || json.Unmarshal(raw, &a) != nil || len(a) > max {
		r.invalid(p, "bounded JSON array required")
		return nil
	}
	return a
}

func (r *residentReader) str(o object, k, p string) string {
	var s string
	if !bytes.HasPrefix(bytes.TrimSpace(o[k]), []byte{'"'}) || !r.field(o, k, p, &s) {
		r.invalid(p+"."+k, "required string missing")
	}
	return s
}

var (
	residentUnsignedToken = regexp.MustCompile(`^(0|[1-9][0-9]*)$`)
	residentSignedToken   = regexp.MustCompile(`^(0|-?[1-9][0-9]*)$`)
)

func (r *residentReader) uint(o object, k, p string, max uint64) uint64 {
	t := strings.TrimSpace(string(o[k]))
	n, e := strconv.ParseUint(t, 10, 64)
	if !residentUnsignedToken.MatchString(t) || e != nil || n > max {
		r.invalid(p+"."+k, "exact unsigned native integer token/range required")
		return 0
	}
	return n
}

func (r *residentReader) sint(o object, k, p string) int64 {
	t := strings.TrimSpace(string(o[k]))
	n, e := strconv.ParseInt(t, 10, 64)
	if !residentSignedToken.MatchString(t) || e != nil {
		r.invalid(p+"."+k, "exact signed int64 token/range required")
		return 0
	}
	return n
}

func (r *residentReader) boolean(o object, k, p string) bool {
	var b bool
	token := string(bytes.TrimSpace(o[k]))
	if (token != "true" && token != "false") || !r.field(o, k, p, &b) {
		r.invalid(p+"."+k, "required Boolean missing")
	}
	return b
}

func (r *residentReader) want(o object, k, p, want string) {
	if r.str(o, k, p) != want {
		r.invalid(p+"."+k, "expected "+strconv.Quote(want))
	}
}

func (r *residentReader) flag(o object, k, p string, want bool) {
	if r.boolean(o, k, p) != want {
		r.invalid(p+"."+k, "contradictory Boolean")
	}
}

func (r *residentReader) number(o object, k, p string, want uint64) {
	if r.uint(o, k, p, math.MaxUint64) != want {
		r.invalid(p+"."+k, "unexpected integer value")
	}
}

func (r *residentReader) equal(a, b json.RawMessage, p string) {
	if !resourceJSONEqual(a, b) {
		r.invalid(p, "joined JSON values differ")
	}
}

func (r *residentReader) absent(o object, p string, keys ...string) {
	for _, k := range keys {
		if _, ok := o[k]; ok {
			r.invalid(p+"."+k, "field must be absent")
		}
	}
}

func (r *residentReader) noError(o object, p string, keys ...string) {
	for _, k := range keys {
		if _, ok := o[k]; ok {
			if r.str(o, k, p) != "" {
				r.invalid(p+"."+k, "contradictory error")
			}
		}
	}
}

func (r *residentReader) stamp(o object, k, p string) time.Time {
	t, e := time.Parse(time.RFC3339Nano, r.str(o, k, p))
	if e != nil {
		r.invalid(p+"."+k, "RFC3339 chronology required")
	}
	return t
}

func (r *residentReader) pathEqual(a, b, p string) {
	if !resourceAbsolute(a) || !resourceAbsolute(b) || !resourcePathEqual(a, b) {
		r.invalid(p, "original recorded path does not match required location")
	}
}

func (r *residentReader) ref(raw json.RawMessage, p string) residentFileRef {
	o := r.obj(raw, p, "path sha256", "")
	return residentFileRef{r.str(o, "path", p), r.str(o, "sha256", p)}
}

func (r *residentReader) parsePlan(data []byte) residentReaderPlan {
	o := r.obj(data, "$.resident_plan", "schema evidence_kind policy_sha256 campaigns", "admission")
	r.number(o, "schema", "$.resident_plan", 1)
	r.kind = r.str(o, "evidence_kind", "$.resident_plan")
	if !slices.Contains([]string{"invented-protocol", "recorded-evidence"}, r.kind) {
		r.invalid("$.resident_plan.evidence_kind", "explicit invented-protocol or recorded-evidence required")
	}
	r.want(o, "policy_sha256", "$.resident_plan", residentPolicySHA)
	var plan residentReaderPlan
	if json.Unmarshal(data, &plan) != nil {
		r.invalid("$.resident_plan", "malformed typed outer campaign plan")
	}
	campaigns := r.arr(o["campaigns"], "$.resident_plan.campaigns", 2)
	if len(campaigns) == 0 {
		r.invalid("$.resident_plan.campaigns", "one calibration/leaf campaign or primary+secondary required")
	}
	for i, raw := range campaigns {
		p := fmt.Sprintf("$.resident_plan.campaigns[%d]", i)
		c := r.obj(raw, p, "key calibration comparison", "")
		k := r.obj(c["key"], p+".key", "comparator scenario settings role", "")
		for _, f := range []string{"comparator", "scenario", "settings", "role"} {
			r.str(k, f, p+".key")
		}
		for _, track := range []string{"calibration", "comparison"} {
			for j, b := range r.arr(c[track], p+"."+track, 3) {
				q := fmt.Sprintf("%s.%s[%d]", p, track, j)
				v := r.obj(b, q, "directory plan chronology", "")
				r.str(v, "directory", q)
				r.uint(v, "chronology", q, math.MaxInt32)
				r.ref(v["plan"], q+".plan")
			}
		}
	}
	if plan.Admission != nil {
		r.ref(o["admission"], "$.resident_plan.admission")
	}
	return plan
}
