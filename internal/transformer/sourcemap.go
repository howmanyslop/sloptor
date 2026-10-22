package transformer

import (
	"encoding/json"
	"path/filepath"
	"strings"

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
		if _, ok := r.Node.(luau.Statement); ok && r.Node.Origin() != nil && positions[r.Start.Line] == nil {
			positions[r.Start.Line] = &r.Node.Origin().Start
		}
	}
	for _, r := range ranges {
		if origin := r.Node.Origin(); origin != nil && r.Closing != nil && origin.Closing != nil && positions[r.Closing.Line] == nil {
			positions[r.Closing.Line] = origin.Closing
		}
	}
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
