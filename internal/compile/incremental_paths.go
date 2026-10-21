package compile

import (
	"path/filepath"
	"slices"
	"strings"

	"rotor/tsgo/collections"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tspath"
)

// Source identities always use the project anchor, so moving a cache between
// internal, external, and cross-volume locations cannot change the graph.
const incrementalProjectPathPrefix = "project://"

func resolveIncrementalSourcePath(projectDir, name string) string {
	if relative, ok := strings.CutPrefix(name, incrementalProjectPathPrefix); ok {
		return normalizeSourceFilePath(filepath.Join(projectDir, filepath.FromSlash(relative)))
	}
	return normalizeSourceFilePath(name)
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
// Without a public or hoisted alias, a transitive input retains its realpath
// and requires matching virtual-store names between restored builds.
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
	pathAfterSegments := func(name string, segments ...string) (string, bool) {
		_, suffix, found := strings.Cut("/"+filepath.ToSlash(canonical(name))+"/", "/"+strings.Join(segments, "/")+"/")
		return suffix, found
	}
	aliases := map[string][]string{}
	addAlias := func(real, link string) {
		key := canonical(real)
		link = normalizeSourceFilePath(link)
		if !slices.Contains(aliases[key], link) {
			aliases[key] = append(aliases[key], link)
		}
	}
	addHoistedPackageAlias := func(real, link string) {
		suffix, hoisted := pathAfterSegments(link, "node_modules", ".pnpm", "node_modules")
		if !hoisted {
			return
		}
		parts := strings.Split(strings.TrimSuffix(suffix, "/"), "/")
		packageParts := 1
		if strings.HasPrefix(parts[0], "@") {
			packageParts = 2
		}
		if len(parts) <= packageParts {
			return
		}
		// The compiler omits directory aliases under ignored .pnpm paths.
		// Infer this package root from its recorded file link, requiring the
		// path inside the package to agree before covering relative imports.
		for range len(parts) - packageParts {
			if canonical(filepath.Base(real)) != canonical(filepath.Base(link)) {
				return
			}
			real, link = filepath.Dir(real), filepath.Dir(link)
		}
		addAlias(real, link)
	}
	addAliases := func(real tspath.Path, links *collections.SyncSet[string]) bool {
		links.Range(func(link string) bool {
			addAlias(string(real), link)
			addHoistedPackageAlias(normalizeSourceFilePath(string(real)), normalizeSourceFilePath(link))
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
		// Workspace files already have stable project-relative identities.
		// A local node_modules store still needs its logical package link.
		relativeToProject := relativeIncrementalPath(projectDir, name)
		isProjectLocal := filepath.IsLocal(filepath.FromSlash(relativeToProject))
		_, inNodeModules := pathAfterSegments(relativeToProject, "node_modules")
		_, inVirtualStore := pathAfterSegments(relativeToProject, "node_modules", ".pnpm")
		if isProjectLocal && !inNodeModules {
			encoded[name] = best
			return best
		}
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
						candidateRelative := relativeIncrementalPath(projectDir, candidate)
						// A sibling dependency link can introduce its parent's
						// platform-dependent store name. Keep the realpath for
						// a local or workspace virtual store until a link escapes
						// that store; still walk this candidate to find one.
						storeSuffix, inStore := pathAfterSegments(candidateRelative, "node_modules", ".pnpm")
						// .pnpm/node_modules is a stable hidden hoist, rather
						// than a directory named for one physical store entry.
						if (isProjectLocal || inVirtualStore) && inStore && !strings.HasPrefix(storeSuffix, "node_modules/") {
							continue
						}
						if !foundLink || lessIncrementalPath(candidateRelative, bestLink) {
							best = relativeIncrementalPath(dir, candidate)
							bestLink = candidateRelative
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
