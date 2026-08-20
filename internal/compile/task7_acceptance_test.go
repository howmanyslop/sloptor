package compile

import (
	"crypto/sha256"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"testing"

	"rotor/internal/config"
)

func TestTask7FlameworkCleanWarmIncrementalAndRecoveryAreDeterministic(t *testing.T) {
	// Given: an incremental native-only project with every non-ordering option set.
	dir := task7FlameworkProject(t, task7AllOptions)
	t.Setenv("ROTOR_NODE_PATH", filepath.Join(dir, "node-must-not-run"))

	// When: it builds clean, warm, after one source edit, fails, and resumes.
	cleanTimings := NewBuildTimings()
	task7Build(t, dir, cleanTimings)
	cleanHash := task7ArtifactHash(t, dir)
	task7Write(t, filepath.Join(dir, "out", "task7-unexpected.txt"), "unexpected\n")
	if task7ArtifactHash(t, dir) == cleanHash {
		t.Fatal("complete artifact hash ignored unexpected output file")
	}
	task7Remove(t, filepath.Join(dir, "out", "task7-unexpected.txt"))
	if task7ArtifactHash(t, dir) != cleanHash {
		t.Fatal("artifact hash did not recover after unexpected output removal")
	}
	warmTimings := NewBuildTimings()
	task7Build(t, dir, warmTimings)
	warmHash := task7ArtifactHash(t, dir)
	goodSource := "export const task7Value = 2;\n"
	task7Write(t, filepath.Join(dir, "src", "main.ts"), goodSource)
	incrementalTimings := NewBuildTimings()
	task7Build(t, dir, incrementalTimings)
	incrementalHash := task7ArtifactHash(t, dir)
	task7Write(t, filepath.Join(dir, "src", "main.ts"), "export const broken: = 1;\n")
	if _, diagnostics, err := BuildProjectWithOptions(dir, ProjectOptions{}); err == nil || len(diagnostics) == 0 {
		t.Fatalf("malformed build = (%v, %v), want failure with diagnostics", diagnostics, err)
	}
	failedHash := task7ArtifactHash(t, dir)
	task7Write(t, filepath.Join(dir, "src", "main.ts"), goodSource)
	resumeTimings := NewBuildTimings()
	task7Build(t, dir, resumeTimings)

	// Then: counts and complete artifact hashes prove deterministic publication.
	if got, want := []int{cleanTimings.Counts.SelectedSources, cleanTimings.Counts.ActualWrites}, []int{1, 1}; !reflect.DeepEqual(got, want) {
		t.Fatalf("clean selected/writes = %v, want %v", got, want)
	}
	if warmHash != cleanHash || warmTimings.Counts.SelectedSources != 0 || warmTimings.Counts.ActualWrites != 0 {
		t.Fatalf("warm hash/counts = %s/%d/%d, want %s/0/0", warmHash, warmTimings.Counts.SelectedSources, warmTimings.Counts.ActualWrites, cleanHash)
	}
	if incrementalHash == cleanHash || incrementalTimings.Counts.SelectedSources != 1 || incrementalTimings.Counts.ActualWrites != 1 {
		t.Fatalf("incremental hash/counts = %s/%d/%d, clean %s", incrementalHash, incrementalTimings.Counts.SelectedSources, incrementalTimings.Counts.ActualWrites, cleanHash)
	}
	if failedHash != incrementalHash || task7ArtifactHash(t, dir) != incrementalHash || resumeTimings.Counts.SelectedSources != 0 || resumeTimings.Counts.ActualWrites != 0 {
		t.Fatalf("failure/resume changed last-good artifacts or cache: failed=%s resumed=%s selected=%d writes=%d", failedHash, task7ArtifactHash(t, dir), resumeTimings.Counts.SelectedSources, resumeTimings.Counts.ActualWrites)
	}
}

