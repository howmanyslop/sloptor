package compile

import (
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"rotor/internal/flamework"
)

func TestFlameworkDiscoveryLookupEmittedArtifactsAndRecovery(t *testing.T) {
	dir := t.TempDir()
	writeNativeFlameworkSolutionProject(t, dir, "@scope/discovery-lookup")
	linkFlameworkFixtureDependencies(t, dir)
	configPath := filepath.Join(dir, "tsconfig.json")
	config := string(mustReadFile(t, configPath))
	config = strings.Replace(config, `"outDir":"out"`, `"outDir":"out","incremental":true,"tsBuildInfoFile":"out/lookup.tsbuildinfo"`, 1)
	writeSolutionFile(t, dir, "tsconfig.json", config)
	goodSource := "/** @metadata identifier */ export class Persisted {}\n"
	writeSolutionFile(t, dir, "src/index.ts", goodSource)
	writeSolutionFile(t, dir, "flamework.build", `{"version":1,"flameworkVersion":"1.3.2","identifiers":{"@scope/discovery-lookup:out/init@Persisted":"b"},"classes":[{"internalId":"b","filePath":"first.lua","decorators":[{"name":"First","internalId":"first"}]},{"internalId":"a","filePath":"middle.lua"},{"internalId":"b","filePath":"third.lua","decorators":[{"name":"Third","internalId":"third"},{"name":"Extra","internalId":"extra"}]}]}`)
	result, diagnostics, err := BuildProjectWithOptions(dir, ProjectOptions{})
	if err != nil || len(diagnostics) != 0 {
		t.Fatalf("persisted fallback build: %v (%v)", err, diagnostics)
	}
	output := result.Outputs["out/init.luau"]
	// Captured from baseline production, including the absence of source decorator calls.
	wantOutput := "-- Compiled with sloptor v2.8.3\nlocal TS = _G[script]\n--* @metadata identifier \n" + `local Reflect = TS.import(script, TS.getModule(script, "@flamework", "core").out).Reflect
local Persisted
do
	Persisted = setmetatable({}, {
		__tostring = function()
			return "Persisted"
		end,
	})
	Persisted.__index = Persisted
	function Persisted.new(...)
		local self = setmetatable({}, Persisted)
		return self:constructor(...) or self
	end
	function Persisted:constructor()
	end
	do
		-- (Flamework) Persisted metadata
		Reflect.defineMetadata(Persisted, "identifier", "b")
	end
end
return {
	Persisted = Persisted,
}
`
	if output != wantOutput {
		t.Fatalf("persisted identifier or decorator provenance changed:\n%s\nwant:\n%s", output, wantOutput)
	}
	declaration := string(mustReadFile(t, filepath.Join(dir, "out/index.d.ts")))
	if want := "/** @metadata identifier */ export declare class Persisted {\n}\n"; declaration != want {
		t.Fatalf("declaration = %q, want %q", declaration, want)
	}
	info, err := flamework.LoadBuildInfo(filepath.Join(dir, "flamework.build"), flamework.FlameworkVersion)
	if err != nil {
		t.Fatal(err)
	}
	wantClasses := []flamework.BuildClass{
		{InternalID: "b", FilePath: "first.lua", Decorators: []flamework.BuildDecorator{{Name: "First", InternalID: "first"}}},
		{InternalID: "a", FilePath: "middle.lua"},
		{InternalID: "b", FilePath: "third.lua", Decorators: []flamework.BuildDecorator{{Name: "Third", InternalID: "third"}, {Name: "Extra", InternalID: "extra"}}},
	}
	if got := info.Snapshot().Classes; !reflect.DeepEqual(got, wantClasses) {
		t.Fatalf("fallback rewrote persisted class payload/order: %#v", got)
	}
	if got, found := info.Identifier("@scope/discovery-lookup:out/init@Persisted"); !found || got != "b" {
		t.Fatalf("persisted mapping = %q/%v", got, found)
	}
	paths := []string{
		filepath.Join(dir, "out/init.luau"), filepath.Join(dir, "out/index.d.ts"),
		filepath.Join(dir, "out/lookup.rbxtsc.tsbuildinfo"),
		filepath.Join(dir, "flamework.build"),
	}
	before := fileHashes(t, paths)
	for _, failure := range []struct {
		name, source, message string
	}{
		{"constructor", `export class Persisted<T extends string> { constructor(value: T extends string ? 1 : 2) {} }`, `Could not find UID for type "T extends string ? 1 : 2"`},
		{"guard", "/** @metadata identifier */ export class Persisted {\n/** @metadata flamework:guard */ value!: `prefix-${string}`;\n}\n", "Flamework encountered a template literal which is unsupported: `prefix-${string}`"},
	} {
		t.Run(failure.name, func(t *testing.T) {
			writeSolutionFile(t, dir, "src/index.ts", failure.source)
			_, diagnostics, err := BuildProjectWithOptions(dir, ProjectOptions{})
			if err == nil || !strings.Contains(strings.ToLower(err.Error()+strings.Join(diagnostics, "\n")), strings.ToLower(failure.message)) {
				t.Fatalf("failure = %v (%v), want %q", err, diagnostics, failure.message)
			}
			if !reflect.DeepEqual(fileHashes(t, paths), before) {
				t.Fatal("failed native transform published output, declarations, build metadata or incremental state")
			}
			writeSolutionFile(t, dir, "src/index.ts", goodSource)
			timings := NewBuildTimings()
			if _, diagnostics, err := BuildProjectWithOptions(dir, ProjectOptions{Timings: timings}); err != nil || len(diagnostics) != 0 {
				t.Fatalf("recovery = %v (%v)", err, diagnostics)
			}
			if !reflect.DeepEqual(fileHashes(t, paths), before) || timings.Counts.SelectedSources != 0 || timings.Counts.ActualWrites != 0 {
				t.Fatalf("recovery changed last-success state: selected=%d writes=%d", timings.Counts.SelectedSources, timings.Counts.ActualWrites)
			}
		})
	}
	if string(mustReadFile(t, filepath.Join(dir, "out/init.luau"))) != output || string(mustReadFile(t, filepath.Join(dir, "out/index.d.ts"))) != declaration {
		t.Fatal("recovery changed emitted artifacts")
	}
}
