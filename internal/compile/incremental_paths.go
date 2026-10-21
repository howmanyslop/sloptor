package compile

import (
	"path/filepath"
	"strings"

	"rotor/tsgo/collections"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tspath"
)

// Source identities always use the project anchor, so moving a cache between
// internal, external, and cross-volume locations cannot change the graph.
const incrementalProjectPathPrefix = "project://"

func resolveIncrementalSourcePath(manifestPath, projectDir, name string) string {
	if relative, ok := strings.CutPrefix(name, incrementalProjectPathPrefix); ok {
		return normalizeSourceFilePath(filepath.Join(projectDir, filepath.FromSlash(relative)))
	}
	if filepath.IsAbs(filepath.FromSlash(name)) {
		return normalizeSourceFilePath(name)
	}
	return normalizeSourceFilePath(filepath.Join(filepath.Dir(manifestPath), filepath.FromSlash(name)))
}

func incrementalSourcePathEncoder(program *compiler.Program) func(string) string {
	encode := incrementalPathEncoder(program.GetCurrentDirectory(), program)
	return func(name string) string {
		relative := encode(name)
		if filepath.IsAbs(filepath.FromSlash(relative)) {
			return relative
		}
		return incrementalProjectPathPrefix + relative
	}
}

func relativeIncrementalPath(dir, name string) string {
	if template, ok := incrementalPathTemplate(name, func(prefix string) string { return relativeIncrementalPath(dir, prefix) }); ok {
		return template
	}
	if name == "" {
		return ""
	}
	name = filepath.FromSlash(name)
	if filepath.IsAbs(name) {
		if relative, err := filepath.Rel(filepath.FromSlash(dir), name); err == nil {
			name = relative
		}
	}
	return filepath.ToSlash(filepath.Clean(name))
}

// Reverse the compiler's known package symlinks instead of persisting pnpm's
// machine-specific store paths. Directory aliases also cover declarations
// reached through relative imports inside a linked package.
func incrementalPathEncoder(dir string, program *compiler.Program) func(string) string {
	projectDir := dir
	if program != nil {
		projectDir = program.GetCurrentDirectory()
	}
	canonical := func(name string) string {
		name = normalizeSourceFilePath(name)
		if program != nil && !program.UseCaseSensitiveFileNames() {
			name = strings.ToLower(name)
		}
		return name
	}
	aliases := map[string][]string{}
	addAliases := func(real tspath.Path, links *collections.SyncSet[string]) bool {
		links.Range(func(link string) bool {
			key := canonical(string(real))
			aliases[key] = append(aliases[key], normalizeSourceFilePath(link))
			return true
		})
		return true
	}
	if program != nil {
		cache := program.GetSymlinkCache()
		cache.FilesByRealpath().Range(addAliases)
		cache.DirectoriesByRealpath().Range(addAliases)
	}
	encoded := map[string]string{}
	var encode func(string) string
	encode = func(name string) string {
		if template, ok := incrementalPathTemplate(name, encode); ok {
			return template
		}
		if name == "" {
			return ""
		}
		if result, ok := encoded[name]; ok {
			return result
		}
		best := relativeIncrementalPath(dir, name)
		var bestLink string
		foundLink := false
		queue := []string{normalizeSourceFilePath(name)}
		seen := map[string]bool{queue[0]: true}
		// A dependency's link can itself live in another linked package.
		// Each round reverses one link, bounded by the known alias count.
		for depth := 0; depth < len(aliases) && len(queue) > 0; depth++ {
			var next []string
			for _, current := range queue {
				for ancestor := current; ; ancestor = filepath.Dir(ancestor) {
					for _, link := range aliases[canonical(ancestor)] {
						rel, _ := filepath.Rel(ancestor, current)
						candidate := filepath.Join(link, rel)
						if seen[candidate] {
							continue
						}
						seen[candidate] = true
						next = append(next, candidate)
						// Prefer a known package link to its realpath, even
						// when the manifest lives closer to the package store.
						relativeToProject := relativeIncrementalPath(projectDir, candidate)
						if !foundLink || lessIncrementalPath(relativeToProject, bestLink) {
							best = relativeIncrementalPath(dir, candidate)
							bestLink = relativeToProject
							foundLink = true
						}
					}
					if filepath.Dir(ancestor) == ancestor {
						break
					}
				}
			}
			queue = next
		}
		encoded[name] = best
		return best
	}
	return encode
}

// Only clean the part before the first substitution. Cleaning */../ changes
// the meaning when * is replaced by several path components.
func incrementalPathTemplate(name string, encodePrefix func(string) string) (string, bool) {
	name = strings.ReplaceAll(name, `\`, "/")
	prefix, suffix, ok := strings.Cut(name, "*")
	if !ok {
		return "", false
	}
	encoded := encodePrefix(prefix)
	if strings.HasSuffix(prefix, "/") && !strings.HasSuffix(encoded, "/") {
		encoded += "/"
	}
	return encoded + "*" + suffix, true
}

func lessIncrementalPath(left, right string) bool {
	if a, b := filepath.IsAbs(filepath.FromSlash(left)), filepath.IsAbs(filepath.FromSlash(right)); a != b {
		return !a
	}
	// Prefer links nearest the project, then pick deterministically when a
	// package has several aliases. Map iteration order must not affect JSON.
	parents := func(name string) int {
		count := 0
		for strings.HasPrefix(name, "../") {
			count++
			name = strings.TrimPrefix(name, "../")
		}
		return count
	}
	if a, b := parents(left), parents(right); a != b {
		return a < b
	}
	if len(left) != len(right) {
		return len(left) < len(right)
	}
	return left < right
}
