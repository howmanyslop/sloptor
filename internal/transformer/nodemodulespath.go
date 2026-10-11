package transformer

import (
	"path/filepath"
	"strings"

	"rotor/internal/rojo"
)

// NodeModulesPathMapping maps the types entry of each typeRoot package to its
// shipped runtime path (createNodeModulesPathMapping.ts, extended with
// package.json `exports`). Exact keys are canonicalized via
// rojo.CanonicalFileName; Patterns cover wildcard export subpaths ("./x/*").
type NodeModulesPathMapping struct {
	Exact    map[string]string
	Patterns []NodeModulesPathPattern
}

// NodeModulesPathPattern is one wildcard export subpath. A types file matches
// when its canonical path starts with TypesPrefix and ends with TypesSuffix;
// the text between them replaces every `*` in Runtime.
type NodeModulesPathPattern struct {
	TypesPrefix string
	TypesSuffix string
	Runtime     string
}

// Lookup returns the runtime path for a types file: an exact key first, then
// the matching pattern with the longest prefix (TS export pattern order).
func (m NodeModulesPathMapping) Lookup(filePath string, useCaseSensitiveFileNames bool) (string, bool) {
	key := rojo.CanonicalFileName(filePath, useCaseSensitiveFileNames)
	if mapped, ok := m.Exact[key]; ok {
		return mapped, true
	}

	best := -1
	for i, pattern := range m.Patterns {
		if len(key) < len(pattern.TypesPrefix)+len(pattern.TypesSuffix) ||
			!strings.HasPrefix(key, pattern.TypesPrefix) || !strings.HasSuffix(key, pattern.TypesSuffix) {
			continue
		}
		if best < 0 || len(pattern.TypesPrefix) > len(m.Patterns[best].TypesPrefix) {
			best = i
		}
	}
	if best < 0 {
		return "", false
	}

	pattern := m.Patterns[best]
	// Take the star text from the original path so case-insensitive file
	// systems keep the import's casing; fall back to the key if lowercasing
	// changed the byte length.
	source := filepath.Clean(filepath.FromSlash(filePath))
	if len(source) != len(key) {
		source = key
	}
	star := source[len(pattern.TypesPrefix) : len(source)-len(pattern.TypesSuffix)]
	return strings.ReplaceAll(pattern.Runtime, "*", star), true
}
