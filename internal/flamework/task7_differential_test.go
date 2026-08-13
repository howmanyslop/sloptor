package flamework

// allow: SIZE_OK — this single task-owned file is the executable two-compiler differential harness.

import (
	"bufio"
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/csv"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"testing"
	"time"

	"rotor/internal/config"
	"rotor/internal/luau/render"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs/osvfs"
)

type task7Case struct {
	name     string
	plugin   string
	config   config.FlameworkConfig
	seedSalt bool
}

var (
	task7ANSI               = regexp.MustCompile(`\x1b\[[0-9;]*m`)
	task7OracleDiagnostic   = regexp.MustCompile(`TS @flamework/core: ([^\n]+)`)
	task7SemanticDiagnostic = regexp.MustCompile(`(?m)error TS([0-9]+): (.+)$`)
)

func TestTask7Differential_matchesRealUpstreamFinalLuauDiagnosticsAndArtifacts(t *testing.T) {
	// Given: a fresh real v1.3.2 npm installation and a controlled behavior/option matrix.
	fixture := filepath.Join("testdata", "task7-differential", "project")
	install := t.TempDir()
	copyTask7Tree(t, fixture, install)
	runTask7Command(t, install, 90*time.Second, "npm", "install", "--ignore-scripts", "--package-lock=false", "--no-audit", "--no-fund", "--prefer-offline")
	version := runTask7Command(t, install, 10*time.Second, "node", "-e", `process.stdout.write(require("rbxts-transformer-flamework/package.json").version)`)
	if version != FlameworkVersion {
		t.Fatalf("oracle transformer version = %q, want %q", version, FlameworkVersion)
	}
	limit0, limit2 := 0, 2
	cases := []task7Case{
		{name: "defaults", plugin: "default", config: config.FlameworkConfig{}},
		{name: "no-semantic-diagnostics", plugin: `"noSemanticDiagnostics":true`, config: config.FlameworkConfig{}},
		{name: "full", plugin: `"salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"full","optimizations":{"guardGenerationDedupLimit":2}`, config: config.FlameworkConfig{Salt: "task7-controlled-salt", HashPrefix: "task7", IDGenerationMode: "full", Optimizations: config.FlameworkOptimizations{GuardGenerationDedupLimit: &limit2}}},
		{name: "short", plugin: `"salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"short","preloadIds":false,"optimizations":{"guardGenerationDedupLimit":0}`, config: config.FlameworkConfig{Salt: "task7-controlled-salt", HashPrefix: "task7", IDGenerationMode: "short", Optimizations: config.FlameworkOptimizations{GuardGenerationDedupLimit: &limit0}}},
		{name: "short-preload", plugin: `"salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"short","preloadIds":true,"optimizations":{"guardGenerationDedupLimit":0}`, config: config.FlameworkConfig{Salt: "task7-controlled-salt", HashPrefix: "task7", IDGenerationMode: "short", PreloadIDs: true, Optimizations: config.FlameworkOptimizations{GuardGenerationDedupLimit: &limit0}}},
		{name: "tiny", plugin: `"salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"tiny","optimizations":{"guardGenerationDedupLimit":2}`, config: config.FlameworkConfig{Salt: "task7-controlled-salt", HashPrefix: "task7", IDGenerationMode: "tiny", Optimizations: config.FlameworkOptimizations{GuardGenerationDedupLimit: &limit2}}},
		{name: "obfuscated", plugin: `"salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"obfuscated","obfuscation":true,"optimizations":{"guardGenerationDedupLimit":2}`, config: config.FlameworkConfig{Salt: "task7-controlled-salt", HashPrefix: "task7", IDGenerationMode: "obfuscated", Obfuscation: true, Optimizations: config.FlameworkOptimizations{GuardGenerationDedupLimit: &limit2}}},
		{name: "implicit-obfuscated-mode-and-persisted-salt", plugin: `"obfuscation":true`, config: config.FlameworkConfig{Obfuscation: true}, seedSalt: true},
	}

	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			oracleRoot, nativeRoot := t.TempDir(), t.TempDir()
			copyTask7Tree(t, fixture, oracleRoot)
			copyTask7Tree(t, fixture, nativeRoot)
			linkTask7Modules(t, install, oracleRoot)
			linkTask7Modules(t, install, nativeRoot)
			seedSalt := ""
			if testCase.seedSalt {
				seedSalt = `,"salt":"task7-controlled-salt"`
			}
			pathHash := ""
			if testCase.config.Obfuscation {
				pathHash = `,"addPaths:src/**/*.ts":"00000000-0000-4000-8000-000000000004"`
			}
			seed := `{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"stringHashes":{"task4:payload":"00000000-0000-4000-8000-000000000004"` + pathHash + `}` + seedSalt + `}`
			writeTransformFixture(t, oracleRoot, "flamework.build", seed)
			writeTransformFixture(t, nativeRoot, "flamework.build", seed)
			if testCase.seedSalt {
				writeTransformFixture(t, oracleRoot, "out/task7.tsbuildinfo", `{"version":"5.5.3"}`)
				writeTransformFixture(t, nativeRoot, "out/task7.tsbuildinfo", `{"version":"5.5.3"}`)
			}
			oracleConfig := task7TSConfig(testCase.plugin, "src")
			if testCase.seedSalt {
				oracleConfig = strings.Replace(oracleConfig, `"compilerOptions":{`, `"compilerOptions":{"incremental":true,"tsBuildInfoFile":"out/task7.tsbuildinfo",`, 1)
			}
			writeTransformFixture(t, oracleRoot, "tsconfig.json", oracleConfig)
			writeTransformFixture(t, nativeRoot, "tsconfig.json", task7TSConfig("", "src"))

			// When: the real npm transformer and Rotor native path compile the same pristine tree.
			oracleOutput := runTask7Oracle(t, oracleRoot, "tsconfig.json", true)
			nativeOutput, diagnostics, nativeErr := runTask7Native(t, nativeRoot, testCase.config)

			// Then: final Luau bytes, ordered diagnostics, and normalized artifact structures match.
			compareTask7Trees(t, filepath.Join(oracleRoot, "out"), nativeOutput)
			if nativeErr != nil || len(diagnostics) != 0 {
				t.Fatalf("native error=%v diagnostics=%v, want none; oracle output:\n%s", nativeErr, diagnostics, oracleOutput)
			}
			for _, artifact := range []string{"flamework.build", "include/flamework/config.json", "include/flamework/globs.json"} {
				compareTask7JSON(t, filepath.Join(oracleRoot, filepath.FromSlash(artifact)), filepath.Join(nativeRoot, filepath.FromSlash(artifact)))
			}
			oracleManifest := task7Manifest(t, oracleRoot)
			nativeManifest := task7Manifest(t, nativeRoot)
			if fmt.Sprint(oracleManifest) != fmt.Sprint(nativeManifest) {
				t.Fatalf("SHA-256 manifests differ\noracle: %v\nnative: %v", oracleManifest, nativeManifest)
			}
			t.Logf("oracle=1.3.2; normalized SHA-256 manifest=%v", oracleManifest)
		})
	}
}

