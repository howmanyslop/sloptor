package main

import (
	"bytes"
	"crypto/sha256"
	"fmt"
	"testing"
)

func unicodeDigestPair(input []byte, field, baseline, candidate string) []byte {
	original := `"` + field + `":"diagnostics"`
	if field == "output_tree_digest" {
		original = `"` + field + `":"output"`
	}
	input = bytes.Replace(input, []byte(original), []byte(`"`+field+`":`+baseline), 1)
	return bytes.Replace(input, []byte(original), []byte(`"`+field+`":`+candidate), 1)
}

func unicodeExtension(input []byte, fragment string) []byte {
	return append(bytes.Clone(input[:len(input)-1]), []byte(","+fragment+"}")...)
}

func TestUnicodeAdmissionRejectsMalformedEscapes(t *testing.T) {
	base := fixture(t, 900, 1000, 10, 20)
	cases := []struct {
		name string
		data []byte
	}{
		{"distinct high diagnostics", unicodeDigestPair(base, "diagnostics_digest", `"\ud800"`, `"\ud801"`)},
		{"distinct high outputs", unicodeDigestPair(base, "output_tree_digest", `"\ud800"`, `"\ud801"`)},
		{"surrogate versus replacement", unicodeDigestPair(base, "diagnostics_digest", `"\ud800"`, `"\ufffd"`)},
		{"surrogate versus literal replacement", unicodeDigestPair(base, "diagnostics_digest", `"\ud800"`, `"�"`)},
		{"last lone high", unicodeExtension(base, `"extension":"\udbff"`)},
		{"first lone low", unicodeExtension(base, `"extension":"\udc00"`)},
		{"last lone low", unicodeExtension(base, `"extension":"\udfff"`)},
		{"high then BMP", unicodeExtension(base, `"extension":"\ud800\u0061"`)},
		{"high then high", unicodeExtension(base, `"extension":"\ud800\ud801"`)},
		{"low then high", unicodeExtension(base, `"extension":"\udc00\ud800"`)},
		{"high then literal astral", unicodeExtension(base, `"extension":"\ud800😀"`)},
		{"high then escaped backslash", unicodeExtension(base, `"extension":"\ud800\\udc00"`)},
		{"high then intervening text", unicodeExtension(base, `"extension":"\ud800x\udc00"`)},
		{"high then escaped quote", unicodeExtension(base, `"extension":"\ud800\"\udc00"`)},
		{"nested unknown value", unicodeExtension(base, `"extension":{"items":[{"label":"\ud800"}]}`)},
		{"nested unknown key", unicodeExtension(base, `"extension":{"items":[{"\udc00":"label"}]}`)},
		{"unknown root key", unicodeExtension(base, `"\ud800":"label"`)},
		{"truncated code unit", unicodeExtension(base, `"extension":"\ud80"`)},
		{"invalid hex unit", unicodeExtension(base, `"extension":"\ud80x"`)},
		{"truncated low unit", unicodeExtension(base, `"extension":"\ud800\udc0"`)},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			r := evaluate(tc.data, config{cold, 0.95, 10, 20})
			if r.Outcome != invalidEvidence || len(r.Issues) == 0 || r.Statistics != nil || r.Primary != nil || r.Guardrail != nil {
				t.Fatalf("malformed Unicode must fail admission without statistics or decisions: outcome=%s issues=%+v", r.Outcome, r.Issues)
			}
			if !bytes.Equal(r.Input.Bytes, tc.data) || r.Input.SHA256 != fmt.Sprintf("%x", sha256.Sum256(tc.data)) {
				t.Fatal("exact original evidence was not retained")
			}
		})
	}
}

func TestUnicodeAdmissionKeepsDistinctValidIdentities(t *testing.T) {
	base := fixture(t, 900, 1000, 10, 20)
	for _, identities := range [][2]string{
		{`"�"`, `"other"`},
		{`"\ud83d\ude00"`, `"😁"`},
		{`"é"`, `"e\u0301"`},
	} {
		data := unicodeDigestPair(base, "diagnostics_digest", identities[0], identities[1])
		r := evaluate(data, config{cold, 0.95, 10, 20})
		if r.Outcome != correctnessFailure || len(r.Issues) != 1 || len(r.Statistics) != 2 {
			t.Fatalf("valid unequal Unicode labels must retain a correctness failure: %s %+v", r.Outcome, r.Issues)
		}
	}
}

func TestUnicodeAdmissionPreservesValidStrings(t *testing.T) {
	base := fixture(t, 900, 1000, 10, 20)
	cases := []struct {
		name string
		data []byte
	}{
		{"surrogate pair versus literal astral", unicodeDigestPair(base, "diagnostics_digest", `"\ud83d\ude00"`, `"😀"`)},
		{"uppercase hex pair", unicodeDigestPair(base, "output_tree_digest", `"\uD83D\uDE00"`, `"😀"`)},
		{"first astral scalar", unicodeDigestPair(base, "diagnostics_digest", `"\ud800\udc00"`, "\"\U00010000\"")},
		{"last scalar", unicodeDigestPair(base, "diagnostics_digest", `"\udbff\udfff"`, "\"\U0010ffff\"")},
		{"legitimate replacement", unicodeDigestPair(base, "diagnostics_digest", `"\ufffd"`, `"�"`)},
		{"ordinary BMP", unicodeDigestPair(base, "diagnostics_digest", `"\u03bb\u4e2d"`, `"λ中"`)},
		{"literal escape text", unicodeDigestPair(base, "diagnostics_digest", `"\\ud800"`, `"\\ud800"`)},
		{"escaped quote and backslash", unicodeExtension(base, `"extension":"quote: \" then literal \\udc00"`)},
		{"backslash before pair", unicodeExtension(base, `"extension":"\\\ud83d\ude00"`)},
		{"two literal backslashes", unicodeExtension(base, `"extension":"\\\\ud800"`)},
		{"nested unknown Unicode", unicodeExtension(base, `"extension":{"\ud83d\ude00":["\ufffd","quote: \"","literal \\ud800","λ中"]}`)},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			r := evaluate(tc.data, config{cold, 0.95, 10, 20})
			if r.Outcome != criterionMet || len(r.Issues) != 0 || len(r.Statistics) != 2 {
				t.Fatalf("valid Unicode must retain the declared evaluation: outcome=%s issues=%+v", r.Outcome, r.Issues)
			}
			if !bytes.Equal(r.Input.Bytes, tc.data) {
				t.Fatal("exact original evidence was not retained")
			}
		})
	}
}