func TestTask7FlameworkPublicOptionsAndDefaultsReachRealCompilerPipeline(t *testing.T) {
	// Given: native projects using the empty defaults and every public option.
	defaultDir := task7FlameworkProject(t, "[flamework]\n")
	configuredDir := task7FlameworkProject(t, task7AllOptions)
	t.Setenv("ROTOR_NODE_PATH", filepath.Join(t.TempDir(), "node-must-not-run"))

	// When: both projects build and their real compiler pipelines are prepared.
	task7Build(t, defaultDir, NewBuildTimings())
	task7Build(t, configuredDir, NewBuildTimings())
	defaultPipeline := task7Pipeline(t, defaultDir)
	configuredPipeline := task7Pipeline(t, configuredDir)

	// Then: upstream defaults and every typed public field arrive unchanged.
	if got := *defaultPipeline.config; got.IDGenerationMode != "full" || got.After != "" || got.Obfuscation || got.NoSemanticDiagnostics || got.HashPrefix != "" || got.Salt != "" || got.PreloadIDs || got.Optimizations.GuardGenerationDedupLimit != nil {
		t.Fatalf("default Flamework config = %+v", got)
	}
	if got, want := *configuredPipeline.config, task7ConfiguredOptions(0); !reflect.DeepEqual(got, want) {
		t.Fatalf("configured Flamework options = %+v, want %+v", got, want)
	}
	if configuredPipeline.project.IDMode() != "tiny" || configuredPipeline.project.HashPrefix() != "task7" || len(configuredPipeline.prefix) != 0 || len(configuredPipeline.suffix) != 0 {
		t.Fatalf("configured native pipeline = mode %q prefix %q stages %d/%d", configuredPipeline.project.IDMode(), configuredPipeline.project.HashPrefix(), len(configuredPipeline.prefix), len(configuredPipeline.suffix))
	}
	for _, mode := range []string{"full", "obfuscated", "short", "tiny"} {
		t.Run(mode, func(t *testing.T) {
			dir := task7FlameworkProject(t, "[flamework]\nidGenerationMode = \""+mode+"\"\nsalt = \"fixed\"\n")
			task7Build(t, dir, NewBuildTimings())
			if got := string(task7Pipeline(t, dir).project.IDMode()); got != mode {
				t.Fatalf("ID mode = %q, want %q", got, mode)
			}
		})
	}
	t.Run("obfuscation default", func(t *testing.T) {
		dir := task7FlameworkProject(t, "[flamework]\nobfuscation = true\nsalt = \"fixed\"\n")
		task7Build(t, dir, NewBuildTimings())
		if got := string(task7Pipeline(t, dir).project.IDMode()); got != "obfuscated" {
			t.Fatalf("obfuscation default ID mode = %q, want obfuscated", got)
		}
	})
}

func TestTask7FlameworkInvalidInputsFailClosedAndPreserveLastGoodArtifacts(t *testing.T) {
	invalid := []task7InvalidCase{
		{"malformed toml", "config: rotor.toml: toml: line 2: expected '.' or ']' to end table name, but got '\\n' instead\nerror: config: rotor.toml: toml: line 2: expected '.' or ']' to end table name, but got '\\n' instead", task7WriteMutation("rotor.toml", "[flamework\n")},
		{"unknown option", "config: rotor.toml: flamework: unknown key \"flamework.unknown\"\nerror: config: rotor.toml: flamework: unknown key \"flamework.unknown\"", task7WriteMutation("rotor.toml", "[flamework]\nunknown = true\n")},
		{"unknown optimization", "config: rotor.toml: flamework: unknown key \"flamework.optimizations.unknown\"\nerror: config: rotor.toml: flamework: unknown key \"flamework.optimizations.unknown\"", task7WriteMutation("rotor.toml", "[flamework.optimizations]\nunknown = 1\n")},
		{"invalid id mode", "config: flamework.idGenerationMode must be one of full, obfuscated, short, tiny; got \"random\"\nerror: compile: invalid Flamework configuration", task7WriteMutation("rotor.toml", "[flamework]\nidGenerationMode = \"random\"\n")},
		{"reserved hash prefix", "config: flamework.hashPrefix must not start with reserved prefix \"$\": \"$reserved\"\nerror: compile: invalid Flamework configuration", task7WriteMutation("rotor.toml", "[flamework]\nhashPrefix = \"$reserved\"\n")},
		{"negative dedup limit", "config: flamework.optimizations.guardGenerationDedupLimit must be >= 0, got -1\nerror: compile: invalid Flamework configuration", task7WriteMutation("rotor.toml", "[flamework.optimizations]\nguardGenerationDedupLimit = -1\n")},
		{"missing after anchor", "<ROOT>/rotor.toml: flamework.after \"missing-transformer\" does not match an effective tsconfig transformer plugin\nerror: compile: invalid flamework.after", task7WriteMutation("rotor.toml", "[flamework]\nafter = \"missing-transformer\"\n")},
		{"self after anchor", "<ROOT>/rotor.toml: flamework.after cannot anchor native Flamework to itself: \"rbxts-transformer-flamework\"\nerror: compile: invalid flamework.after", task7WriteMutation("rotor.toml", "[flamework]\nafter = \"rbxts-transformer-flamework\"\n")},
	}
	task7InvalidCases(t, invalid)
}