func TestTask7Diagnostic006_publicCompilerPreconditionsAreExecutable(t *testing.T) {
	// Given: the pinned package has an intentionally mismatched private TypeScript,
	// while roblox-ts and the native compiler each retain their public compiler ABI.
	fixture := filepath.Join("testdata", "task7-differential", "project")
	install := t.TempDir()
	copyTask7Tree(t, fixture, install)
	writeTransformFixture(t, install, "tsconfig.json", task7TSConfig("default", "src"))
	runTask7Command(t, install, 90*time.Second, "npm", "install", "--ignore-scripts", "--package-lock=false", "--no-audit", "--no-fund", "--prefer-offline")
	transformerRoot := filepath.Join(install, "node_modules", "rbxts-transformer-flamework")
	runTask7Command(t, install, 90*time.Second, "npm", "install", "--prefix", transformerRoot, "--no-save", "--ignore-scripts", "--package-lock=false", "--no-audit", "--no-fund", "--prefer-offline", "typescript@5.4.5")
	versions := runTask7Command(t, install, 10*time.Second, "node", "-e", `process.stdout.write(require("./node_modules/rbxts-transformer-flamework/node_modules/typescript").version + "|" + require(require.resolve("typescript", { paths: [require.resolve("roblox-ts")] })).version)`)
	if versions != "5.4.5|5.5.3" {
		t.Fatalf("mismatched TypeScript fixture versions = %q, want 5.4.5|5.5.3", versions)
	}

	// When: the real package entrypoint performs its public hook, and native receives
	// only SourceFiles obtained from its single compiled Program.
	upstream := runTask7Command(t, install, 10*time.Second, "node", "-e", `require("rbxts-transformer-flamework")`, "--", "--force-flamework-hook")
	nativeRoot := t.TempDir()
	copyTask7Tree(t, fixture, nativeRoot)
	linkTask7Modules(t, install, nativeRoot)
	writeTransformFixture(t, nativeRoot, "tsconfig.json", task7TSConfig("", "src"))
	program := newTransformProgram(t, nativeRoot)
	files := task7SourceFiles(program, nativeRoot)
	for _, file := range files {
		if program.GetSourceFile(filepath.ToSlash(file.FileName())) != file {
			t.Fatalf("native callback file %q is not owned by its compiler Program", file.FileName())
		}
	}
	_, nativeDiagnostics, nativeErr := runTask7NativeMode(t, nativeRoot, config.FlameworkConfig{NoSemanticDiagnostics: true}, false)

	// Then: the upstream precondition emits its exact three-line warning and hooks
	// to roblox-ts 5.5.3 without failing. Native succeeds with Program-owned files,
	// making both a foreign TypeScript ABI and a missing parse-tree node unconstructible.
	wantUpstream := strings.Join([]string{
		"[Flamework]: TypeScript version differs",
		"[Flamework]: Flamework: v5.4.5, roblox-ts: v5.5.3",
		"[Flamework]: Flamework will switch to v5.5.3, but you can get rid of this warning by running: npm i -D typescript@5.5.3",
	}, "\n")
	if upstream != wantUpstream {
		t.Fatalf("upstream TypeScript hook output = %q, want %q", upstream, wantUpstream)
	}
	if nativeErr != nil || len(nativeDiagnostics) != 0 || len(files) == 0 {
		t.Fatalf("native single-ABI build = files:%d diagnostics:%q error:%v", len(files), nativeDiagnostics, nativeErr)
	}
	t.Logf("upstream hook output:\n%s", upstream)
	t.Logf("public-precondition proof: upstream hook 5.4.5->5.5.3 succeeded with three exact warnings; native transformed %d Program-owned SourceFiles with one compiled tsgo ABI", len(files))
}

