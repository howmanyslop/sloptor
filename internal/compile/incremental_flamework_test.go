package compile

import (
	"encoding/json"
	"os"
	"path/filepath"
	"slices"
	"testing"

	"rotor/internal/config"
	"rotor/internal/flamework"
)

func TestFlameworkIncrementalSaltDeterministicAcrossInputOrder(t *testing.T) {
	// Given: equivalent effective Flamework inputs in different discovery order.
	limit := 0
	first := &FlameworkIncrementalInputs{
		EffectiveConfig: config.FlameworkConfig{
			IDGenerationMode: "full",
			Optimizations: config.FlameworkOptimizations{
				GuardGenerationDedupLimit: &limit,
			},
		},
		TransformerVersion: "1.3.2-native",
		EffectivePlugins: []json.RawMessage{
			json.RawMessage(`{"transform":"first","z":1,"a":2}`),
			json.RawMessage(`{"transform":"second"}`),
		},
		PackageInputs: []FlameworkIncrementalFile{
			{Path: `node_modules\\@scope\\two\\package.json`, Contents: []byte(`{"name":"@scope/two"}`)},
			{Path: "node_modules/@scope/one/package.json", Contents: []byte(`{"name":"@scope/one"}`)},
		},
		BuildInfoInputs: []FlameworkIncrementalFile{
			{Path: "node_modules/@scope/two/flamework.build", Contents: []byte(`{"version":2}`)},
			{Path: "node_modules/@scope/one/flamework.build", Contents: []byte(`{"version":1}`)},
		},
		RelevantGlobs: []FlameworkIncrementalGlob{
			{Pattern: `src\\server\\**\\*.ts`, Matches: []string{`src\\server\\main.ts`}},
			{Pattern: "src/shared/**/*.ts", Matches: []string{"src/shared/two.ts", "src/shared/one.ts"}},
		},
	}
	second := &FlameworkIncrementalInputs{
		EffectiveConfig:    first.EffectiveConfig,
		TransformerVersion: first.TransformerVersion,
		EffectivePlugins: []json.RawMessage{
			json.RawMessage(`{"a":2,"transform":"first","z":1}`),
			json.RawMessage(`{"transform":"second"}`),
		},
		PackageInputs:   slices.Clone(first.PackageInputs),
		BuildInfoInputs: slices.Clone(first.BuildInfoInputs),
		RelevantGlobs: []FlameworkIncrementalGlob{
			{Pattern: "src/shared/**/*.ts", Matches: []string{"src/shared/one.ts", "src/shared/two.ts"}},
			{Pattern: "src/server/**/*.ts", Matches: []string{"src/server/main.ts"}},
		},
	}
	slices.Reverse(second.PackageInputs)
	slices.Reverse(second.BuildInfoInputs)

	// When: both salt contributions are calculated.
	firstSalt, err := flameworkIncrementalSalt(first)
	if err != nil {
		t.Fatal(err)
	}
	secondSalt, err := flameworkIncrementalSalt(second)
	if err != nil {
		t.Fatal(err)
	}

	// Then: the output hashes are byte-for-byte deterministic.
	if firstSalt != secondSalt {
		t.Fatalf("equivalent Flamework salts differ: %s != %s", firstSalt, secondSalt)
	}
}

func TestFlameworkIncrementalSaltChangesForEveryTransformerInput(t *testing.T) {
	// Given: one effective native Flamework input snapshot.
	base := &FlameworkIncrementalInputs{
		EffectiveConfig:    config.FlameworkConfig{IDGenerationMode: "full"},
		TransformerVersion: "1.3.2-native",
		EffectivePlugins:   []json.RawMessage{json.RawMessage(`{"transform":"plugin","option":true}`)},
		RuntimeConfig:      []byte(`{"profiling":false}`),
		PackageInputs: []FlameworkIncrementalFile{
			{Path: "package.json", Contents: []byte(`{"name":"fixture","version":"1.0.0"}`)},
		},
		BuildInfoInputs: []FlameworkIncrementalFile{
			{Path: "node_modules/@scope/pkg/flamework.build", Contents: []byte(`{"version":1}`)},
		},
		RelevantGlobs: []FlameworkIncrementalGlob{{Pattern: "src/**/*.ts", Matches: []string{"src/main.ts"}}},
	}
	baseSalt, err := flameworkIncrementalSalt(base)
	if err != nil {
		t.Fatal(err)
	}

	tests := map[string]func(*FlameworkIncrementalInputs){
		"effective config":    func(inputs *FlameworkIncrementalInputs) { inputs.EffectiveConfig.HashPrefix = "changed" },
		"transformer version": func(inputs *FlameworkIncrementalInputs) { inputs.TransformerVersion = "1.3.3-native" },
		"effective plugin config": func(inputs *FlameworkIncrementalInputs) {
			inputs.EffectivePlugins[0] = json.RawMessage(`{"transform":"plugin","option":false}`)
		},
		"effective plugin removal": func(inputs *FlameworkIncrementalInputs) { inputs.EffectivePlugins = nil },
		"runtime config":           func(inputs *FlameworkIncrementalInputs) { inputs.RuntimeConfig = []byte(`{"profiling":true}`) },
		"package state": func(inputs *FlameworkIncrementalInputs) {
			inputs.PackageInputs[0].Contents = []byte(`{"name":"fixture","version":"2.0.0"}`)
		},
		"package build info": func(inputs *FlameworkIncrementalInputs) { inputs.BuildInfoInputs[0].Contents = []byte(`{"version":2}`) },
		"glob pattern":       func(inputs *FlameworkIncrementalInputs) { inputs.RelevantGlobs[0].Pattern = "src/server/**/*.ts" },
		"glob matches": func(inputs *FlameworkIncrementalInputs) {
			inputs.RelevantGlobs[0].Matches = append(inputs.RelevantGlobs[0].Matches, "src/added.ts")
		},
	}
	for name, mutate := range tests {
		t.Run(name, func(t *testing.T) {
			changed := cloneFlameworkIncrementalInputs(base)
			mutate(changed)

			// When: one real transformer input changes.
			changedSalt, err := flameworkIncrementalSalt(changed)
			if err != nil {
				t.Fatal(err)
			}

			// Then: the observable cache salt changes.
			if changedSalt == baseSalt {
				t.Fatalf("%s change preserved salt %s", name, baseSalt)
			}
		})
	}
}

