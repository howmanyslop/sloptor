package render

import (
	"strconv"
	"strings"

	"rotor/internal/luau"
)

const indentCharacter = "\t"

type RenderState struct {
	indent         string
	seenTempNodes  map[int]string
	tempIDFallback int
	listNodesStack []*luau.ListNode[luau.Statement]
	writer         strings.Builder
	positions      bool
	ranges         []byteRange
	contentEnd     int
	currentRange   int
}

func NewRenderState() *RenderState {
	return &RenderState{seenTempNodes: map[int]string{}}
}

func (s *RenderState) pushIndent() { s.indent += indentCharacter }
func (s *RenderState) popIndent()  { s.indent = s.indent[len(indentCharacter):] }

// getTempName: solveTempIds should have populated seenTempNodes; the counter
// fallback mirrors upstream's safety net.
func (s *RenderState) getTempName(node *luau.TemporaryIdentifier) string {
	if name, ok := s.seenTempNodes[node.ID]; ok {
		return name
	}
	name := "_" + strconv.Itoa(s.tempIDFallback)
	s.tempIDFallback++
	s.seenTempNodes[node.ID] = name
	return name
}

func (s *RenderState) pushListNode(n *luau.ListNode[luau.Statement]) {
	s.listNodesStack = append(s.listNodesStack, n)
}

func (s *RenderState) peekListNode() *luau.ListNode[luau.Statement] {
	if len(s.listNodesStack) == 0 {
		return nil
	}
	return s.listNodesStack[len(s.listNodesStack)-1]
}

func (s *RenderState) popListNode() {
	s.listNodesStack = s.listNodesStack[:len(s.listNodesStack)-1]
}

func (s *RenderState) Indented(text string) string { return s.indent + text }

func (s *RenderState) Line(text string) string { return s.Indented(text) + "\n" }

// LineWithEnd is upstream state.line(text, endNode): appends `;` when needed
// to avoid Luau ambiguous-syntax errors.
func (s *RenderState) LineWithEnd(text string, endNode luau.Statement) string {
	return s.Indented(text) + getEnding(s, endNode) + "\n"
}

func (s *RenderState) Block(callback func() string) string {
	s.pushIndent()
	result := callback()
	s.popIndent()
	return result
}

// Layout writes do not extend a node's content range.
func (s *RenderState) writeIndent() { s.writer.WriteString(s.indent) }
func (s *RenderState) newline()     { s.writer.WriteByte('\n') }

func (s *RenderState) write(text string) {
	s.writer.WriteString(text)
	if s.positions {
		s.contentEnd = s.writer.Len()
	}
}

func (s *RenderState) closing(keyword string) {
	if s.positions {
		s.ranges[s.currentRange].closing = s.writer.Len()
	}
	s.write(keyword)
}

func (s *RenderState) block(statements *luau.List[luau.Statement]) {
	s.pushIndent()
	s.statements(statements)
	s.popIndent()
}