func task7Manifest(t *testing.T, root string) []string {
	t.Helper()
	entries := make([]string, 0)
	for _, artifact := range []string{"flamework.build", "include/flamework/config.json", "include/flamework/globs.json"} {
		path := filepath.Join(root, filepath.FromSlash(artifact))
		data := normalizedTask7JSON(t, path)
		sum := sha256.Sum256(data)
		entries = append(entries, fmt.Sprintf("%x  %s", sum, artifact))
	}
	err := filepath.WalkDir(filepath.Join(root, "out"), func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil || entry.IsDir() || filepath.Ext(path) != ".luau" {
			return walkErr
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		relative, err := filepath.Rel(root, path)
		if err != nil {
			return err
		}
		sum := sha256.Sum256(stripTask7LuauHeader(data))
		entries = append(entries, fmt.Sprintf("%x  %s", sum, filepath.ToSlash(relative)))
		return nil
	})
	if err != nil {
		t.Fatalf("build Task 7 manifest: %v", err)
	}
	sort.Strings(entries)
	return entries
}

func TestTask7Differential_matchesRealUpstreamOrderedDiagnostics(t *testing.T) {
	// Given: each malformed upstream branch in a fresh deterministic compilation.
	fixture, install := filepath.Join("testdata", "task7-differential", "project"), t.TempDir()
	copyTask7Tree(t, fixture, install)
	runTask7Command(t, install, 90*time.Second, "npm", "install", "--ignore-scripts", "--package-lock=false", "--no-audit", "--no-fund", "--prefer-offline")
	plugin := `"salt":"task7-controlled-salt","hashPrefix":"task7","idGenerationMode":"full"`
	for _, source := range []string{
		"invalid/non-constant-path.ts",
		"invalid/missing-rojo-path.ts",
		"invalid/template-literal-guard.ts",
		"invalid/type-parameter-guard.ts",
		"invalid/class-guard.ts",
		"invalid/non-tuple-guard.ts",
		"invalid/declaration-uid-placement.ts",
		"invalid/const-parameter.ts",
		"invalid/const-parameter-spread.ts",
		"invalid/network-middleware.ts",
	} {
		t.Run(filepath.Base(source), func(t *testing.T) {
			oracleRoot, nativeRoot := t.TempDir(), t.TempDir()
			copyTask7Tree(t, fixture, oracleRoot)
			copyTask7Tree(t, fixture, nativeRoot)
			linkTask7Modules(t, install, oracleRoot)
			linkTask7Modules(t, install, nativeRoot)
			writeTransformFixture(t, oracleRoot, "tsconfig.json", task7TSFileConfig(plugin, source))
			writeTransformFixture(t, nativeRoot, "tsconfig.json", task7TSFileConfig("", source))

			// When: both implementations process the same invalid file.
			oracleOutput := runTask7Oracle(t, oracleRoot, "tsconfig.json", false)
			_, nativeDiagnostics, nativeErr := runTask7Native(t, nativeRoot, config.FlameworkConfig{Salt: "task7-controlled-salt", HashPrefix: "task7", IDGenerationMode: "full"})
			if nativeErr != nil {
				var macroErr *MacroError
				var guardErr *GuardGenerationError
				if errors.As(nativeErr, &macroErr) {
					nativeErr = macroErr
				} else if errors.As(nativeErr, &guardErr) {
					nativeErr = guardErr
				}
				nativeDiagnostics = append(nativeDiagnostics, nativeErr.Error())
			}
			matches := task7OracleDiagnostic.FindAllStringSubmatch(task7ANSI.ReplaceAllString(oracleOutput, ""), -1)
			orderedOracle := make([]string, 0, len(matches))
			for _, match := range matches {
				orderedOracle = append(orderedOracle, strings.TrimSuffix(match[1], "\r"))
			}

			// Then: the exact ordered diagnostic messages match.
			if fmt.Sprint(orderedOracle) != fmt.Sprint(nativeDiagnostics) {
				t.Fatalf("ordered diagnostics differ\noracle: %v\nnative: %v\nraw oracle:\n%s", orderedOracle, nativeDiagnostics, oracleOutput)
			}
		})
	}
}

