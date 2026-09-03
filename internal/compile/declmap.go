package compile

import (
	"strings"
)

// base64VLQAlphabet is the source-map VLQ digit alphabet (RFC 4648 base64,
// used as an ordinal alphabet rather than as an encoding).
const base64VLQAlphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"

// utf8ByteOrderMark is what `emitBOM` prepends to an emitted file.
const utf8ByteOrderMark = "\ufeff"

// shiftDeclarationMapColumns keeps a `.d.ts.map` honest after the module
// specifiers in the emitted `.d.ts` were spliced to a different length.
//
// tsgo writes the map from the text it printed, and the paths rewrite runs
// after that, so every mapping that sits to the RIGHT of a rewritten specifier
// on the same generated line points one or more columns off. Editors are the
// only consumer of a `.d.ts.map` (go-to-definition through a declaration), and
// a wrong column there lands the cursor mid-token, so the columns are fixed up
// rather than dropped. Only field 0 of each segment (the generated column) is
// touched; every other field is copied through verbatim, so names, sources,
// and source positions are preserved exactly.
//
// declText is the text as EMITTED (before the splices), because that is the
// coordinate system the map was written in.
func shiftDeclarationMapColumns(mapText string, declText string, edits []textEdit) string {
	// `emitBOM` prepends a byte order mark AFTER the map is generated, so the
	// map counts columns in text that has none while the edits are offsets
	// into text that does. Drop it from both before they are compared.
	bomOffset := 0
	if strings.HasPrefix(declText, utf8ByteOrderMark) {
		bomOffset = len(utf8ByteOrderMark)
		declText = declText[bomOffset:]
	}
	shifts := declarationColumnShifts(declText, edits, bomOffset)
	if len(shifts) == 0 {
		return mapText
	}
	// tsgo writes the map by json.Marshal of sourcemap.RawSourceMap, so
	// `mappings` is a plain JSON string whose value is base64 VLQ — an
	// alphabet with nothing JSON has to escape. Splicing that one value keeps
	// every other byte of the map (field order included) exactly as tsgo wrote
	// it, which a decode/re-encode round trip would not.
	const key = `"mappings":"`
	start := strings.Index(mapText, key)
	if start < 0 {
		return mapText
	}
	start += len(key)
	end := strings.IndexByte(mapText[start:], '"')
	if end < 0 {
		return mapText
	}
	end += start
	shifted := shiftMappingColumns(mapText[start:end], shifts)
	return mapText[:start] + shifted + mapText[end:]
}

// columnShift is one splice expressed in source-map coordinates: every mapping
// on line `line` whose generated column is at or past `column` moves by
// `delta` UTF-16 code units.
type columnShift struct {
	column int
	delta  int
}

// declarationColumnShifts converts byte-range edits over declText into per-line
// UTF-16 column shifts, keyed by zero-based generated line.
func declarationColumnShifts(declText string, edits []textEdit, bomOffset int) map[int][]columnShift {
	if len(edits) == 0 {
		return nil
	}
	shifts := map[int][]columnShift{}
	for _, edit := range edits {
		startLine, startColumn, ok := utf16Position(declText, edit.start-bomOffset)
		if !ok {
			continue
		}
		endLine, endColumn, ok := utf16Position(declText, edit.end-bomOffset)
		// A module specifier is a single-line token; anything else is not
		// something this fixup can describe, so leave the map alone for it.
		if !ok || endLine != startLine {
			continue
		}
		delta := utf16Length(edit.text) - (endColumn - startColumn)
		if delta == 0 {
			continue
		}
		shifts[startLine] = append(shifts[startLine], columnShift{column: endColumn, delta: delta})
	}
	if len(shifts) == 0 {
		return nil
	}
	return shifts
}

// shiftMappingColumns rewrites the generated column of every segment in a
// base64 VLQ mappings string. Generated columns are delta-encoded per line and
// reset at every `;`, so the running column is tracked on both sides.
func shiftMappingColumns(mappings string, shifts map[int][]columnShift) string {
	var builder strings.Builder
	builder.Grow(len(mappings) + len(mappings)/8)
	line := 0
	previous := 0
	shifted := 0
	for index := 0; index < len(mappings); {
		switch mappings[index] {
		case ';':
			builder.WriteByte(';')
			index++
			line++
			previous = 0
			shifted = 0
			continue
		case ',':
			builder.WriteByte(',')
			index++
			continue
		}
		segmentEnd := index
		for segmentEnd < len(mappings) && mappings[segmentEnd] != ',' && mappings[segmentEnd] != ';' {
			segmentEnd++
		}
		value, next, ok := decodeBase64VLQ(mappings, index)
		if !ok {
			// Unparseable mappings are not worth failing an emit over; copy
			// the rest through and let the editor deal with what tsgo wrote.
			builder.WriteString(mappings[index:])
			return builder.String()
		}
		column := previous + value
		adjusted := column + shiftFor(shifts[line], column)
		appendVLQ(&builder, adjusted-shifted, base64VLQAlphabet)
		builder.WriteString(mappings[next:segmentEnd])
		previous = column
		shifted = adjusted
		index = segmentEnd
	}
	return builder.String()
}

// shiftFor sums every shift on a line that lands at or before column.
func shiftFor(lineShifts []columnShift, column int) int {
	total := 0
	for _, shift := range lineShifts {
		if column >= shift.column {
			total += shift.delta
		}
	}
	return total
}

func decodeBase64VLQ(text string, index int) (value int, next int, ok bool) {
	result := 0
	shift := uint(0)
	for index < len(text) {
		digit := strings.IndexByte(base64VLQAlphabet, text[index])
		if digit < 0 {
			return 0, index, false
		}
		index++
		result |= (digit & 31) << shift
		if digit&32 == 0 {
			if result&1 != 0 {
				return -(result >> 1), index, true
			}
			return result >> 1, index, true
		}
		shift += 5
	}
	return 0, index, false
}

// utf16Length counts a string's length in UTF-16 code units, which is the unit
// source-map columns are expressed in.
func utf16Length(text string) int {
	length := 0
	for _, r := range text {
		if r > 0xFFFF {
			length += 2
			continue
		}
		length++
	}
	return length
}
