package render

import (
	"cmp"
	"slices"
	"unicode/utf8"

	"rotor/internal/luau"
)

// Range describes one rendered node occurrence. Coordinates are 0-based with
// UTF-16 columns. End is exclusive; surrounding indentation and newlines are
// excluded. Closing points at the end or until keyword, when present.
type Range struct {
	Node       luau.Node
	Start, End luau.Position
	Closing    *luau.Position
}

type byteRange struct {
	node                luau.Node
	start, end, closing int
}

// RenderASTWithPositions renders the same text as RenderAST and returns ranges
// in start order, with enclosing nodes before children that start at the same
// position. A node used more than once has a range for each occurrence.
func RenderASTWithPositions(ast *luau.List[luau.Statement]) (string, []Range) {
	s := NewRenderState()
	s.positions = true
	s.prepareTempNames(ast)
	s.statements(ast)
	text := s.writer.String()
	return text, positionsForRanges(text, s.ranges)
}

func (s *RenderState) beginRange(node luau.Node) int {
	index := len(s.ranges)
	s.ranges = append(s.ranges, byteRange{node: node, start: s.writer.Len(), closing: -1})
	return index
}

func (s *RenderState) endRange(index int) { s.ranges[index].end = s.contentEnd }

func positionsForRanges(text string, offsets []byteRange) []Range {
	type coordinate struct {
		offset int
		target *luau.Position
	}
	ranges := make([]Range, len(offsets))
	coordinates := make([]coordinate, 0, 3*len(offsets))
	for i, offset := range offsets {
		r := &ranges[i]
		r.Node = offset.node
		coordinates = append(coordinates, coordinate{offset.start, &r.Start}, coordinate{offset.end, &r.End})
		if offset.closing >= 0 {
			r.Closing = new(luau.Position)
			coordinates = append(coordinates, coordinate{offset.closing, r.Closing})
		}
	}
	slices.SortFunc(coordinates, func(a, b coordinate) int { return cmp.Compare(a.offset, b.offset) })
	position := luau.Position{}
	byteOffset := 0
	for _, coordinate := range coordinates {
		for byteOffset < coordinate.offset {
			c := text[byteOffset]
			if c == '\n' {
				position.Line++
				position.Column = 0
				byteOffset++
			} else if c < utf8.RuneSelf {
				position.Column++
				byteOffset++
			} else {
				r, size := utf8.DecodeRuneInString(text[byteOffset:])
				position.Column++
				if r > 0xffff {
					position.Column++
				}
				byteOffset += size
			}
		}
		*coordinate.target = position
	}
	return ranges
}