func TestTask7Differential_noSemanticDiagnosticsControlsTheSemanticGate(t *testing.T) {
	// Given: a macro-bearing source with one stable TypeScript semantic error.
	fixture, install := filepath.Join("testdata", "task7-differential", "project"), t.TempDir()
	copyTask7Tree(t, fixture, install)
	runTask7Command(t, install, 90*time.Second, "npm", "install", "--ignore-scripts", "--package-lock=false", "--no-audit", "--no-fund", "--prefer-offline")
	source := "invalid/semantic-error.ts"

	// When: the default false branch validates semantics in both compiler implementations.
	oracleRoot, nativeRoot := t.TempDir(), t.TempDir()
	for _, root := range []string{oracleRoot, nativeRoot} {
		copyTask7Tree(t, fixture, root)
		linkTask7Modules(t, install, root)
	}
	writeTransformFixture(t, oracleRoot, "tsconfig.json", task7TSFileConfig("default", source))
	oracleOutput := task7ANSI.ReplaceAllString(runTask7Oracle(t, oracleRoot, "tsconfig.json", false), "")
	writeTransformFixture(t, nativeRoot, "tsconfig.json", task7TSFileConfig("", source))
	program := newTransformProgram(t, nativeRoot)
	nativeSource := program.GetSourceFile(filepath.ToSlash(filepath.Join(nativeRoot, source)))
	if nativeSource == nil {
		t.Fatal("native semantic fixture source not found")
	}
	nativeDiagnostics := program.GetSemanticDiagnostics(context.Background(), nativeSource)

	// Then: the ordered TypeScript diagnostic tuple is identical and neither path emits Luau.
	matches := task7SemanticDiagnostic.FindAllStringSubmatch(oracleOutput, -1)
	if len(matches) != 1 || len(nativeDiagnostics) != 1 {
		t.Fatalf("semantic diagnostics oracle=%v native=%v\n%s", matches, nativeDiagnostics, oracleOutput)
	}
	wantTuple := matches[0][1] + ":" + strings.TrimSuffix(matches[0][2], "\r")
	gotTuple := fmt.Sprintf("%d:%s", nativeDiagnostics[0].Code(), nativeDiagnostics[0].String())
	if gotTuple != wantTuple {
		t.Fatalf("semantic diagnostic tuple native=%q oracle=%q", gotTuple, wantTuple)
	}

	// When: noSemanticDiagnostics=true skips that gate and both transformers emit the macro result.
	oracleSkipRoot, nativeSkipRoot := t.TempDir(), t.TempDir()
	for _, root := range []string{oracleSkipRoot, nativeSkipRoot} {
		copyTask7Tree(t, fixture, root)
		linkTask7Modules(t, install, root)
	}
	writeTransformFixture(t, oracleSkipRoot, "tsconfig.json", task7TSFileConfig(`"noSemanticDiagnostics":true`, source))
	runTask7Oracle(t, oracleSkipRoot, "tsconfig.json", false)
	writeTransformFixture(t, nativeSkipRoot, "tsconfig.json", task7TSFileConfig("", source))
	_, diagnostics, err := runTask7NativeMode(t, nativeSkipRoot, config.FlameworkConfig{NoSemanticDiagnostics: true}, false)
	if err != nil || len(diagnostics) != 0 {
		t.Fatalf("native skip gate error=%v diagnostics=%v", err, diagnostics)
	}
	compareTask7JSON(t, filepath.Join(oracleSkipRoot, "flamework.build"), filepath.Join(nativeSkipRoot, "flamework.build"))
}

