package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path"
	"path/filepath"
	"slices"
	"strings"

	"rotor/internal/config"
	"rotor/internal/flamework"
)

// FlameworkIncrementalFile is one normalized file input consumed by the native transformer.
type FlameworkIncrementalFile struct {
	Path     string
	Contents []byte
}

// FlameworkIncrementalGlob is one transformer glob and its current matched paths.
type FlameworkIncrementalGlob struct {
	Pattern string
	Matches []string
}

// FlameworkIncrementalInputs is the effective native transformer state that contributes to Rotor's incremental salt.
type FlameworkIncrementalInputs struct {
	EffectiveConfig    config.FlameworkConfig
	TransformerVersion string
	EffectivePlugins   []json.RawMessage
	RuntimeConfig      []byte
	PackageInputs      []FlameworkIncrementalFile
	BuildInfoInputs    []FlameworkIncrementalFile
	RelevantGlobs      []FlameworkIncrementalGlob
}

type flameworkIncrementalFileState struct {
	Path string `json:"path"`
	Hash string `json:"hash"`
}

func flameworkIncrementalInputs(state *flameworkPipeline) (*FlameworkIncrementalInputs, error) {
	if state == nil {
		return nil, nil
	}
	project := state.project
	if _, _, err := project.RuntimeArtifacts(); err != nil {
		return nil, fmt.Errorf("refresh Flamework glob inputs: %w", err)
	}
	rootBuild := project.BuildInfoSnapshot()
	inputs := &FlameworkIncrementalInputs{
		EffectiveConfig:    *state.config,
		TransformerVersion: flamework.FlameworkVersion,
		EffectivePlugins:   rawTransformerPlugins(state.plugins),
		RelevantGlobs:      incrementalGlobs(rootBuild.Metadata),
	}
	if rootBuild.Metadata != nil && rootBuild.Metadata.Config != nil {
		runtimeConfig, err := json.Marshal(rootBuild.Metadata.Config)
		if err != nil {
			return nil, fmt.Errorf("marshal Flamework runtime config for incremental salt: %w", err)
		}
		inputs.RuntimeConfig = runtimeConfig
	}
	packagePath := filepath.Join(project.RootDirectory(), "package.json")
	packageData, err := os.ReadFile(packagePath)
	if err != nil {
		return nil, fmt.Errorf("read Flamework package input %q: %w", packagePath, err)
	}
	inputs.PackageInputs = append(inputs.PackageInputs, FlameworkIncrementalFile{Path: packagePath, Contents: packageData})
	for _, snapshot := range project.PackageBuildInfoSnapshots() {
		buildData, err := os.ReadFile(snapshot.Path)
		if err != nil {
			return nil, fmt.Errorf("read Flamework package build input %q: %w", snapshot.Path, err)
		}
		inputs.BuildInfoInputs = append(inputs.BuildInfoInputs, FlameworkIncrementalFile{Path: snapshot.Path, Contents: buildData})
		dependencyPackagePath := filepath.Join(filepath.Dir(snapshot.Path), "package.json")
		if dependencyPackageData, readErr := os.ReadFile(dependencyPackagePath); readErr == nil {
			inputs.PackageInputs = append(inputs.PackageInputs, FlameworkIncrementalFile{Path: dependencyPackagePath, Contents: dependencyPackageData})
		} else if !os.IsNotExist(readErr) {
			return nil, fmt.Errorf("read Flamework dependency package input %q: %w", dependencyPackagePath, readErr)
		}
	}
	return inputs, nil
}

func incrementalGlobs(metadata *flamework.BuildMetadata) []FlameworkIncrementalGlob {
	if metadata == nil || metadata.Globs == nil || metadata.Globs.Paths == nil {
		return nil
	}
	globs := make([]FlameworkIncrementalGlob, 0, len(*metadata.Globs.Paths))
	for pattern, matches := range *metadata.Globs.Paths {
		globs = append(globs, FlameworkIncrementalGlob{Pattern: pattern, Matches: slices.Clone(matches)})
	}
	return globs
}

