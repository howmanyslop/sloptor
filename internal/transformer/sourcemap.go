package transformer

import (
	"cmp"
	"encoding/json"
	"path/filepath"
	"slices"
	"strings"
	"unicode"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/tsgo/ast"
)

const base64VLQAlphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"

type sourceMapV3 struct {
	Version        int      `json:"version"`
	File           string   `json:"file"`
	Sources        []string `json:"sources"`
	SourcesContent []string `json:"sourcesContent"`
	Mappings       string   `json:"mappings"`
}

type sourceMapMapping struct {
	generatedLine   int
	generatedColumn int
	sourceLine      int
	sourceColumn    int
}

// RenderWithSourceMap renders statements once and returns their deterministic V3 source map.
func (s *State) RenderWithSourceMap(statements *luau.List[luau.Statement], sourceFile *ast.SourceFile) (text, mapJSON string) {
	text, ranges := render.RenderASTWithPositions(statements)
	positions := make([]*luau.Position, strings.Count(text, "\n")+1)
	for _, r := range ranges {
		if _, comment := r.Node.(*luau.Comment); comment {
			continue
		}
		if _, ok := r.Node.(luau.Statement); ok && r.Node.Origin() != nil && positions[r.Start.Line] == nil {
			positions[r.Start.Line] = &r.Node.Origin().Start
		}
	}
	for _, r := range ranges {
		if origin := r.Node.Origin(); origin != nil && r.Closing != nil && origin.Closing != nil && positions[r.Closing.Line] == nil {
			positions[r.Closing.Line] = origin.Closing
		}
	}
	// Ranges start in text order, with parents before children on a tie.
	for _, r := range ranges {
		if _, statement := r.Node.(luau.Statement); !statement && r.Node.Origin() != nil && positions[r.Start.Line] == nil {
			positions[r.Start.Line] = &r.Node.Origin().Start
		}
	}
	addSourceMapCoverage(text, ranges, positions)
	mappings := make([]sourceMapMapping, 0, len(positions))
	for line, position := range positions {
		if position != nil {
			mappings = append(mappings, sourceMapMapping{
				generatedLine: line,
				sourceLine:    position.Line,
				sourceColumn:  position.Column,
			})
		}
	}
	// Parents precede children at a shared start, so the last start wins.
	// Operator anchors are appended last and take priority on a tie.
	for _, r := range ranges {
		origin := r.Node.Origin()
		if _, statement := r.Node.(luau.Statement); !statement && origin != nil && !origin.StartUnset && r.Start.Column > 0 {
			mappings = append(mappings, sourceMapMapping{r.Start.Line, r.Start.Column, origin.Start.Line, origin.Start.Column})
		}
	}
	for _, r := range ranges {
		if origin := r.Node.Origin(); origin != nil && origin.Operator != nil && r.Operator != nil && r.Operator.Column > 0 {
			mappings = append(mappings, sourceMapMapping{r.Operator.Line, r.Operator.Column, origin.Operator.Line, origin.Operator.Column})
		}
	}
	slices.SortStableFunc(mappings, func(a, b sourceMapMapping) int {
		if order := cmp.Compare(a.generatedLine, b.generatedLine); order != 0 {
			return order
		}
		return cmp.Compare(a.generatedColumn, b.generatedColumn)
	})
	kept := mappings[:0]
	for i, mapping := range mappings {
		if i+1 < len(mappings) && mappings[i+1].generatedLine == mapping.generatedLine && mappings[i+1].generatedColumn == mapping.generatedColumn {
			continue
		}
		if len(kept) > 0 {
			previous := kept[len(kept)-1]
			if previous.generatedLine == mapping.generatedLine && previous.sourceLine == mapping.sourceLine && previous.sourceColumn == mapping.sourceColumn {
				continue
			}
		}
		kept = append(kept, mapping)
	}
	mappings = kept
	fileName := sourceFile.FileName()
	outputFileName := strings.TrimSuffix(fileName, filepath.Ext(fileName)) + ".luau"
	encoded, err := json.Marshal(sourceMapV3{
		Version:        3,
		File:           outputFileName,
		Sources:        []string{fileName},
		SourcesContent: []string{sourceFile.Text()},
		Mappings:       encodeSourceMapMappings(mappings),
	})
	if err != nil {
		panic(err)
	}
	return text, string(encoded)
}

func sourceMapPositionBefore(a, b luau.Position) bool {
	return a.Line < b.Line || a.Line == b.Line && a.Column < b.Column
}

func addSourceMapCoverage(text string, ranges []render.Range, positions []*luau.Position) {
	type openRange struct {
		end     luau.Position
		origin  *luau.Origin
		comment bool
	}
	stack := []openRange{}
	nextRange, lineNumber := 0, 0
	for line := range strings.SplitSeq(text, "\n") {
		lineIndex := lineNumber
		lineNumber++
		if positions[lineIndex] != nil {
			continue
		}
		column, nonBlank := 0, false
		for _, r := range line {
			if !unicode.IsSpace(r) {
				nonBlank = true
				break
			}
			column++
		}
		if !nonBlank {
			continue
		}
		point := luau.Position{Line: lineIndex, Column: column}
		for nextRange < len(ranges) && !sourceMapPositionBefore(point, ranges[nextRange].Start) {
			r := ranges[nextRange]
			nextRange++
			for len(stack) > 0 && !sourceMapPositionBefore(r.Start, stack[len(stack)-1].end) {
				stack = stack[:len(stack)-1]
			}
			origin := r.Node.Origin()
			if origin == nil && len(stack) > 0 {
				origin = stack[len(stack)-1].origin
			}
			_, comment := r.Node.(*luau.Comment)
			stack = append(stack, openRange{end: r.End, origin: origin, comment: comment})
		}
		for len(stack) > 0 && !sourceMapPositionBefore(point, stack[len(stack)-1].end) {
			stack = stack[:len(stack)-1]
		}
		if len(stack) > 0 && !stack[len(stack)-1].comment {
			if origin := stack[len(stack)-1].origin; origin != nil {
				positions[lineIndex] = &origin.Start
			}
		}
	}
}

func encodeSourceMapMappings(mappings []sourceMapMapping) string {
	var builder strings.Builder
	lastGeneratedLine := 0
	lastGeneratedColumn := 0
	lastSourceLine := 0
	lastSourceColumn := 0
	firstMappingOnLine := true

	for _, mapping := range mappings {
		for lastGeneratedLine < mapping.generatedLine {
			builder.WriteByte(';')
			lastGeneratedLine++
			lastGeneratedColumn = 0
			firstMappingOnLine = true
		}
		if !firstMappingOnLine {
			builder.WriteByte(',')
		}
		appendBase64VLQ(&builder, mapping.generatedColumn-lastGeneratedColumn)
		appendBase64VLQ(&builder, 0)
		appendBase64VLQ(&builder, mapping.sourceLine-lastSourceLine)
		appendBase64VLQ(&builder, mapping.sourceColumn-lastSourceColumn)
		lastGeneratedColumn = mapping.generatedColumn
		lastSourceLine = mapping.sourceLine
		lastSourceColumn = mapping.sourceColumn
		firstMappingOnLine = false
	}
	return builder.String()
}

func appendBase64VLQ(builder *strings.Builder, value int) {
	if value < 0 {
		value = (-value << 1) | 1
	} else {
		value <<= 1
	}
	for {
		digit := value & 31
		value >>= 5
		if value > 0 {
			digit |= 32
		}
		builder.WriteByte(base64VLQAlphabet[digit])
		if value == 0 {
			return
		}
	}
}