func TestTask7Differential_scopedPackageDefaultsHashPrefixToPackageName(t *testing.T) {
	// Given: the representative corpus published as a scoped package with hashPrefix omitted.
	fixture, install := filepath.Join("testdata", "task7-differential", "project"), t.TempDir()
	copyTask7Tree(t, fixture, install)
	runTask7Command(t, install, 90*time.Second, "npm", "install", "--ignore-scripts", "--package-lock=false", "--no-audit", "--no-fund", "--prefer-offline")
	oracleRoot, nativeRoot := t.TempDir(), t.TempDir()
	for _, root := range []string{oracleRoot, nativeRoot} {
		copyTask7Tree(t, fixture, root)
		linkTask7Modules(t, install, root)
		packageData, err := os.ReadFile(filepath.Join(root, "package.json"))
		if err != nil {
			t.Fatalf("read package fixture: %v", err)
		}
		writeTransformFixture(t, root, "package.json", strings.Replace(string(packageData), `"rotor-flamework-task4-oracle"`, `"@task7/scoped-package"`, 1))
		seed := `{"version":1,"flameworkVersion":"1.3.2","identifiers":{},"stringHashes":{"task4:payload":"00000000-0000-4000-8000-000000000004"}}`
		writeTransformFixture(t, root, "flamework.build", seed)
	}
	writeTransformFixture(t, oracleRoot, "tsconfig.json", task7TSConfig("default", "src"))
	writeTransformFixture(t, nativeRoot, "tsconfig.json", task7TSConfig("", "src"))

	// When: the real transformer and native transformer compile the package.
	runTask7Oracle(t, oracleRoot, "tsconfig.json", true)
	_, diagnostics, err := runTask7NativeMode(t, nativeRoot, config.FlameworkConfig{}, false)

	// Then: package-prefixed IDs and transformer-owned build metadata are exact.
	if err != nil || len(diagnostics) != 0 {
		t.Fatalf("native package error=%v diagnostics=%v", err, diagnostics)
	}
	compareTask7JSON(t, filepath.Join(oracleRoot, "flamework.build"), filepath.Join(nativeRoot, "flamework.build"))
	buildData, readErr := os.ReadFile(filepath.Join(nativeRoot, "flamework.build"))
	if readErr != nil || !bytes.Contains(buildData, []byte(`"identifierPrefix": "@task7/scoped-package"`)) {
		t.Fatalf("scoped package build info error=%v:\n%s", readErr, buildData)
	}
}

func TestTask7Differential_accountsForEveryPinnedUpstreamEntryAndCase(t *testing.T) {
	// Given: the pinned upstream inventory and machine-consumed differential matrix.
	inventory := readTask7TSV(t, filepath.Join("testdata", "task7-differential", "upstream-inventory.tsv"))
	coverage := readTask7TSV(t, filepath.Join("testdata", "task7-differential", "coverage.tsv"))
	loci := readTask7TSV(t, filepath.Join("testdata", "task7-differential", "diagnostic-loci.tsv"))

	// When: all source entries and declared behavior families are accounted.
	paths, families := map[string]bool{}, map[string]bool{}
	for _, record := range inventory[1:] {
		paths[record[1]], families[record[2]] = true, true
	}
	covered := map[string]bool{}
	for _, record := range coverage[1:] {
		for _, family := range strings.Split(record[1], ",") {
			covered[family] = true
		}
	}

	// Then: the unported entry and behavior-family counts are exactly zero.
	var unported []string
	for family := range families {
		if !covered[family] && family != "entrypoint" && family != "nonbehavior" && family != "dispatch" {
			unported = append(unported, family)
		}
	}
	pinnedLoci := task7PinnedDiagnosticLoci(t)
	accountedLoci := make(map[string]bool, len(loci)-1)
	for _, record := range loci[1:] {
		if len(record) != 3 || record[2] == "" {
			t.Fatalf("invalid diagnostic locus record %v", record)
		}
		accountedLoci[record[0]+":"+record[1]] = true
	}
	if len(paths) != 67 || len(unported) != 0 || len(coverage) != 22 || len(pinnedLoci) != 64 || fmt.Sprint(sortedTask7BoolKeys(pinnedLoci)) != fmt.Sprint(sortedTask7BoolKeys(accountedLoci)) {
		t.Fatalf("accounting paths=%d cases=%d diagnostic-loci=%d unported=%v missing-loci=%v, want 67/21/64/zero", len(paths), len(coverage)-1, len(pinnedLoci), unported, task7MissingKeys(pinnedLoci, accountedLoci))
	}
}

