package compile

import (
	"strings"
	"testing"
)

// encodeGeneratedColumns builds a mappings string whose segments carry only a
// generated column (the field the fixup touches) plus three constant source
// fields, so a test can state the columns it means.
func encodeGeneratedColumns(lines [][]int) string {
	var builder strings.Builder
	for lineIndex, columns := range lines {
		if lineIndex > 0 {
			builder.WriteByte(';')
		}
		previous := 0
		for columnIndex, column := range columns {
			if columnIndex > 0 {
				builder.WriteByte(',')
			}
			appendVLQ(&builder, column-previous, base64VLQAlphabet)
			appendVLQ(&builder, 0, base64VLQAlphabet)
			appendVLQ(&builder, 0, base64VLQAlphabet)
			appendVLQ(&builder, 0, base64VLQAlphabet)
			previous = column
		}
	}
	return builder.String()
}

func decodeGeneratedColumns(t *testing.T, mappings string) [][]int {
	t.Helper()
	lines := [][]int{{}}
	previous := 0
	for index := 0; index < len(mappings); {
		switch mappings[index] {
		case ';':
			lines = append(lines, []int{})
			previous = 0
			index++
			continue
		case ',':
			index++
			continue
		}
		value, next, ok := decodeBase64VLQ(mappings, index)
		if !ok {
			t.Fatalf("undecodable mappings at %d: %q", index, mappings)
		}
		previous += value
		lines[len(lines)-1] = append(lines[len(lines)-1], previous)
		for next < len(mappings) && mappings[next] != ',' && mappings[next] != ';' {
			next++
		}
		index = next
	}
	return lines
}

func wrapMappings(mappings string) string {
	return `{"version":3,"file":"main.d.ts","sourceRoot":"","sources":["../src/main.ts"],"names":[],"mappings":"` + mappings + `"}`
}

func TestShiftDeclarationMapColumns(t *testing.T) {
	// Line 0 of a printed declaration, with the specifier at columns 26..44.
	const declText = "import { Value } from \"@alias/value\";\nexport declare const value: Value;\n"
	specifierStart := strings.Index(declText, `"@alias/value"`)
	specifierEnd := specifierStart + len(`"@alias/value"`)

	for _, testCase := range []struct {
		name  string
		text  string
		edits []textEdit
		input [][]int
		want  [][]int
	}{
		{
			name:  "columns past a shortened specifier move left",
			text:  declText,
			edits: []textEdit{{start: specifierStart, end: specifierEnd, text: `"./value"`}},
			input: [][]int{{0, 7, 26, 37, 38}, {0, 7, 21}},
			want:  [][]int{{0, 7, 26, 32, 33}, {0, 7, 21}},
		},
		{
			name:  "columns past a lengthened specifier move right",
			text:  declText,
			edits: []textEdit{{start: specifierStart, end: specifierEnd, text: `"./deeply/nested/value"`}},
			input: [][]int{{0, 26, 37}, {0, 21}},
			want:  [][]int{{0, 26, 46}, {0, 21}},
		},
		{
			name:  "a same-length rewrite leaves the map alone",
			text:  declText,
			edits: []textEdit{{start: specifierStart, end: specifierEnd, text: `"./aliased/xy"`}},
			input: [][]int{{0, 26, 37}},
			want:  [][]int{{0, 26, 37}},
		},
		{
			name: "two rewrites on one line accumulate",
			text: "import \"@alias/a\";import \"@alias/b\";\n",
			edits: []textEdit{
				{start: 7, end: 17, text: `"./a"`},
				{start: 25, end: 35, text: `"./b"`},
			},
			input: [][]int{{0, 7, 18, 25, 36}},
			want:  [][]int{{0, 7, 13, 20, 26}},
		},
		{
			name:  "a byte order mark does not offset the columns",
			text:  utf8ByteOrderMark + declText,
			edits: []textEdit{{start: len(utf8ByteOrderMark) + specifierStart, end: len(utf8ByteOrderMark) + specifierEnd, text: `"./value"`}},
			input: [][]int{{0, 26, 37}},
			want:  [][]int{{0, 26, 32}},
		},
		{
			// Source-map columns count UTF-16 code units, so an astral
			// character before the specifier is worth two columns, not one
			// rune and not four bytes.
			name:  "a non-BMP character before the specifier costs two columns",
			text:  "/** \U0001F600 */ import \"@alias/value\";\n",
			edits: []textEdit{{start: 12, end: 26, text: `"./value"`}},
			input: [][]int{{0, 12, 26}},
			want:  [][]int{{0, 12, 21}},
		},
		{
			name:  "CRLF line endings do not shift the columns",
			text:  "import \"@alias/value\";\r\nexport declare const value: number;\r\n",
			edits: []textEdit{{start: 7, end: 21, text: `"./value"`}},
			input: [][]int{{0, 7, 21}, {0, 7}},
			want:  [][]int{{0, 7, 16}, {0, 7}},
		},
		{
			name:  "empty generated lines are preserved",
			text:  "import \"@alias/value\";\n\n\nexport declare const value: number;\n",
			edits: []textEdit{{start: 7, end: 21, text: `"./value"`}},
			input: [][]int{{0, 7, 21}, {}, {}, {0, 7}},
			want:  [][]int{{0, 7, 16}, {}, {}, {0, 7}},
		},
		{
			// A generated column may go BACKWARD within a line (tsgo emits
			// out-of-order segments for a reprinted type), so the running
			// column has to be tracked on both sides rather than assumed
			// monotonic.
			name:  "negative generated-column deltas survive the shift",
			text:  "import \"@alias/value\";\n",
			edits: []textEdit{{start: 7, end: 21, text: `"./value"`}},
			input: [][]int{{0, 21, 7, 21, 0}},
			want:  [][]int{{0, 16, 7, 16, 0}},
		},
		{
			name:  "no edits leaves the map byte-identical",
			text:  declText,
			edits: nil,
			input: [][]int{{0, 26, 37}},
			want:  [][]int{{0, 26, 37}},
		},
	} {
		t.Run(testCase.name, func(t *testing.T) {
			mapText := wrapMappings(encodeGeneratedColumns(testCase.input))
			shifted := shiftDeclarationMapColumns(mapText, testCase.text, testCase.edits)
			got := decodeGeneratedColumns(t, declarationMapMappings(t, shifted))
			if len(got) != len(testCase.want) {
				t.Fatalf("generated lines = %d, want %d (%v)", len(got), len(testCase.want), got)
			}
			for lineIndex := range testCase.want {
				if len(got[lineIndex]) != len(testCase.want[lineIndex]) {
					t.Fatalf("line %d columns = %v, want %v", lineIndex, got[lineIndex], testCase.want[lineIndex])
				}
				for columnIndex, column := range testCase.want[lineIndex] {
					if got[lineIndex][columnIndex] != column {
						t.Fatalf("line %d columns = %v, want %v", lineIndex, got[lineIndex], testCase.want[lineIndex])
					}
				}
			}
			// Everything outside the mappings field is copied verbatim.
			if wrapMappings(declarationMapMappings(t, shifted)) != shifted {
				t.Fatalf("fixup rewrote something other than the mappings field: %s", shifted)
			}
		})
	}
}

