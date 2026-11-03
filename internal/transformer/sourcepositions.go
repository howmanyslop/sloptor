package transformer

import (
	"sort"
	"unicode/utf8"

	"rotor/tsgo/ast"
	"rotor/tsgo/scanner"
)

// At each completed non-ASCII rune, record cumulative bytes in excess of
// UTF-16 code units. TextPos is int32, so these offsets share its size limit.
type sourceColumnCheckpoint struct {
	byteOffset int32
	extraBytes int32
}

func (state *State) sourcePositionAt(file *ast.SourceFile, offset int) SourcePosition {
	if !state.SourceMapEnabled {
		line, column := scanner.GetECMALineAndUTF16CharacterOfPosition(file, offset)
		return SourcePosition{Line: line, Column: int(column)}
	}
	checkpoints, ok := state.sourceMapColumnIndexes[file]
	if !ok {
		if state.sourceMapColumnIndexes == nil {
			state.sourceMapColumnIndexes = make(map[*ast.SourceFile][]sourceColumnCheckpoint)
		}
		checkpoints = sourceColumnCheckpoints(file.Text())
		state.sourceMapColumnIndexes[file] = checkpoints
	}
	line, column := scanner.GetECMALineAndByteOffsetOfPosition(file, offset)
	lineStart := offset - column
	column -= sourceColumnAdjustment(checkpoints, offset) - sourceColumnAdjustment(checkpoints, lineStart)
	return SourcePosition{Line: line, Column: column}
}

func sourceColumnCheckpoints(text string) []sourceColumnCheckpoint {
	// Non-nil even for pure-ASCII text: callers pass the result straight
	// into sourceColumnAdjustment, which must never see a nil slice.
	checkpoints := []sourceColumnCheckpoint{}
	var extraBytes int32
	for offset := 0; offset < len(text); {
		if text[offset] < utf8.RuneSelf {
			offset++
			continue
		}
		r, width := utf8.DecodeRuneInString(text[offset:])
		offset += width
		if width == 1 {
			continue // An invalid UTF-8 byte is one replacement code unit.
		}
		extraBytes += int32(width - 1)
		if r > 0xffff {
			extraBytes-- // A non-BMP rune occupies two UTF-16 code units.
		}
		checkpoints = append(checkpoints, sourceColumnCheckpoint{byteOffset: int32(offset), extraBytes: extraBytes})
	}
	return checkpoints
}

func sourceColumnAdjustment(checkpoints []sourceColumnCheckpoint, offset int) int {
	index := sort.Search(len(checkpoints), func(i int) bool { return int(checkpoints[i].byteOffset) > offset })
	if index == 0 {
		return 0
	}
	return int(checkpoints[index-1].extraBytes)
}