func task7PinnedDiagnosticLoci(t *testing.T) map[string]bool {
	t.Helper()
	root := filepath.Join("..", "..", "reference", "rbxts-transformer-flamework")
	pattern := regexp.MustCompile("Diagnostics\\.(error|warning|createDiagnostic|addDiagnostic)|throw new (Error|DiagnosticError)|throw `|process\\.exit\\(|Logger\\.error")
	loci := map[string]bool{}
	err := filepath.WalkDir(filepath.Join(root, "src"), func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil || entry.IsDir() || filepath.Ext(path) != ".ts" {
			return walkErr
		}
		file, err := os.Open(path)
		if err != nil {
			return err
		}
		defer func() { _ = file.Close() }()
		scanner := bufio.NewScanner(file)
		for line := 1; scanner.Scan(); line++ {
			if pattern.MatchString(scanner.Text()) {
				relative, relErr := filepath.Rel(root, path)
				if relErr != nil {
					return relErr
				}
				loci[filepath.ToSlash(relative)+":"+fmt.Sprint(line)] = true
			}
		}
		return scanner.Err()
	})
	if err != nil {
		t.Fatalf("scan pinned diagnostic loci: %v", err)
	}
	return loci
}

func runTask7Native(t *testing.T, root string, cfg config.FlameworkConfig) (string, []string, error) {
	return runTask7NativeMode(t, root, cfg, true)
}

func runTask7NativeMode(t *testing.T, root string, cfg config.FlameworkConfig, lower bool) (string, []string, error) {
	t.Helper()
	program := newTransformProgram(t, root)
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	files := task7SourceFiles(program, root)
	project, err := OpenProject(ProjectOptions{ProjectDir: root, RootDir: task7RootDir(files), OutDir: "out", Config: cfg})
	if err != nil {
		t.Fatalf("OpenProject() error = %v", err)
	}
	runtime := MacroRuntime{UUID: func() (string, error) { return "00000000-0000-4000-8000-000000000004", nil }, RandomIndex: func(int) (int, error) { return 0, nil }}
	result, err := Transform(TransformInput{Program: program, Checker: checker, Files: files, Project: project, MacroRuntime: &runtime})
	if err != nil {
		return filepath.Join(root, "out"), nil, err
	}
	diagnostics := make([]string, 0, len(result.Diagnostics))
	for _, diagnostic := range result.Diagnostics {
		diagnostics = append(diagnostics, diagnostic.String())
	}
	if len(diagnostics) != 0 {
		return filepath.Join(root, "out"), diagnostics, nil
	}
	for index, source := range result.Files {
		printed := printer.NewPrinter(printer.PrinterOptions{}, printer.PrintHandlers{}, result.Sources[index].EmitContext()).EmitSourceFile(source)
		reparsed := parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: source.FileName(), Path: tspath.Path(source.FileName())}, printed, core.ScriptKindTS)
		if len(reparsed.Diagnostics()) != 0 {
			t.Fatalf("reparse %s diagnostics = %v", source.FileName(), reparsed.Diagnostics())
		}
		if err := os.WriteFile(filepath.FromSlash(source.FileName()), []byte(printed), 0o644); err != nil {
			t.Fatalf("write transformed source: %v", err)
		}
	}
	if err := project.Persist(); err != nil {
		t.Fatalf("Persist() error = %v", err)
	}
	if lower {
		lowerTask7Files(t, root, project)
	}
	return filepath.Join(root, "out"), diagnostics, nil
}

