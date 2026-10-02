package compile

import (
	"os"
	"path/filepath"
	"testing"
)

func TestBuildFlameworkIncludeArtifactsFollowEmitIncludeFiles(t *testing.T) {
	for _, test := range []struct {
		name         string
		emitIncludes bool
	}{
		{name: "include enabled", emitIncludes: true},
		{name: "noInclude", emitIncludes: false},
	} {
		t.Run(test.name, func(t *testing.T) {
			// Given: a native Flamework game with runtime config.
			dir := writeProject(t, "noinclude-game", "")
			enableIncrementalBuilds(t, dir)
			task7Write(t, filepath.Join(dir, "rotor.toml"), "[flamework]\n")
			task7Write(t, filepath.Join(dir, "default.project.json"), `{"name":"fixture","tree":{"$className":"DataModel","ReplicatedStorage":{"TS":{"$path":"out"},"rbxts_include":{"$path":"include","node_modules":{"$className":"Folder","@rbxts":{"$path":"node_modules/@rbxts"}}}}}}`)
			task7Write(t, filepath.Join(dir, "flamework.json"), `{"profiling":false}`)

			// When: the project builds with or without include emission.
			if _, diagnostics, err := BuildProjectWithOptions(dir, ProjectOptions{EmitIncludeFiles: test.emitIncludes}); err != nil || len(diagnostics) != 0 {
				t.Fatalf("BuildProjectWithOptions = (%v, %v)", diagnostics, err)
			}

			// Then: include/flamework follows the include switch; build info persists either way.
			for _, name := range []string{"config.json", "globs.json"} {
				_, err := os.Stat(filepath.Join(dir, "include", "flamework", name))
				if test.emitIncludes && name == "config.json" && err != nil {
					t.Fatalf("include/flamework/%s: %v", name, err)
				}
				if !test.emitIncludes && !os.IsNotExist(err) {
					t.Fatalf("include/flamework/%s written under noInclude: %v", name, err)
				}
			}
			if _, err := os.Stat(filepath.Join(dir, "flamework.build")); err != nil {
				t.Fatalf("flamework.build: %v", err)
			}
		})
	}
}
