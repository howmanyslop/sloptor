package flamework

import (
	"sort"
	"strings"
	"unicode"
)

type Metadata struct {
	tokens map[string]struct{}
}

func NewMetadata(tokens []string) Metadata {
	set := make(map[string]struct{}, len(tokens))
	for _, token := range tokens {
		set[token] = struct{}{}
	}
	return Metadata{tokens: set}
}

func ParseMetadataText(text string) Metadata {
	trimmed := strings.TrimSpace(text)
	start := -1
	end := -1
	for index, character := range trimmed {
		if unicode.IsSpace(character) {
			if start == -1 {
				start = index
			}
			end = index + len(string(character))
			continue
		}
		if start != -1 {
			break
		}
	}
	if start != -1 {
		trimmed = trimmed[:start] + " " + trimmed[end:]
	}
	return NewMetadata(strings.Split(trimmed, " "))
}

func (m Metadata) Requested(name string) bool {
	if _, excluded := m.tokens["~"+name]; excluded {
		return false
	}
	_, requested := m.tokens[name]
	_, wildcard := m.tokens["*"]
	return requested || wildcard
}

func (m Metadata) Tokens() []string {
	tokens := make([]string, 0, len(m.tokens))
	for token := range m.tokens {
		tokens = append(tokens, token)
	}
	sort.Strings(tokens)
	return tokens
}