func lowerTask7Files(t *testing.T, root string, project *Project) {
	t.Helper()
	program := newTransformProgram(t, root)
	for _, source := range task7SourceFiles(program, root) {
		checker, release := program.GetTypeCheckerForFile(context.Background(), source)
		state := transformer.NewState(program, checker, source, transformer.NewDiagService(), transformer.NewMultiState())
		projectType := transformer.ProjectTypeGame
		if !project.IsGame() {
			projectType = transformer.ProjectTypePackage
		}
		if rbx, ok := project.RojoResolver().GetRbxPathFromFilePath(filepath.Join(project.IncludeDirectory(), "RuntimeLib.lua")); ok {
			state.SetRojoContext(&transformer.RojoContext{Resolver: project.RojoResolver(), PathTranslator: project.PathTranslator(), RuntimeLibRbxPath: rbx, ProjectPath: root, NodeModulesPath: filepath.Join(root, "node_modules"), TypeRoots: []string{filepath.Join(root, "node_modules", "@rbxts"), filepath.Join(root, "node_modules", "@flamework")}, UseCaseSensitiveFileNames: osvfs.FS().UseCaseSensitiveFileNames(), ImportPathMap: map[string]string{}}, projectType)
		}
		luau := render.RenderAST(transformer.TransformSourceFile(state))
		release()
		if state.Diags.HasErrors() {
			t.Fatalf("lower %s diagnostics = %v", source.FileName(), state.Diags.Flush())
		}
		relative, err := filepath.Rel(filepath.Join(root, task7RootDir([]*ast.SourceFile{source})), filepath.FromSlash(source.FileName()))
		if err != nil {
			t.Fatalf("relative output path: %v", err)
		}
		output := filepath.Join(root, "out", strings.TrimSuffix(relative, filepath.Ext(relative))+".luau")
		writeTransformFixture(t, filepath.Dir(output), filepath.Base(output), luau)
	}
}

func task7SourceFiles(program *compiler.Program, root string) []*ast.SourceFile {
	var files []*ast.SourceFile
	for _, source := range program.SourceFiles() {
		name := filepath.ToSlash(source.FileName())
		if !source.IsDeclarationFile && strings.HasPrefix(name, filepath.ToSlash(root)+"/") && strings.HasSuffix(name, ".ts") {
			files = append(files, source)
		}
	}
	sort.Slice(files, func(i, j int) bool { return files[i].FileName() < files[j].FileName() })
	return files
}

func task7RootDir(files []*ast.SourceFile) string {
	for _, source := range files {
		if strings.Contains(filepath.ToSlash(source.FileName()), "/invalid/") {
			return "invalid"
		}
	}
	return "src"
}

func runTask7Oracle(t *testing.T, root, project string, success bool) string {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()
	command := exec.CommandContext(ctx, filepath.Join(root, "node_modules", ".bin", "rbxtsc"), "--project", project)
	command.Dir = root
	command.Env = append(os.Environ(), "NO_COLOR=1", "NODE_OPTIONS=--require=./deterministic-random.cjs")
	output, err := command.CombinedOutput()
	if ctx.Err() != nil || (success && err != nil) || (!success && err == nil) {
		t.Fatalf("oracle rbxtsc success=%v error=%v timeout=%v:\n%s", success, err, ctx.Err(), output)
	}
	return string(output)
}

func task7TSConfig(plugin, rootDir string) string {
	plugins := "[]"
	if plugin == "default" {
		plugins = `[{"transform":"rbxts-transformer-flamework"}]`
	} else if plugin != "" {
		plugins = `[{"transform":"rbxts-transformer-flamework",` + plugin + `}]`
	}
	return fmt.Sprintf(`{"compilerOptions":{"allowSyntheticDefaultImports":true,"downlevelIteration":true,"experimentalDecorators":true,"forceConsistentCasingInFileNames":true,"module":"commonjs","moduleDetection":"force","moduleResolution":"Node","noLib":true,"outDir":"out","plugins":%s,"rootDir":"%s","strict":true,"target":"ESNext","typeRoots":["node_modules/@rbxts","node_modules/@flamework"],"types":["types"]},"include":["%s"]}`, plugins, rootDir, rootDir)
}

func task7TSFileConfig(plugin, source string) string {
	config := task7TSConfig(plugin, "invalid")
	return strings.Replace(config, `"include":["invalid"]`, `"files":["`+source+`"]`, 1)
}

func runTask7Command(t *testing.T, directory string, timeout time.Duration, name string, args ...string) string {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()
	command := exec.CommandContext(ctx, name, args...)
	command.Dir = directory
	output, err := command.CombinedOutput()
	if err != nil || ctx.Err() != nil {
		t.Fatalf("%s %v error=%v timeout=%v:\n%s", name, args, err, ctx.Err(), output)
	}
	return string(bytes.TrimSpace(output))
}

func copyTask7Tree(t *testing.T, source, destination string) {
	t.Helper()
	err := filepath.WalkDir(source, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		relative, err := filepath.Rel(source, path)
		if err != nil || relative == "." {
			return err
		}
		target := filepath.Join(destination, relative)
		if entry.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		return os.WriteFile(target, data, 0o644)
	})
	if err != nil {
		t.Fatalf("copy fixture tree: %v", err)
	}
}

