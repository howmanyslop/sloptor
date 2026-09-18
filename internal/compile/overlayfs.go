package compile

import (
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/vfs"
	"rotor/tsgo/vfs/cachedvfs"
	"rotor/tsgo/vfs/osvfs"
	"rotor/tsgo/vfs/wrapvfs"
)

// normalizeOverlays rekeys a caller-supplied overlay map into the form
// newOverlayFS looks up: slash-normalized, and lowercased on case-insensitive
// filesystems. Callers pass ordinary absolute paths and stay unaware of it.
func normalizeOverlays(overlays map[string]string) map[string]string {
	caseSensitive := osvfs.FS().UseCaseSensitiveFileNames()
	out := make(map[string]string, len(overlays))
	for path, text := range overlays {
		out[normalizeOverlayPath(path, caseSensitive)] = text
	}
	return out
}

// overlayAliases expands the small caller-provided overlay set once, before a
// compiler host starts probing the filesystem. The hot FileExists/ReadFile
// path remains lexical: module resolution may make thousands of probes per
// build, most of which have nothing to do with an overlay.
func overlayAliases(overlays map[string]string, configPath string, caseSensitive bool) map[string]string {
	if len(overlays) == 0 {
		return nil
	}
	aliases := make(map[string]string, len(overlays)*2)
	configDir := filepath.Dir(filepath.FromSlash(configPath))
	canonicalConfigDir, configIsPhysical := filepath.EvalSymlinks(configDir)
	for path, text := range overlays {
		aliases[normalizeOverlayPath(path, caseSensitive)] = text
		if physical, ok := canonicalExistingOverlayPath(path, caseSensitive); ok {
			aliases[physical] = text
			if configIsPhysical == nil {
				relative, err := filepath.Rel(canonicalConfigDir, filepath.FromSlash(physical))
				if err == nil && relative != ".." && !strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
					// Config parsing may probe an include path before it has made it
					// absolute, while later source reads use the lexical config root.
					aliases[normalizeOverlayPath(relative, caseSensitive)] = text
					aliases[normalizeOverlayPath(filepath.Join(configDir, relative), caseSensitive)] = text
				}
			}
		}
	}
	return aliases
}

// overlayText first reads the caller-owned lexical map. Apart from keeping
// path lookup cheap, that preserves virtual overlays added after a prior
// negative filesystem lookup. Physical aliases are fixed at host setup and
// cover existing files spelled through a symlinked parent.
func overlayText(overlays, aliases map[string]string, path string, caseSensitive bool) (string, bool) {
	path = normalizeOverlayPath(path, caseSensitive)
	if text, ok := overlays[path]; ok {
		return text, true
	}
	text, ok := aliases[path]
	return text, ok
}

// canonicalExistingOverlayPath resolves only a complete, existing absolute
// target. A missing target remains lexical so the overlay-match guard reports
// it instead of accepting a typo through a symlinked parent directory.
func canonicalExistingOverlayPath(path string, caseSensitive bool) (string, bool) {
	path = normalizeOverlayPath(path, caseSensitive)
	if !filepath.IsAbs(path) {
		return "", false
	}
	resolved, err := filepath.EvalSymlinks(path)
	if err != nil {
		return "", false
	}
	return normalizeOverlayPath(resolved, caseSensitive), true
}

// matchOverlaysToProgram counts the overlay keys that name a source file the
// program actually holds, and rejects the run outright if any names nothing.
//
// Silence is the failure mode this exists to prevent. An overlay FS only
// overrides FileExists and ReadFile, so a key the program never asks about —
// a relative path, a typo, a file outside the tsconfig include set — changes
// nothing and yields a clean report on the UNMODIFIED tree. A consumer
// comparing a before and an after cannot tell that apart from a real pass.
func matchOverlaysToProgram(program *compiler.Program, overlays map[string]string) (int, error) {
	matched, unmatched := overlayKeysInProgram(program, overlays)
	if len(unmatched) > 0 {
		sort.Strings(unmatched)
		return 0, fmt.Errorf("compile: overlay matches no file in the program: %s", strings.Join(unmatched, ", "))
	}
	return len(matched), nil
}

// matchSolutionOverlaysToProgram is matchOverlaysToProgram for one project of a
// solution census. A solution's files are split between its projects, so "every
// overlay must match" is only answerable against the union of all of them: this
// records what one project matched, and the caller's solutionOverlayMatches
// checks the union once, after every project has run.
func matchSolutionOverlaysToProgram(program *compiler.Program, overlays map[string]string, tracker *solutionOverlayMatches) (int, error) {
	matched, _ := overlayKeysInProgram(program, overlays)
	tracker.record(matched)
	return len(matched), nil
}