func cloneFlameworkIncrementalInputs(inputs *FlameworkIncrementalInputs) *FlameworkIncrementalInputs {
	cloned := *inputs
	cloned.RuntimeConfig = slices.Clone(inputs.RuntimeConfig)
	cloned.EffectivePlugins = slices.Clone(inputs.EffectivePlugins)
	cloned.PackageInputs = slices.Clone(inputs.PackageInputs)
	cloned.BuildInfoInputs = slices.Clone(inputs.BuildInfoInputs)
	cloned.RelevantGlobs = slices.Clone(inputs.RelevantGlobs)
	for index := range cloned.RelevantGlobs {
		cloned.RelevantGlobs[index].Matches = slices.Clone(inputs.RelevantGlobs[index].Matches)
	}
	return &cloned
}

func TestFlameworkIncrementalInputsExcludesGeneratedRootBuildState(t *testing.T) {
	// Given: a native project with one dependency build input and persisted root glob state.
	root := t.TempDir()
	writeFlameworkIncrementalFile(t, filepath.Join(root, "package.json"), `{"name":"fixture","version":"1.0.0"}`)
	writeFlameworkIncrementalFile(t, filepath.Join(root, "flamework.json"), `{"profiling":false}`)
	writeFlameworkIncrementalFile(t, filepath.Join(root, "flamework.build"), `{"version":1,"flameworkVersion":"1.3.2","identifiers":{"root":"one"},"metadata":{"globs":{"paths":{"src/**/*.ts":["src/main.ts"]},"origins":{}}}}`)
	dependency := filepath.Join(root, "node_modules", "@scope", "dependency")
	writeFlameworkIncrementalFile(t, filepath.Join(dependency, "package.json"), `{"name":"@scope/dependency","version":"1.0.0"}`)
	dependencyBuild := filepath.Join(dependency, "flamework.build")
	writeFlameworkIncrementalFile(t, dependencyBuild, `{"version":1,"flameworkVersion":"1.3.2","identifiers":{"dependency":"one"}}`)
	project, err := flamework.OpenProject(flamework.ProjectOptions{ProjectDir: root, RootDir: "src", OutDir: "out", Config: config.FlameworkConfig{IDGenerationMode: "full"}})
	if err != nil {
		t.Fatal(err)
	}
	if err := project.AddPackageBuildInfo(dependencyBuild); err != nil {
		t.Fatal(err)
	}
	state := &flameworkPipeline{config: &config.FlameworkConfig{IDGenerationMode: "full"}, project: project}
	before, err := flameworkIncrementalInputs(state)
	if err != nil {
		t.Fatal(err)
	}
	beforeSalt, err := flameworkIncrementalSalt(before)
	if err != nil {
		t.Fatal(err)
	}

	// When: only the generated root identifier cache changes.
	writeFlameworkIncrementalFile(t, filepath.Join(root, "flamework.build"), `{"version":1,"flameworkVersion":"1.3.2","identifiers":{"root":"two"},"metadata":{"globs":{"paths":{"src/**/*.ts":["src/main.ts"]},"origins":{}}}}`)
	after, err := flameworkIncrementalInputs(state)
	if err != nil {
		t.Fatal(err)
	}
	afterSalt, err := flameworkIncrementalSalt(after)
	if err != nil {
		t.Fatal(err)
	}

	// Then: the generated artifact does not self-invalidate, while dependency state remains salted.
	if afterSalt != beforeSalt {
		t.Fatalf("generated root build changed salt: %s != %s", afterSalt, beforeSalt)
	}
	writeFlameworkIncrementalFile(t, dependencyBuild, `{"version":1,"flameworkVersion":"1.3.2","identifiers":{"dependency":"two"}}`)
	dependencyChanged, err := flameworkIncrementalInputs(state)
	if err != nil {
		t.Fatal(err)
	}
	dependencySalt, err := flameworkIncrementalSalt(dependencyChanged)
	if err != nil {
		t.Fatal(err)
	}
	if dependencySalt == beforeSalt {
		t.Fatalf("dependency build state preserved salt %s", beforeSalt)
	}
}

func writeFlameworkIncrementalFile(t *testing.T, path, contents string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}
