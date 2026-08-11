package flamework

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"sort"
	"strings"

	"rotor/internal/rojo"
)

var ErrInvalidGlob = errors.New("flamework: invalid glob")

// GlobPaths is the runtime pathglob mapping emitted for one project.
type GlobPaths map[string][]rojo.RbxPath

// ExpandGlobs reproduces Flamework's case-insensitive project-root glob pass,
// translating each matching input through the project's PathTranslator.
func ExpandGlobs(root string, translator *rojo.PathTranslator, patterns []string) (map[string][]string, error) {
	cleanPatterns := make([]string, len(patterns))
	for index, pattern := range patterns {
		clean, err := cleanGlobPattern(pattern)
		if err != nil {
			return nil, err
		}
		cleanPatterns[index] = clean
	}

	files := make([]string, 0)
	err := fs.WalkDir(os.DirFS(root), ".", func(name string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.Type().IsRegular() {
			files = append(files, name)
		}
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("flamework: expand globs in %q: %w", root, err)
	}
	sort.Strings(files)

	expanded := make(map[string][]string, len(patterns))
	for index, pattern := range patterns {
		matches := make([]string, 0)
		for _, name := range files {
			matched, err := matchGlob(cleanPatterns[index], filepath.ToSlash(name))
			if err != nil {
				return nil, fmt.Errorf("%w %q: %w", ErrInvalidGlob, pattern, err)
			}
			if !matched {
				continue
			}
			output, err := TranslateOutputPath(root, translator, name)
			if err != nil {
				return nil, fmt.Errorf("flamework: translate glob %q match %q: %w", pattern, name, err)
			}
			matches = append(matches, output)
		}
		expanded[pattern] = matches
	}
	return expanded, nil
}

// ResolveGlobPaths maps translated project-relative outputs into Rojo paths.
// Matches outside the Rojo tree are omitted, as in upstream convertGlobs.
func ResolveGlobPaths(root string, resolver *rojo.RojoResolver, expanded map[string][]string) (GlobPaths, error) {
	resolved := make(GlobPaths, len(expanded))
	for pattern, matches := range expanded {
		rbxPaths := make([]rojo.RbxPath, 0, len(matches))
		for _, match := range matches {
			filePath, err := projectPath(root, match)
			if err != nil {
				return nil, fmt.Errorf("flamework: resolve glob %q match %q: %w", pattern, match, err)
			}
			if resolver == nil {
				continue
			}
			if rbxPath, ok := resolver.GetRbxPathFromFilePath(filePath); ok {
				rbxPaths = append(rbxPaths, append(rojo.RbxPath(nil), rbxPath...))
			}
		}
		resolved[pattern] = rbxPaths
	}
	return resolved, nil
}

// MarshalGlobsArtifact builds deterministic include/flamework/globs.json
// bytes without writing them.
func MarshalGlobsArtifact(game *GlobPaths, packages map[string]GlobPaths) ([]byte, bool, error) {
	if game == nil && len(packages) == 0 {
		return nil, false, nil
	}
	gamePaths := GlobPaths{}
	if game != nil {
		gamePaths = *game
	}
	payload := struct {
		Game     GlobPaths            `json:"game"`
		Packages map[string]GlobPaths `json:"packages"`
	}{Game: gamePaths, Packages: packages}
	if payload.Packages == nil {
		payload.Packages = map[string]GlobPaths{}
	}
	data, err := json.Marshal(payload)
	if err != nil {
		return nil, false, fmt.Errorf("flamework: marshal globs artifact: %w", err)
	}
	return data, true, nil
}

func cleanGlobPattern(pattern string) (string, error) {
	normalized := strings.ReplaceAll(pattern, "\\", "/")
	if path.IsAbs(normalized) {
		return "", fmt.Errorf("%w: %q", ErrPathEscape, pattern)
	}
	clean := path.Clean(normalized)
	if clean == ".." || strings.HasPrefix(clean, "../") {
		return "", fmt.Errorf("%w: %q", ErrPathEscape, pattern)
	}
	clean = strings.TrimPrefix(clean, "./")
	for _, segment := range strings.Split(clean, "/") {
		if segment == "**" {
			continue
		}
		if _, err := path.Match(segment, ""); err != nil {
			return "", fmt.Errorf("%w %q: %w", ErrInvalidGlob, pattern, err)
		}
	}
	return clean, nil
}

func matchGlob(pattern, name string) (bool, error) {
	return matchGlobSegments(strings.Split(strings.ToLower(pattern), "/"), strings.Split(strings.ToLower(name), "/"))
}

func matchGlobSegments(pattern, name []string) (bool, error) {
	if len(pattern) == 0 {
		return len(name) == 0, nil
	}
	if pattern[0] == "**" {
		matched, err := matchGlobSegments(pattern[1:], name)
		if err != nil || matched || len(name) == 0 || strings.HasPrefix(name[0], ".") {
			return matched, err
		}
		return matchGlobSegments(pattern, name[1:])
	}
	if len(name) == 0 || (strings.HasPrefix(name[0], ".") && !strings.HasPrefix(pattern[0], ".")) {
		return false, nil
	}
	matched, err := path.Match(pattern[0], name[0])
	if err != nil || !matched {
		return false, err
	}
	return matchGlobSegments(pattern[1:], name[1:])
}