// An unparseable mappings string must not fail an emit; the rest is copied
// through and the editor gets whatever tsgo wrote.
func TestShiftDeclarationMapColumnsToleratesUndecodableMappings(t *testing.T) {
	const declText = "import \"@alias/value\";\n"
	edits := []textEdit{{start: 7, end: 21, text: `"./value"`}}
	shifted := shiftDeclarationMapColumns(wrapMappings("AAAA,!!!!"), declText, edits)
	if !strings.Contains(shifted, "!!!!") {
		t.Fatalf("undecodable mappings were dropped: %s", shifted)
	}
}

// declarationMapMappings pulls the `mappings` string out of a source map the
// same way the rewriter's fixup does.
func declarationMapMappings(t *testing.T, mapText string) string {
	t.Helper()
	const key = `"mappings":"`
	start := strings.Index(mapText, key)
	if start < 0 {
		t.Fatalf("declaration map has no mappings field: %s", mapText)
	}
	start += len(key)
	end := strings.IndexByte(mapText[start:], '"')
	if end < 0 {
		t.Fatalf("declaration map mappings field is unterminated: %s", mapText)
	}
	return mapText[start : start+end]
}

// The generated columns of a `.d.ts.map` must never point past the end of the
// line they name; an over-shifted column lands an editor's cursor in nothing.
func TestShiftDeclarationMapColumnsStaysInsideEveryLine(t *testing.T) {
	const declText = "import { Value } from \"@alias/deeply/nested/value\";\nexport declare const value: Value;\n"
	specifierStart := strings.Index(declText, `"@alias`)
	specifierEnd := strings.Index(declText, `;`)
	edits := []textEdit{{start: specifierStart, end: specifierEnd, text: `"./v"`}}
	input := [][]int{{0, 7, 12, 14, 18, 20, specifierStart, specifierEnd, specifierEnd + 1}, {0, 7, 21}}

	shifted := shiftDeclarationMapColumns(wrapMappings(encodeGeneratedColumns(input)), declText, edits)
	rewritten := applyTextEdits(declText, edits)
	lines := strings.Split(rewritten, "\n")
	for lineIndex, columns := range decodeGeneratedColumns(t, declarationMapMappings(t, shifted)) {
		for _, column := range columns {
			if column < 0 || column > len(lines[lineIndex]) {
				t.Fatalf("line %d column %d is outside %q", lineIndex, column, lines[lineIndex])
			}
		}
	}
}
