package compile

import (
	"encoding/json"
	"fmt"
	"strings"

	"rotor/internal/transformer"
	"rotor/tsgo/sourcemap"
)

func rewriteSourceMapWithTrace(raw string, trace *sourceTraceMap) (string, error) {
	var sourceMap rawSourceMap
	if err := json.Unmarshal([]byte(raw), &sourceMap); err != nil {
		return "", fmt.Errorf("parse Luau source map: %w", err)
	}
	if sourceMap.Version != 3 {
		return "", fmt.Errorf("parse Luau source map: version = %d, want 3", sourceMap.Version)
	}

	mappings := []emittedMapping{}
	decoder := sourcemap.DecodeMappings(sourceMap.Mappings)
	for mapping := range decoder.Values() {
		emitted := emittedMapping{
			generatedLine:   mapping.GeneratedLine,
			generatedColumn: int(mapping.GeneratedCharacter),
		}
		if mapping.IsSourceMapping() {
			original := trace.OriginalPosition(transformer.SourcePosition{
				Line:   mapping.SourceLine,
				Column: int(mapping.SourceCharacter),
			})
			if original != nil {
				emitted.sourceLine = original.Line
				emitted.sourceColumn = original.Column
				emitted.hasSource = true
			}
		}
		mappings = append(mappings, emitted)
	}
	if err := decoder.Error(); err != nil {
		return "", fmt.Errorf("parse Luau source map mappings: %w", err)
	}

	sourceMap.Sources = []string{trace.OriginalSourceFileName()}
	sourceMap.SourcesContent = []string{trace.OriginalSourceText()}
	sourceMap.Mappings = encodeEmittedMappings(mappings)
	encoded, err := json.Marshal(sourceMap)
	if err != nil {
		return "", fmt.Errorf("encode Luau source map: %w", err)
	}
	return string(encoded), nil
}

type emittedMapping struct {
	generatedLine   int
	generatedColumn int
	sourceLine      int
	sourceColumn    int
	hasSource       bool
}

func encodeEmittedMappings(mappings []emittedMapping) string {
	const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
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
		appendVLQ(&builder, mapping.generatedColumn-lastGeneratedColumn, alphabet)
		lastGeneratedColumn = mapping.generatedColumn
		if mapping.hasSource {
			appendVLQ(&builder, 0, alphabet)
			appendVLQ(&builder, mapping.sourceLine-lastSourceLine, alphabet)
			appendVLQ(&builder, mapping.sourceColumn-lastSourceColumn, alphabet)
			lastSourceLine = mapping.sourceLine
			lastSourceColumn = mapping.sourceColumn
		}
		firstMappingOnLine = false
	}
	return builder.String()
}

func appendVLQ(builder *strings.Builder, value int, alphabet string) {
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
		builder.WriteByte(alphabet[digit])
		if value == 0 {
			return
		}
	}
}