func TestTask7FlameworkInvalidProjectStateFailsClosedAndPreservesArtifacts(t *testing.T) {
	invalid := []task7InvalidCase{
		{"missing package", "package.json not found in <ROOT>\nerror: compile: open native Flamework project: package.json not found in <ROOT>", func(t *testing.T, dir string) { t.Helper(); task7Remove(t, filepath.Join(dir, "package.json")) }},
		{"invalid package", "invalid Flamework package.json at <ROOT>/package.json\nerror: compile: open native Flamework project: invalid Flamework package.json at <ROOT>/package.json", task7WriteMutation("package.json", "null\n")},
		{"invalid build info", "load project Flamework build info: flamework: invalid flamework.build: SyntaxError: Expected property name or '}' in JSON at position 2 (line 2 column 1)\nerror: compile: open native Flamework project: load project Flamework build info: flamework: invalid flamework.build: SyntaxError: Expected property name or '}' in JSON at position 2 (line 2 column 1)", task7WriteMutation("flamework.build", "{\n")},
		{"runtime nonobject", "flamework: invalid flamework.json: expected JSON object\nerror: compile: open native Flamework project: flamework: invalid flamework.json: expected JSON object", task7WriteMutation("flamework.json", "[]")},
		{"runtime trailing value", "flamework: invalid flamework.json: SyntaxError: invalid character '{' after top-level value in JSON at position 4 (line 1 column 5)\nerror: compile: open native Flamework project: flamework: invalid flamework.json: SyntaxError: invalid character '{' after top-level value in JSON at position 4 (line 1 column 5)", task7WriteMutation("flamework.json", "{} {}")},
		{"runtime log level", "Malformed flamework.json\nenum /logLevel: must be equal to one of the allowed values {\"allowedValues\":[\"none\",\"verbose\"]}\nerror: compile: open native Flamework project: Malformed flamework.json\nenum /logLevel: must be equal to one of the allowed values {\"allowedValues\":[\"none\",\"verbose\"]}", task7WriteMutation("flamework.json", `{"logLevel":"debug"}`)},
		{"runtime profiling", "Malformed flamework.json\ntype /profiling: must be boolean {\"type\":\"boolean\"}\nerror: compile: open native Flamework project: Malformed flamework.json\ntype /profiling: must be boolean {\"type\":\"boolean\"}", task7WriteMutation("flamework.json", `{"profiling":"yes"}`)},
		{"runtime dependency warnings", "Malformed flamework.json\ntype /disableDependencyWarnings: must be boolean {\"type\":\"boolean\"}\nerror: compile: open native Flamework project: Malformed flamework.json\ntype /disableDependencyWarnings: must be boolean {\"type\":\"boolean\"}", task7WriteMutation("flamework.json", `{"disableDependencyWarnings":1}`)},
		{"native and legacy plugin", "[flamework] cannot be combined with the legacy rbxts-transformer-flamework plugin; remove it from tsconfig.json\nerror: [flamework] cannot be combined with the legacy rbxts-transformer-flamework plugin; remove it from tsconfig.json", func(t *testing.T, dir string) {
			t.Helper()
			task7SetPlugins(t, dir, `[{"transform":"rbxts-transformer-flamework"}]`)
		}},
		{"duplicate after anchor", "<ROOT>/rotor.toml: flamework.after \"duplicate\" matches 2 effective tsconfig transformer plugins; anchor must be unique\nerror: compile: invalid flamework.after", func(t *testing.T, dir string) {
			t.Helper()
			task7Write(t, filepath.Join(dir, "rotor.toml"), "[flamework]\nafter = \"duplicate\"\n")
			task7SetPlugins(t, dir, `[{"transform":"duplicate"},{"transform":"duplicate"}]`)
		}},
	}
	task7InvalidCases(t, invalid)
}

type task7InvalidCase struct {
	name, tuple string
	mutate      func(*testing.T, string)
}

