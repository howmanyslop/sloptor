package transformer

import (
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/rojo"
)

func TestNodeModulesPathMappingLookup(t *testing.T) {
	pkgDir := filepath.FromSlash("/repo/node_modules/@scope/pkg")
	join := func(parts ...string) string { return filepath.Join(append([]string{pkgDir}, parts...)...) }
	pattern := func(types, runtime string, caseSensitive bool) NodeModulesPathPattern {
		prefix, suffix, _ := strings.Cut(rojo.CanonicalFileName(types, caseSensitive), "*")
		return NodeModulesPathPattern{TypesPrefix: prefix, TypesSuffix: suffix, Runtime: runtime}
	}

	t.Run("prefers an exact key over a pattern", func(t *testing.T) {
		mapping := NodeModulesPathMapping{
			Exact:    map[string]string{rojo.CanonicalFileName(join("out", "a.d.ts"), true): join("exact", "a.luau")},
			Patterns: []NodeModulesPathPattern{pattern(join("out", "*.d.ts"), join("out", "*.luau"), true)},
		}

		got, ok := mapping.Lookup(join("out", "a.d.ts"), true)

		if !ok || got != join("exact", "a.luau") {
			t.Errorf("Lookup = %q, %v; want %q", got, ok, join("exact", "a.luau"))
		}
	})

	t.Run("substitutes every star in the runtime target", func(t *testing.T) {
		mapping := NodeModulesPathMapping{
			Patterns: []NodeModulesPathPattern{pattern(join("types", "*.d.ts"), join("out", "*", "*.luau"), true)},
		}

		got, _ := mapping.Lookup(join("types", "a.d.ts"), true)

		if want := join("out", "a", "a.luau"); got != want {
			t.Errorf("Lookup = %q, want %q", got, want)
		}
	})

	t.Run("keeps the import casing on case-insensitive file systems", func(t *testing.T) {
		mapping := NodeModulesPathMapping{
			Patterns: []NodeModulesPathPattern{pattern(join("Test", "*.ts"), join("out-test", "*.luau"), false)},
		}

		got, ok := mapping.Lookup(join("TEST", "Helpers", "Styles.ts"), false)

		if want := join("out-test", "Helpers", "Styles.luau"); !ok || got != want {
			t.Errorf("Lookup = %q, %v; want %q", got, ok, want)
		}
	})

	t.Run("does not match when prefix and suffix overlap", func(t *testing.T) {
		mapping := NodeModulesPathMapping{
			Patterns: []NodeModulesPathPattern{pattern(join("a.d*.d.ts"), join("*.luau"), true)},
		}

		if got, ok := mapping.Lookup(join("a.d.ts"), true); ok {
			t.Errorf("Lookup = %q, want no match", got)
		}
	})
}