func linkTask7Modules(t *testing.T, install, root string) {
	t.Helper()
	source := filepath.Join(install, "node_modules")
	destination := filepath.Join(root, "node_modules")
	err := filepath.WalkDir(source, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		relative, err := filepath.Rel(source, path)
		if err != nil || relative == "." {
			return err
		}
		target := filepath.Join(destination, relative)
		if entry.IsDir() {
			return os.MkdirAll(target, 0o755)
		}
		if entry.Type()&os.ModeSymlink != 0 {
			link, err := os.Readlink(path)
			if err != nil {
				return err
			}
			return os.Symlink(link, target)
		}
		return os.Link(path, target)
	})
	if err != nil {
		t.Fatalf("clone oracle node_modules: %v", err)
	}
}

func compareTask7Trees(t *testing.T, oracle, native string) {
	t.Helper()
	want, got := map[string][]byte{}, map[string][]byte{}
	for root, target := range map[string]map[string][]byte{oracle: want, native: got} {
		err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, walkErr error) error {
			if walkErr != nil || entry.IsDir() || filepath.Ext(path) != ".luau" {
				return walkErr
			}
			relative, err := filepath.Rel(root, path)
			if err != nil {
				return err
			}
			data, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			target[filepath.ToSlash(relative)] = stripTask7LuauHeader(data)
			return nil
		})
		if err != nil {
			t.Fatalf("walk Luau tree: %v", err)
		}
	}
	if fmt.Sprint(sortedTask7Keys(want)) != fmt.Sprint(sortedTask7Keys(got)) {
		t.Fatalf("Luau tree paths differ: oracle=%v native=%v", sortedTask7Keys(want), sortedTask7Keys(got))
	}
	for name, expected := range want {
		if !bytes.Equal(expected, got[name]) {
			t.Fatalf("final Luau %s differs\n--- oracle\n%s\n--- native\n%s", name, expected, got[name])
		}
	}
}

// stripTask7LuauHeader removes the leading `-- Compiled with ...` comment line
// so the differential comparison is header-independent (the project design
// contract is byte-identical Luau modulo the compiler branding line).
func stripTask7LuauHeader(data []byte) []byte {
	if !bytes.HasPrefix(data, []byte("-- Compiled with ")) {
		return data
	}
	if idx := bytes.IndexByte(data, '\n'); idx >= 0 {
		return data[idx+1:]
	}
	return nil
}

func compareTask7JSON(t *testing.T, oracle, native string) {
	t.Helper()
	if want, got := normalizedTask7JSON(t, oracle), normalizedTask7JSON(t, native); !bytes.Equal(want, got) {
		t.Fatalf("artifact structures differ\noracle %s\nnative %s", want, got)
	}
}

func normalizedTask7JSON(t *testing.T, path string) []byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read artifact %s: %v", path, err)
	}
	var value any
	if err := json.Unmarshal(data, &value); err != nil {
		t.Fatalf("decode artifact %s: %v", path, err)
	}
	normalizeTask7JSON(value, false, filepath.Base(path) == "globs.json")
	result, err := json.Marshal(value)
	if err != nil {
		t.Fatalf("canonicalize artifact %s: %v", path, err)
	}
	return result
}

func normalizeTask7JSON(value any, sortStringArrays, sortNestedArrays bool) {
	switch typed := value.(type) {
	case map[string]any:
		for key, child := range typed {
			if key == "filePath" {
				if pathValue, ok := child.(string); ok {
					typed[key] = strings.ReplaceAll(pathValue, "\\", "/")
					child = typed[key]
				}
			}
			normalizeTask7JSON(child, sortStringArrays || key == "paths" || key == "origins", sortNestedArrays)
		}
	case []any:
		nested := len(typed) > 0
		if nested {
			_, nested = typed[0].([]any)
		}
		if sortStringArrays || sortNestedArrays && nested {
			sort.Slice(typed, func(i, j int) bool { return fmt.Sprint(typed[i]) < fmt.Sprint(typed[j]) })
		}
		for _, child := range typed {
			normalizeTask7JSON(child, sortStringArrays, sortNestedArrays)
		}
	}
}

func readTask7TSV(t *testing.T, path string) [][]string {
	t.Helper()
	file, err := os.Open(path)
	if err != nil {
		t.Fatalf("open %s: %v", path, err)
	}
	defer func() { _ = file.Close() }()
	reader := csv.NewReader(file)
	reader.Comma = '\t'
	records, err := reader.ReadAll()
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	return records
}

func sortedTask7Keys(values map[string][]byte) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func sortedTask7BoolKeys(values map[string]bool) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func task7MissingKeys(want, got map[string]bool) []string {
	missing := map[string]bool{}
	for key := range want {
		if !got[key] {
			missing[key] = true
		}
	}
	return sortedTask7BoolKeys(missing)
}