func task7InvalidCases(t *testing.T, cases []task7InvalidCase) {
	t.Helper()
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			dir := task7FlameworkProject(t, "[flamework]\n")
			task7Build(t, dir, NewBuildTimings())
			test.mutate(t, dir)
			before := task7ArtifactHash(t, dir)
			_, diagnostics, err := BuildProjectWithOptions(dir, ProjectOptions{})
			got := task7DiagnosticTuple(dir, diagnostics, err)
			if err == nil || got != test.tuple {
				t.Fatalf("invalid build tuple = %q, want %q", got, test.tuple)
			}
			if after := task7ArtifactHash(t, dir); after != before {
				t.Fatalf("invalid build published artifacts: before=%s after=%s", before, after)
			}
		})
	}
}

const task7AllOptions = "[flamework]\nafter = \"\"\nnoSemanticDiagnostics = true\nobfuscation = true\nidGenerationMode = \"tiny\"\nhashPrefix = \"task7\"\nsalt = \"task7-fixed-salt\"\npreloadIds = true\n\n[flamework.optimizations]\nguardGenerationDedupLimit = 0\n"

func task7ConfiguredOptions(limit int) config.FlameworkConfig {
	return config.FlameworkConfig{After: "", NoSemanticDiagnostics: true, Obfuscation: true, IDGenerationMode: "tiny", HashPrefix: "task7", Salt: "task7-fixed-salt", PreloadIDs: true, Optimizations: config.FlameworkOptimizations{GuardGenerationDedupLimit: &limit}}
}

func task7FlameworkProject(t *testing.T, configText string) string {
	t.Helper()
	dir := writeProject(t, "@scope/task7-acceptance", "")
	enableIncrementalBuilds(t, dir)
	task7Write(t, filepath.Join(dir, "rotor.toml"), configText)
	return dir
}

func task7Pipeline(t *testing.T, dir string) *flameworkPipeline {
	t.Helper()
	_, program, diagnostics, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (%v)", err, diagnostics)
	}
	pipeline, diagnostics, err := prepareFlameworkPipeline(filepath.ToSlash(dir), program, ProjectOptions{})
	if err != nil || pipeline == nil {
		t.Fatalf("prepareFlameworkPipeline = (%v, %v, %v)", pipeline, diagnostics, err)
	}
	return pipeline
}

func task7Build(t *testing.T, dir string, timings *BuildTimings) {
	t.Helper()
	if _, diagnostics, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings}); err != nil || len(diagnostics) != 0 {
		t.Fatalf("BuildProjectWithOptions = (%v, %v)", diagnostics, err)
	}
}

func task7ArtifactHash(t *testing.T, dir string) string {
	t.Helper()
	var records []string
	err := filepath.WalkDir(dir, func(path string, entry fs.DirEntry, err error) error {
		if err != nil || !entry.Type().IsRegular() {
			return err
		}
		relative, err := filepath.Rel(dir, path)
		if err != nil {
			return err
		}
		relative = filepath.ToSlash(relative)
		if relative != "flamework.build" && !strings.HasPrefix(relative, "out/") && !strings.HasPrefix(relative, "include/") {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		sum := sha256.Sum256(data)
		records = append(records, fmt.Sprintf("%s %x", relative, sum))
		return nil
	})
	if err != nil {
		t.Fatalf("walk build-owned artifacts: %v", err)
	}
	sort.Strings(records)
	sum := sha256.Sum256([]byte(strings.Join(records, "\n")))
	return fmt.Sprintf("%x", sum)
}

func task7DiagnosticTuple(dir string, diagnostics []string, err error) string {
	parts := append([]string{}, diagnostics...)
	if err != nil {
		parts = append(parts, "error: "+err.Error())
	}
	value := strings.Join(parts, "\n")
	for _, root := range []string{dir, filepath.Clean(dir), filepath.ToSlash(dir)} {
		value = strings.ReplaceAll(value, root, "<ROOT>")
	}
	return strings.ReplaceAll(value, "<ROOT>\\", "<ROOT>/")
}

func task7Write(t *testing.T, path, contents string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}

func task7Remove(t *testing.T, path string) {
	t.Helper()
	if err := os.Remove(path); err != nil {
		t.Fatal(err)
	}
}

func task7WriteMutation(relative, contents string) func(*testing.T, string) {
	return func(t *testing.T, dir string) {
		t.Helper()
		task7Write(t, filepath.Join(dir, relative), contents)
	}
}

func task7SetPlugins(t *testing.T, dir, plugins string) {
	t.Helper()
	path := filepath.Join(dir, "tsconfig.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	updated := strings.Replace(string(data), `"compilerOptions": {`, `"compilerOptions": {"plugins": `+plugins+`,`, 1)
	task7Write(t, path, updated)
}
