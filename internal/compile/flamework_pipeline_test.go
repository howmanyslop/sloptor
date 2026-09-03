package compile

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"rotor/internal/config"
	"rotor/internal/transformer"
)

func TestEffectiveTransformerPluginsResolvesCompleteExtendsChain(t *testing.T) {
	// Given: the child inherits plugins through two config levels.
	dir := t.TempDir()
	writePipelineConfig(t, dir, "base.json", `{"compilerOptions":{"plugins":[{"transform":"prefix"},{"transform":"suffix"}]}}`)
	writePipelineConfig(t, dir, "middle.json", `{"extends":"./base.json","compilerOptions":{"strict":true}}`)
	child := writePipelineConfig(t, dir, "tsconfig.json", `{"extends":"./middle.json","include":["src"]}`)

	// When: the effective transformer list is resolved.
	plugins, err := effectiveTransformerPlugins(child)
	// Then: the inherited declaration order is preserved.
	if err != nil {
		t.Fatal(err)
	}
	if got, want := transformerPluginNames(plugins), []string{"prefix", "suffix"}; !reflect.DeepEqual(got, want) {
		t.Fatalf("effective plugins = %v, want %v", got, want)
	}
}

func TestSplitTransformerPluginsDefaultsNativeFirst(t *testing.T) {
	plugins := []transformerPluginConfig{{Transform: "first"}, {Transform: "second"}}

	prefix, suffix, err := splitTransformerPlugins(plugins, "")
	if err != nil {
		t.Fatal(err)
	}
	if len(prefix) != 0 || !reflect.DeepEqual(transformerPluginNames(suffix), []string{"first", "second"}) {
		t.Fatalf("split = (%v, %v), want native then all external plugins", transformerPluginNames(prefix), transformerPluginNames(suffix))
	}
}

func TestSplitTransformerPluginsAnchorsAfterUniqueEffectivePlugin(t *testing.T) {
	plugins := []transformerPluginConfig{{Transform: "prefix"}, {Transform: "anchor"}, {Transform: "suffix"}}

	prefix, suffix, err := splitTransformerPlugins(plugins, "anchor")
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(transformerPluginNames(prefix), []string{"prefix", "anchor"}) || !reflect.DeepEqual(transformerPluginNames(suffix), []string{"suffix"}) {
		t.Fatalf("split = (%v, %v), want ([prefix anchor], [suffix])", transformerPluginNames(prefix), transformerPluginNames(suffix))
	}
}

func TestSplitTransformerPluginsRejectsMissingDuplicateAndSelfAnchors(t *testing.T) {
	tests := []struct {
		name    string
		plugins []transformerPluginConfig
		after   string
		want    string
	}{
		{name: "missing", plugins: []transformerPluginConfig{{Transform: "present"}}, after: "missing", want: "does not match"},
		{name: "duplicate", plugins: []transformerPluginConfig{{Transform: "same"}, {Transform: "same"}}, after: "same", want: "matches 2"},
		{name: "self", after: legacyFlameworkTransformer, want: "cannot anchor"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			_, _, err := splitTransformerPlugins(test.plugins, test.after)
			if err == nil || !strings.Contains(err.Error(), test.want) {
				t.Fatalf("splitTransformerPlugins() error = %v, want containing %q", err, test.want)
			}
		})
	}
}

func TestComposeSourceTraceMapsMapsAcrossEveryStage(t *testing.T) {
	// Given: suffix -> native -> prefix -> authored mappings.
	authored := &sourceTraceMap{fileName: "main.ts", text: "authored", mappings: []traceMapping{{generatedLine: 3, generatedColumn: 0, sourceLine: 7, sourceColumn: 2}}}
	native := &sourceTraceMap{fileName: "main.ts", text: "prefix", mappings: []traceMapping{{generatedLine: 2, generatedColumn: 0, sourceLine: 3, sourceColumn: 0}}}
	suffix := &sourceTraceMap{fileName: "main.ts", text: "native", mappings: []traceMapping{{generatedLine: 1, generatedColumn: 0, sourceLine: 2, sourceColumn: 0}}}

	composed := composeSourceTraceMaps(suffix, composeSourceTraceMaps(native, authored))
	position := composed.OriginalPosition(transformer.SourcePosition{Line: 1, Column: 0})

	if position == nil || position.Line != 7 || position.Column != 2 {
		t.Fatalf("composed original = %+v, want 7:2", position)
	}
	if composed.OriginalSourceText() != "authored" {
		t.Fatalf("composed source text = %q, want authored", composed.OriginalSourceText())
	}
}

