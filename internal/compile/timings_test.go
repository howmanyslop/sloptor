package compile

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"testing"
	"testing/synctest"
	"time"

	"rotor/internal/logservice"
)

func TestBuildTimings(t *testing.T) {
	t.Run("unchanged rebuild performs zero output writes", func(t *testing.T) {
		dir := writeProject(t, "@scope/timings-warm", "")
		if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil {
			t.Fatalf("seed build: %v (diags: %v)", err, diags)
		}
		timings := NewBuildTimings()

		if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings}); err != nil {
			t.Fatalf("warm build: %v (diags: %v)", err, diags)
		}
		if timings.Counts.ActualWrites != 0 {
			t.Fatalf("actual writes = %d, want 0", timings.Counts.ActualWrites)
		}
		if timings.Counts.HashSkips == 0 {
			t.Fatal("hash skips = 0, want at least one")
		}
	})

	t.Run("pluginless build records zero sidecar and overlay stages", func(t *testing.T) {
		// Given
		dir := writeProject(t, "@scope/timings-pluginless", "")
		timings := NewBuildTimings()

		// When
		_, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings})
		// Then
		if err != nil {
			t.Fatalf("BuildProjectWithOptions: %v (diags: %v)", err, diags)
		}
		if timings.SchemaVersion != BuildTimingSchemaVersion {
			t.Errorf("schemaVersion = %d, want %d", timings.SchemaVersion, BuildTimingSchemaVersion)
		}
		if timings.Stages.SidecarRoundTripMs != 0 || timings.Stages.OverlayProgramMs != 0 {
			t.Errorf("pluginless sidecar stages = %+v, want zero", timings.Stages)
		}
		if timings.Counts.TotalSources != 1 || timings.Counts.SelectedSources != 1 {
			t.Errorf("source counts = %+v, want one total and selected source", timings.Counts)
		}
		assertBuildTimingsNonNegative(t, timings)
	})

	t.Run("transformer build records separate sidecar and overlay stages", func(t *testing.T) {
		// Given
		setRepoSidecarPath(t)
		closeSidecarSessions()
		dir := writeProject(t, "@scope/timings-transformer", "")
		t.Cleanup(closeSidecarSessions)
		writeSidecarPluginFixture(t, dir, "", `{
	"compilerOptions": {
		"allowSyntheticDefaultImports": true,
		"module": "CommonJS",
		"moduleResolution": "Node",
		"noLib": true,
		"moduleDetection": "force",
		"strict": true,
		"target": "ESNext",
		"types": [],
		"typeRoots": ["node_modules/@rbxts"],
		"rootDir": "src",
		"outDir": "out",
		"plugins": [{"transform": "./plugins/prefix-string.js", "prefix": "timings"}]
	},
	"include": ["src"]
}`)
		if err := os.WriteFile(filepath.Join(dir, "plugins", "prefix-string.js"), []byte(prefixStringPlugin), 0o644); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const value = \"value\";\n"), 0o644); err != nil {
			t.Fatal(err)
		}
		timings := NewBuildTimings()

		// When
		_, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings})
		// Then
		if err != nil {
			t.Fatalf("BuildProjectWithOptions: %v (diags: %v)", err, diags)
		}
		if !timings.sidecarRoundTripRecorded || !timings.overlayProgramRecorded {
			t.Errorf("transformer stages were not separately recorded: %+v", timings.Stages)
		}
		assertBuildTimingsNonNegative(t, timings)
	})
}

func assertBuildTimingsNonNegative(t *testing.T, timings *BuildTimings) {
	t.Helper()
	for label, duration := range map[string]int64{
		"total":                   timings.TotalMs,
		"initial program":         timings.Stages.InitialProgramMs,
		"incremental selection":   timings.Stages.IncrementalSelectionMs,
		"cleanup":                 timings.Stages.CleanupMs,
		"include copy":            timings.Stages.IncludeCopyMs,
		"non-compiled copy":       timings.Stages.NonCompiledCopyMs,
		"sidecar wait":            timings.Stages.SidecarSessionWaitMs,
		"sidecar preparation":     timings.Stages.SidecarPreparationMs,
		"sidecar round trip":      timings.Stages.SidecarRoundTripMs,
		"sidecar decode":          timings.Stages.SidecarResponseDecodeMs,
		"overlay program":         timings.Stages.OverlayProgramMs,
		"project context":         timings.Stages.ProjectContextMs,
		"semantic diagnostics":    timings.Stages.SemanticDiagnosticsMs,
		"transform/render":        timings.Stages.NativeTransformRenderMs,
		"compiled output writes":  timings.Stages.CompiledOutputWritesMs,
		"declaration emit writes": timings.Stages.DeclarationEmitWritesMs,
		"incremental manifest":    timings.Stages.IncrementalManifestMs,
		"persistence":             timings.Stages.PersistenceMs,
	} {
		if duration < 0 {
			t.Errorf("%s = %dms, want nonnegative", label, duration)
		}
	}
}

