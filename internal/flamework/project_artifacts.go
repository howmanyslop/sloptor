package flamework

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

func (p *Project) AddGlob(pattern, origin string) {
	p.buildInfo.AddGlob(pattern, origin)
}

func (p *Project) InvalidateGlobs(origin string) {
	p.buildInfo.InvalidateGlobs(origin)
}

func (p *Project) RuntimeArtifacts() ([]byte, []byte, error) {
	local := p.buildInfo.Snapshot()
	if local.Metadata != nil && local.Metadata.Globs != nil && local.Metadata.Globs.Paths != nil {
		patterns := sortedGlobPatterns(*local.Metadata.Globs.Paths)
		expanded, err := ExpandGlobs(p.rootDirectory, p.translator, patterns)
		if err != nil {
			return nil, nil, err
		}
		p.buildInfo.SetGlobPaths(expanded)
		local = p.buildInfo.Snapshot()
	}
	if !p.isGame {
		return nil, nil, nil
	}
	packages := p.buildInfo.PackageSnapshots()
	configPackages := make(map[string]RuntimeConfig)
	globPackages := make(map[string]GlobPaths)

	for _, pkg := range packages {
		prefix, ok := pkg.IdentifierPrefix()
		if !ok {
			continue
		}
		if pkg.Metadata != nil && pkg.Metadata.Config != nil {
			configPackages[prefix] = *cloneRuntimeConfig(pkg.Metadata.Config)
		}
		if pkg.Metadata == nil || pkg.Metadata.Globs == nil || pkg.Metadata.Globs.Paths == nil {
			continue
		}
		root, err := p.packageArtifactRoot(pkg.Path)
		if err != nil {
			return nil, nil, err
		}
		resolved, err := ResolveGlobPaths(root, p.resolver, cloneGlobMatches(*pkg.Metadata.Globs.Paths))
		if err != nil {
			return nil, nil, err
		}
		globPackages[prefix] = resolved
	}

	configJSON, _, err := MarshalRuntimeConfigArtifact(runtimeConfigFromMetadata(local.Metadata), configPackages)
	if err != nil {
		return nil, nil, err
	}

	var gameGlobs *GlobPaths
	if local.Metadata != nil && local.Metadata.Globs != nil {
		expanded := map[string][]string{}
		if local.Metadata.Globs.Paths != nil {
			expanded = cloneGlobMatches(*local.Metadata.Globs.Paths)
		}
		resolved, resolveErr := ResolveGlobPaths(p.rootDirectory, p.resolver, expanded)
		if resolveErr != nil {
			return nil, nil, resolveErr
		}
		if p.config.Obfuscation {
			resolved, resolveErr = p.obfuscateGlobPatterns(resolved)
			if resolveErr != nil {
				return nil, nil, resolveErr
			}
		}
		gameGlobs = &resolved
	}
	globsJSON, _, err := MarshalGlobsArtifact(gameGlobs, globPackages)
	if err != nil {
		return nil, nil, err
	}
	return configJSON, globsJSON, nil
}

func (p *Project) obfuscateGlobPatterns(paths GlobPaths) (GlobPaths, error) {
	obfuscated := make(GlobPaths, len(paths))
	for pattern, matches := range paths {
		hash, err := p.HashString(pattern, "addPaths")
		if err != nil {
			return nil, err
		}
		obfuscated[hash] = matches
	}
	return obfuscated, nil
}

func (p *Project) Persist() error {
	configJSON, globsJSON, err := p.RuntimeArtifacts()
	if err != nil {
		return err
	}
	return p.PersistArtifacts(configJSON, globsJSON)
}

func runtimeConfigFromMetadata(metadata *BuildMetadata) *RuntimeConfig {
	if metadata == nil {
		return nil
	}
	return cloneRuntimeConfig(metadata.Config)
}

func sortedGlobPatterns(paths map[string][]string) []string {
	patterns := make([]string, 0, len(paths))
	for pattern := range paths {
		patterns = append(patterns, pattern)
	}
	sort.Strings(patterns)
	return patterns
}

func cloneGlobMatches(paths map[string][]string) map[string][]string {
	cloned := make(map[string][]string, len(paths))
	for pattern, matches := range paths {
		cloned[pattern] = append([]string(nil), matches...)
	}
	return cloned
}

func (p *Project) packageArtifactRoot(buildInfoPath string) (string, error) {
	root := filepath.Dir(buildInfoPath)
	relative, err := filepath.Rel(p.rootDirectory, root)
	if err == nil && relative != ".." && !strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
		return root, nil
	}
	packageJSONPath := findPackageJSON(root)
	if packageJSONPath == "" {
		return "", fmt.Errorf("%w for package build info %s", ErrPackageNotFound, buildInfoPath)
	}
	data, err := os.ReadFile(packageJSONPath)
	if err != nil {
		return "", fmt.Errorf("read package identity for build info: %w", err)
	}
	var identity struct {
		Name string `json:"name"`
	}
	if err := json.Unmarshal(data, &identity); err != nil || identity.Name == "" {
		return "", fmt.Errorf("%w at %s", ErrInvalidPackage, packageJSONPath)
	}
	return filepath.Join(p.rootDirectory, "node_modules", filepath.FromSlash(identity.Name)), nil
}