// overlayKeysInProgram splits the overlay keys into those naming a source file
// the program holds (as a normalized set, so a caller can union them across
// projects) and those naming nothing (in their original spelling, for the error
// message).
func overlayKeysInProgram(program *compiler.Program, overlays map[string]string) (matched map[string]struct{}, unmatched []string) {
	caseSensitive := osvfs.FS().UseCaseSensitiveFileNames()
	inProgram := make(map[string]string, len(program.SourceFiles()))
	for _, sourceFile := range program.SourceFiles() {
		inProgram[normalizeOverlayPath(sourceFile.FileName(), caseSensitive)] = sourceFile.FileName()
	}

	matched = make(map[string]struct{}, len(overlays))
	var canonicalProgram map[string]string
	for path := range overlays {
		normalized := normalizeOverlayPath(path, caseSensitive)
		_, found := inProgram[normalized]
		if !found {
			if physical, ok := canonicalExistingOverlayPath(path, caseSensitive); ok {
				if canonicalProgram == nil {
					canonicalProgram = make(map[string]string, len(program.SourceFiles()))
					for _, sourceFile := range program.SourceFiles() {
						if canonical, exists := canonicalExistingOverlayPath(sourceFile.FileName(), caseSensitive); exists {
							canonicalProgram[canonical] = sourceFile.FileName()
						}
					}
				}
				_, found = canonicalProgram[physical]
			}
		}
		if !found {
			unmatched = append(unmatched, path)
			continue
		}
		matched[normalized] = struct{}{}
	}
	return matched, unmatched
}

// rekeyOverlaysToProgram turns caller spellings into the exact file names the
// already-parsed program reads. This is a bounded setup step for an overlay
// build, so the virtual filesystem can keep every subsequent probe lexical.
func rekeyOverlaysToProgram(program *compiler.Program, overlays map[string]string) (map[string]string, []string, error) {
	caseSensitive := osvfs.FS().UseCaseSensitiveFileNames()
	inProgram := make(map[string]string, len(program.SourceFiles()))
	for _, sourceFile := range program.SourceFiles() {
		inProgram[normalizeOverlayPath(sourceFile.FileName(), caseSensitive)] = sourceFile.FileName()
	}

	resolved := make(map[string]string, len(overlays))
	origins := make(map[string]string, len(overlays))
	var canonicalProgram map[string]string
	var unmatched []string
	paths := make([]string, 0, len(overlays))
	for path := range overlays {
		paths = append(paths, path)
	}
	sort.Strings(paths)
	for _, path := range paths {
		text := overlays[path]
		fileName, found := inProgram[normalizeOverlayPath(path, caseSensitive)]
		if !found {
			if physical, ok := canonicalExistingOverlayPath(path, caseSensitive); ok {
				if canonicalProgram == nil {
					canonicalProgram = make(map[string]string, len(program.SourceFiles()))
					for _, sourceFile := range program.SourceFiles() {
						if canonical, exists := canonicalExistingOverlayPath(sourceFile.FileName(), caseSensitive); exists {
							canonicalProgram[canonical] = sourceFile.FileName()
						}
					}
				}
				fileName, found = canonicalProgram[physical]
			}
		}
		if !found {
			unmatched = append(unmatched, path)
			continue
		}
		key := normalizeOverlayPath(fileName, caseSensitive)
		if previous, exists := resolved[key]; exists && previous != text {
			return nil, nil, fmt.Errorf("compile: conflicting overlays name the same file: %s and %s", origins[key], path)
		}
		resolved[key] = text
		origins[key] = path
	}
	return resolved, unmatched, nil
}

func newOverlayFS(rawBase vfs.FS, configPath string, overlays map[string]string) vfs.FS {
	return newOverlayFSWithConfigRead(rawBase, configPath, overlays, nil)
}

func newOverlayFSWithConfigRead(rawBase vfs.FS, configPath string, overlays map[string]string, onConfigRead func(string, string)) vfs.FS {
	baseFS := cachedvfs.From(SanitizeFSWithConfigRead(bundled.WrapFS(rawBase), configPath, onConfigRead))
	caseSensitive := baseFS.UseCaseSensitiveFileNames()
	aliases := overlayAliases(overlays, configPath, caseSensitive)
	return wrapvfs.Wrap(baseFS, wrapvfs.Replacements{
		FileExists: func(path string) bool {
			if _, ok := overlayText(overlays, aliases, path, caseSensitive); ok {
				return true
			}
			return baseFS.FileExists(path)
		},
		ReadFile: func(path string) (string, bool) {
			if text, ok := overlayText(overlays, aliases, path, caseSensitive); ok {
				return text, true
			}
			return baseFS.ReadFile(path)
		},
	})
}