func TestBuildTimingsCollector(t *testing.T) {
	t.Run("child finish does not set parent totalMs", func(t *testing.T) {
		synctest.Test(t, func(t *testing.T) {
			parent := NewBuildTimings()
			parent.initProjects([]SolutionProject{
				{ConfigPath: "/a/tsconfig.json"},
				{ConfigPath: "/b/tsconfig.json"},
			})
			first := parent.newProject("/a/tsconfig.json")
			second := parent.newProject("/b/tsconfig.json")
			first.finish()
			time.Sleep(100 * time.Millisecond)
			second.finish()
			parent.finish()
			if parent.TotalMs < 100 {
				t.Fatalf("parent totalMs = %d, want at least 100 after last child", parent.TotalMs)
			}
			if first.TotalMs >= parent.TotalMs {
				t.Fatalf("first child totalMs %d should be smaller than parent %d", first.TotalMs, parent.TotalMs)
			}
		})
	})

	t.Run("counts from concurrent children are summed", func(t *testing.T) {
		parent := NewBuildTimings()
		parent.initProjects([]SolutionProject{
			{ConfigPath: "/z/tsconfig.json"},
			{ConfigPath: "/a/tsconfig.json"},
		})
		first := parent.newProject("/z/tsconfig.json")
		second := parent.newProject("/a/tsconfig.json")
		first.setSourceCounts(3, 2)
		first.setEmittedEntries(2)
		second.setSourceCounts(4, 1)
		second.setEmittedEntries(1)
		second.finish()
		first.finish()
		if parent.Counts.TotalSources != 7 || parent.Counts.SelectedSources != 3 {
			t.Fatalf("summed sources = %+v, want total 7 selected 3", parent.Counts)
		}
		if parent.Counts.EmittedEntries != 3 {
			t.Fatalf("emitted = %d, want 3", parent.Counts.EmittedEntries)
		}
		if got := []string{parent.Projects[0].ConfigPath, parent.Projects[1].ConfigPath}; got[0] != "/z/tsconfig.json" || got[1] != "/a/tsconfig.json" {
			t.Fatalf("project order = %v, want graph order", got)
		}
	})

	t.Run("overlapping stage work may exceed wall time", func(t *testing.T) {
		synctest.Test(t, func(t *testing.T) {
			parent := NewBuildTimings()
			first := parent.newProject("/a/tsconfig.json")
			second := parent.newProject("/b/tsconfig.json")
			stopFirst := first.startStage(initialProgramStage)
			stopSecond := second.startStage(initialProgramStage)
			time.Sleep(50 * time.Millisecond)
			stopFirst()
			stopSecond()
			first.finish()
			second.finish()
			parent.finish()
			if parent.Stages.InitialProgramMs < 100 {
				t.Fatalf("work ms = %d, want at least 100", parent.Stages.InitialProgramMs)
			}
			if parent.TotalMs < 50 {
				t.Fatalf("totalMs = %d, want at least 50", parent.TotalMs)
			}
			if parent.StageSemantics != StageSemanticsWorkMs {
				t.Fatalf("stageSemantics = %q", parent.StageSemantics)
			}
		})
	})

	t.Run("sidecar flags OR across children", func(t *testing.T) {
		parent := NewBuildTimings()
		pluginless := parent.newProject("/plain/tsconfig.json")
		pluginless.recordPreparedTransformerProgram(&preparedTransformerProgram{})
		pluginless.finish()
		transformer := parent.newProject("/plugin/tsconfig.json")
		transformer.recordPreparedTransformerProgram(&preparedTransformerProgram{
			sidecarRoundTripRecorded: true,
			overlayProgramRecorded:   true,
			sidecarRoundTripDuration: time.Millisecond,
		})
		transformer.finish()
		if !parent.sidecarRoundTripRecorded || !parent.overlayProgramRecorded {
			t.Fatalf("parent sidecar flags = roundTrip %t overlay %t, want both true", parent.sidecarRoundTripRecorded, parent.overlayProgramRecorded)
		}
	})

	t.Run("deferred persist is measured after child finish", func(t *testing.T) {
		synctest.Test(t, func(t *testing.T) {
			parent := NewBuildTimings()
			parent.initProjects([]SolutionProject{{ConfigPath: "/a/tsconfig.json"}})
			child := parent.newProject("/a/tsconfig.json")
			child.finish()
			persist := timedPersist(child, func() error {
				time.Sleep(50 * time.Millisecond)
				return nil
			})
			if err := persist(); err != nil {
				t.Fatal(err)
			}
			parent.finish()
			if child.Stages.PersistenceMs < 50 || parent.Stages.PersistenceMs < 50 {
				t.Fatalf("persistence child=%d parent=%d, want at least 50", child.Stages.PersistenceMs, parent.Stages.PersistenceMs)
			}
			if parent.Projects[0].Stages.PersistenceMs < 50 {
				t.Fatalf("project snapshot persistence = %d, want at least 50", parent.Projects[0].Stages.PersistenceMs)
			}
			if parent.TotalMs < 50 {
				t.Fatalf("parent totalMs = %d, want persist included", parent.TotalMs)
			}
		})
	})
}