func TestFlameworkPipelineUsesConfiguredAfterOnRealBuildPath(t *testing.T) {
	// Given: native Flamework is enabled and anchored after a real external transform.
	dir := writeProject(t, "flamework-pipeline-real-build", "")
	setRepoSidecarPath(t)
	closeSidecarSessions()
	t.Cleanup(closeSidecarSessions)
	symlinkTransformerFixtureModules(t, dir)
	if err := os.WriteFile(filepath.Join(dir, "default.project.json"), []byte(`{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"},"rbxts_include":{"$path":"include","node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"},"@flamework":{"$path":"node_modules/@flamework"}}}}}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	plugins := `[{"transform":"./plugins/prefix.js","phase":"prefix"},{"transform":"./plugins/suffix.js","phase":"suffix"}]`
	configText := strings.Replace(sidecarDeclarationConfig(plugins), `"declaration": true,`, `"declaration": true, "experimentalDecorators": true,`, 1)
	configText = strings.Replace(configText, `"typeRoots": ["node_modules/@rbxts"]`, `"typeRoots": ["node_modules/@rbxts", "node_modules/@flamework"]`, 1)
	writeSidecarPluginFixture(t, dir, "", configText)
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("[flamework]\nafter = \"./plugins/prefix.js\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"prefix.js", "suffix.js"} {
		if err := os.WriteFile(filepath.Join(dir, "plugins", name), []byte(flameworkStagePlugin), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("import { Service } from \"@flamework/core\";\n@Service()\nexport class TestService {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: the actual build surface runs.
	timings := NewBuildTimings()
	result, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings})
	// Then: both the prefix plugin and native stage reached emitted Luau.
	if err != nil {
		t.Fatalf("BuildProjectWithOptions: %v (diags: %v)", err, diags)
	}
	if timings.Counts.SidecarStats == 0 {
		t.Fatal("sidecar stats = 0, want a disk snapshot on the first stage")
	}
	if timings.Counts.TotalSources > 0 && timings.Counts.SidecarSourceReads > 2*timings.Counts.TotalSources+8 {
		t.Fatalf("sidecar source reads = %d for %d sources, want at most one compile-file pass plus declaration reverts", timings.Counts.SidecarSourceReads, timings.Counts.TotalSources)
	}
	output := result.Outputs["out/main.luau"]
	prefix := strings.Index(output, "prefix-stage")
	native := strings.Index(output, "defineMetadata")
	suffix := strings.Index(output, "suffix-after-native")
	if prefix < 0 || native <= prefix || suffix <= native {
		t.Fatalf("stage order = prefix:%d native:%d suffix:%d, want strict order:\n%s", prefix, native, suffix, output)
	}
	if strings.Count(output, `"identifier"`) != 1 {
		t.Fatalf("native Flamework identifier metadata count = %d, want 1:\n%s", strings.Count(output, `"identifier"`), output)
	}
	declaration, err := os.ReadFile(filepath.Join(dir, "out", "main.d.ts"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Count(string(declaration), "prefix-declaration") != 1 || strings.Count(string(declaration), "suffix-declaration") != 1 || strings.Index(string(declaration), "prefix-declaration") > strings.Index(string(declaration), "suffix-declaration") {
		t.Fatalf("declaration transforms did not run once in plugin order:\n%s", declaration)
	}
}

func TestFlameworkPipelineComposesDiagnosticTraceAcrossEveryStage(t *testing.T) {
	// Given: prefix, native, and suffix stages each reprint the source before a TypeScript diagnostic is reported.
	dir := writeProject(t, "@scope/flamework-trace", "")
	setRepoSidecarPath(t)
	closeSidecarSessions()
	t.Cleanup(closeSidecarSessions)
	symlinkTransformerFixtureModules(t, dir)
	plugins := `[{"transform":"./plugins/prefix.js","phase":"prefix"},{"transform":"./plugins/suffix.js","phase":"suffix"}]`
	writeSidecarPluginFixture(t, dir, "", sidecarDeclarationConfig(plugins))
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("[flamework]\nafter = \"./plugins/prefix.js\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"prefix.js", "suffix.js"} {
		if err := os.WriteFile(filepath.Join(dir, "plugins", name), []byte(flameworkStagePlugin), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte(positionFixtureSource), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: the real build reaches its post-pipeline TypeScript diagnostics.
	result, _, err := BuildProjectWithOptions(dir, ProjectOptions{})

	// Then: TS2304 still indexes the authored token on disk.
	if err == nil || result == nil {
		t.Fatalf("BuildProjectWithOptions() = (%v, %v), want located diagnostic failure", result, err)
	}
	for _, diagnostic := range result.Diagnostics {
		if diagnostic.Code == "TS2304" {
			assertLocatesUseMountEffect(t, diagnostic)
			return
		}
	}
	t.Fatalf("TS2304 missing from diagnostics: %+v", result.Diagnostics)
}

func TestPrepareFlameworkPipelineAcceptsRootDirsWhenRootDirIsNull(t *testing.T) {
	// Given: native Flamework is enabled and tsconfig uses rootDirs with rootDir: null
	// (the test-project pattern: sources live under both src/ and test/).
	dir := task7FlameworkProject(t, "[flamework]\n")
	if err := os.Mkdir(filepath.Join(dir, "test"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "test", "spec.ts"), []byte("export const spec = 1;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	rewriteFlameworkRootDirs(t, dir)

	// When: the real compile pipeline opens native Flamework.
	result, diagnostics, err := BuildProjectWithOptions(dir, ProjectOptions{})

	// Then: rootDirs stands in for rootDir; OpenProject does not assert.
	if err != nil || len(diagnostics) != 0 {
		t.Fatalf("BuildProjectWithOptions = (%v, %v)", diagnostics, err)
	}
	if _, ok := result.Outputs["out/src/main.luau"]; !ok {
		t.Fatalf("outputs = %v, want out/src/main.luau", result.Outputs)
	}
	if _, ok := result.Outputs["out/test/spec.luau"]; !ok {
		t.Fatalf("outputs = %v, want out/test/spec.luau from rootDirs test/", result.Outputs)
	}
}

func rewriteFlameworkRootDirs(t *testing.T, dir string) {
	t.Helper()
	path := filepath.Join(dir, "tsconfig.json")
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	text := strings.Replace(string(data), `"rootDir": "src"`, `"rootDir": null, "rootDirs": ["src", "test"]`, 1)
	text = strings.Replace(text, `"include": ["src"]`, `"include": ["src", "test"]`, 1)
	if text == string(data) {
		t.Fatal("rewriteFlameworkRootDirs: tsconfig did not contain expected rootDir/include")
	}
	if err := os.WriteFile(path, []byte(text), 0o644); err != nil {
		t.Fatal(err)
	}
}

const flameworkStagePlugin = `const ts = require("typescript");
module.exports = function (_program, config) {
	const source = (context) => (sourceFile) => {
		let nativeMetadataPresent = false;
		function inspect(node) {
			if (ts.isStringLiteral(node) && node.text === "identifier") nativeMetadataPresent = true;
			ts.forEachChild(node, inspect);
		}
		inspect(sourceFile);
		const value = config.phase === "suffix" ? (nativeMetadataPresent ? "suffix-after-native" : "suffix-before-native") : "prefix-stage";
		const marker = context.factory.createVariableStatement(undefined, context.factory.createVariableDeclarationList([
			context.factory.createVariableDeclaration("__" + config.phase.toUpperCase() + "_STAGE__", undefined, undefined, context.factory.createStringLiteral(value)),
		], ts.NodeFlags.Const));
		return context.factory.updateSourceFile(sourceFile, config.phase === "prefix" ? [marker, ...sourceFile.statements] : [...sourceFile.statements, marker]);
	};
	const declaration = (context) => (sourceFile) => {
		const marker = context.factory.createVariableStatement(undefined, context.factory.createVariableDeclarationList([
			context.factory.createVariableDeclaration("__" + config.phase.toUpperCase() + "_DECLARATION__", undefined, undefined, context.factory.createStringLiteral(config.phase + "-declaration")),
		], ts.NodeFlags.Const));
		return context.factory.updateSourceFile(sourceFile, [...sourceFile.statements, marker]);
	};
	return { before: source, afterDeclarations: declaration };
};
`

func symlinkTransformerFixtureModules(t *testing.T, dir string) {
	t.Helper()
	fixture := filepath.Clean(filepath.Join(repoSidecarDir(t), "..", "..", "testdata", "transformers", "project", "node_modules"))
	if err := os.Symlink(fixture, filepath.Join(dir, "node_modules")); err != nil {
		t.Fatal(err)
	}
}

func writePipelineConfig(t *testing.T, dir, name, text string) string {
	t.Helper()
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, []byte(text), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func transformerPluginNames(plugins []transformerPluginConfig) []string {
	names := make([]string, len(plugins))
	for index, plugin := range plugins {
		names[index] = plugin.Transform
	}
	return names
}

var _ = config.FlameworkConfig{}

// An incremental build that selected nothing has nothing to transform and
// nothing to emit, so the pipeline must not reach the worker at all: the
// worker builds its whole LanguageService program before it can discover that
// the compile list is empty. The plugin named here does not exist, so any
// round trip fails loudly.
func TestRunCompilePipelineSkipsTheWorkerWhenNothingIsSelected(t *testing.T) {
	dir := writeProject(t, "@scope/empty-selection", "")
	projectDir, program, diags, err := newProjectProgram(dir, "")
	if err != nil {
		t.Fatalf("newProjectProgram: %v (diags: %v)", err, diags)
	}
	missing := []transformerPluginConfig{{Transform: "rotor-transformer-that-does-not-exist"}}
	pipeline := &flameworkPipeline{config: &config.FlameworkConfig{}, plugins: missing, prefix: missing, suffix: missing}

	result, diags, err := runCompilePipeline(projectDir, program, nil, nil, pipeline, nil)
	if err != nil {
		t.Fatalf("runCompilePipeline: %v (diags: %v)", err, diags)
	}
	if len(result.prepared.sourceFiles) != 0 {
		t.Fatalf("prepared %d source files, want none", len(result.prepared.sourceFiles))
	}
	if result.prepared.flamework != pipeline.config {
		t.Fatal("the empty pipeline dropped the Flamework config the rest of the build reads")
	}
	if len(result.prepared.declarations) != 0 {
		t.Fatalf("emitted %d declarations for an empty selection, want none", len(result.prepared.declarations))
	}
}