func flameworkIncrementalSalt(inputs *FlameworkIncrementalInputs) (string, error) {
	if inputs == nil {
		return "", nil
	}
	effectivePlugins, err := normalizeEffectivePlugins(inputs.EffectivePlugins)
	if err != nil {
		return "", err
	}
	payload, err := json.Marshal(struct {
		Version            string                          `json:"version"`
		EffectiveConfig    config.FlameworkConfig          `json:"effectiveConfig"`
		TransformerVersion string                          `json:"transformerVersion"`
		EffectivePlugins   []json.RawMessage               `json:"effectivePlugins"`
		RuntimeConfigHash  string                          `json:"runtimeConfigHash"`
		PackageInputs      []flameworkIncrementalFileState `json:"packageInputs"`
		BuildInfoInputs    []flameworkIncrementalFileState `json:"buildInfoInputs"`
		RelevantGlobs      []FlameworkIncrementalGlob      `json:"relevantGlobs"`
	}{
		Version:            "rotor-flamework-incremental-v1",
		EffectiveConfig:    inputs.EffectiveConfig,
		TransformerVersion: inputs.TransformerVersion,
		EffectivePlugins:   effectivePlugins,
		RuntimeConfigHash:  incrementalContentHash(inputs.RuntimeConfig),
		PackageInputs:      normalizeFlameworkIncrementalFiles(inputs.PackageInputs),
		BuildInfoInputs:    normalizeFlameworkIncrementalFiles(inputs.BuildInfoInputs),
		RelevantGlobs:      normalizeFlameworkIncrementalGlobs(inputs.RelevantGlobs),
	})
	if err != nil {
		return "", err
	}
	return incrementalContentHash(payload), nil
}

func normalizeEffectivePlugins(plugins []json.RawMessage) ([]json.RawMessage, error) {
	normalized := make([]json.RawMessage, len(plugins))
	for index, raw := range plugins {
		var value any
		if err := json.Unmarshal(raw, &value); err != nil {
			return nil, fmt.Errorf("normalize effective transformer plugin: %w", err)
		}
		canonical, err := json.Marshal(value)
		if err != nil {
			return nil, fmt.Errorf("marshal effective transformer plugin: %w", err)
		}
		normalized[index] = canonical
	}
	return normalized, nil
}

func normalizeFlameworkIncrementalFiles(inputs []FlameworkIncrementalFile) []flameworkIncrementalFileState {
	states := make([]flameworkIncrementalFileState, len(inputs))
	for index, input := range inputs {
		states[index] = flameworkIncrementalFileState{
			Path: normalizeFlameworkIncrementalPath(input.Path),
			Hash: incrementalContentHash(input.Contents),
		}
	}
	slices.SortFunc(states, func(left, right flameworkIncrementalFileState) int {
		if byPath := strings.Compare(left.Path, right.Path); byPath != 0 {
			return byPath
		}
		return strings.Compare(left.Hash, right.Hash)
	})
	return states
}

func normalizeFlameworkIncrementalGlobs(globs []FlameworkIncrementalGlob) []FlameworkIncrementalGlob {
	normalized := make([]FlameworkIncrementalGlob, len(globs))
	for index, glob := range globs {
		matches := make([]string, len(glob.Matches))
		for matchIndex, match := range glob.Matches {
			matches[matchIndex] = normalizeFlameworkIncrementalPath(match)
		}
		slices.Sort(matches)
		normalized[index] = FlameworkIncrementalGlob{
			Pattern: normalizeFlameworkIncrementalPath(glob.Pattern),
			Matches: slices.Compact(matches),
		}
	}
	slices.SortFunc(normalized, func(left, right FlameworkIncrementalGlob) int {
		return strings.Compare(left.Pattern, right.Pattern)
	})
	return normalized
}

func normalizeFlameworkIncrementalPath(value string) string {
	return path.Clean(strings.ReplaceAll(value, `\`, "/"))
}

func incrementalContentHash(contents []byte) string {
	sum := sha256.Sum256(contents)
	return hex.EncodeToString(sum[:])
}