func TestSidecarMetricsJSONRoundTrip(t *testing.T) {
	withoutMetrics := []byte(`{"diagnostics":[],"transformed":[],"declarations":[]}`)
	var plain sidecarResponse
	if err := json.Unmarshal(withoutMetrics, &plain); err != nil {
		t.Fatalf("unmarshal without metrics: %v", err)
	}
	if plain.Metrics != nil {
		t.Fatalf("metrics = %+v, want nil", plain.Metrics)
	}

	withMetrics := []byte(`{"diagnostics":[],"transformed":[],"metrics":{"wallMs":12,"cpuUserUs":3,"cpuSystemUs":4,"nodeVersion":"v22.0.0"}}`)
	var metered sidecarResponse
	if err := json.Unmarshal(withMetrics, &metered); err != nil {
		t.Fatalf("unmarshal with metrics: %v", err)
	}
	if metered.Metrics == nil || metered.Metrics.WallMs != 12 || metered.Metrics.NodeVersion != "v22.0.0" {
		t.Fatalf("metrics = %+v", metered.Metrics)
	}
}

func TestVerboseSidecarStages(t *testing.T) {
	t.Run("transformer build prints every sidecar stage", func(t *testing.T) {
		// Given
		logged := buildVerboseTransformerProject(t)

		// Then
		for _, name := range []string{
			sidecarSessionWaitStage.traceName(),
			sidecarPreparationStage.traceName(),
			sidecarRoundTripStage.traceName() + " (./plugins/prefix-string.js)",
			sidecarResponseDecodeStage.traceName(),
			overlayProgramStage.traceName(),
		} {
			assertStageLogged(t, logged, name)
		}
	})

	t.Run("transformer build charges the round trip to its plugins", func(t *testing.T) {
		// Given
		logged := buildVerboseTransformerProject(t)

		// Then
		line := regexp.MustCompile(`(?m)^.*: transformer plugin \./plugins/prefix-string\.js \( \d+ ms \)$`)
		if !line.MatchString(logged) {
			t.Errorf("missing per-plugin line; log: %s", logged)
		}
	})
}

func buildVerboseTransformerProject(t *testing.T) string {
	t.Helper()
	setRepoSidecarPath(t)
	closeSidecarSessions()
	dir := writeProject(t, "@scope/verbose-sidecar", "")
	t.Cleanup(closeSidecarSessions)
	writeSidecarPluginFixture(t, dir, "", `{
	"compilerOptions": {
	"allowSyntheticDefaultImports": true,
	"module": "CommonJS",
	"moduleResolution": "Node",
	"noLib": true,
	"moduleDetection": "force",
	"strict": true,
	"target": "ESNext",
	"types": [],
	"typeRoots": ["node_modules/@rbxts"],
	"rootDir": "src",
	"outDir": "out",
	"plugins": [{"transform": "./plugins/prefix-string.js", "prefix": "verbose"}]
	},
	"include": ["src"]
}`)
	if err := os.WriteFile(filepath.Join(dir, "plugins", "prefix-string.js"), []byte(prefixStringPlugin), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "src", "main.ts"), []byte("export const value = \"value\";\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	logged := captureVerboseLog(t)

	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: NewBuildTimings()}); err != nil {
		t.Fatalf("BuildProjectWithOptions: %v (diags: %v)", err, diags)
	}
	return logged.String()
}

func captureVerboseLog(t *testing.T) *bytes.Buffer {
	t.Helper()
	buf := &bytes.Buffer{}
	previousOutput, previousVerbose := logservice.Output, logservice.Verbose
	logservice.Output, logservice.Verbose = buf, true
	t.Cleanup(func() {
		logservice.Output, logservice.Verbose = previousOutput, previousVerbose
	})
	return buf
}

func assertStageLogged(t *testing.T, logged, name string) {
	t.Helper()
	quoted := regexp.QuoteMeta(name)
	for pattern, want := range map[string]string{
		`(?m)^(?:.*: )?` + quoted + `\.\.\.$`:        "start",
		`(?m)^(?:.*: )?` + quoted + ` \( \d+ ms \)$`: "completion",
	} {
		if !regexp.MustCompile(pattern).MatchString(logged) {
			t.Errorf("missing %s line for stage %q; log: %s", want, name, logged)
		}
	}
}
