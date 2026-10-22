package compile

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSourceMapPerformanceGoldens(t *testing.T) {
	setRepoSidecarPath(t)
	closeSidecarSessions()
	dir, _, golden := stagePerformanceOutputFixture(t)
	t.Cleanup(closeSidecarSessions)
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) != 0 {
		t.Fatalf("build: %v (diagnostics: %v)", err, diags)
	}
	for _, test := range []struct{ name, want string }{
		{"alpha", ` 1 -      | -- Compiled with sloptor vX
 2 0->3:0 | local alpha = {
 3 0->3:0 | 	label = "alpha",
 4 0->3:0 | 	weight = 1,
 5 0->3:0 | }
 6 -      | return {
 7 0->3:0 | 	alpha = alpha,
 8 -      | }
`},
		{"beta", ` 1 -      | -- Compiled with sloptor vX
 2 0->6:0 | local beta = {
 3 0->6:0 | 	label = "beta",
 4 0->6:0 | 	weight = 2,
 5 0->6:0 | }
 6 -      | return {
 7 0->6:0 | 	beta = beta,
 8 -      | }
`},
	} {
		t.Run(test.name, func(t *testing.T) {
			path := "out/" + test.name + ".luau"
			read := func(relative string) string {
				t.Helper()
				contents, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(relative)))
				if err != nil {
					t.Fatal(err)
				}
				return string(contents)
			}
			text, mapJSON := read(path), read(path+".map")
			if text != string(golden[path]) {
				t.Fatal("performance fixture Luau bytes changed")
			}
			// TypeScript's source reader strips a UTF-8 BOM before parsing;
			// sourcesContent retains that decoded text, including its CRLFs.
			source := strings.TrimPrefix(read("src/"+test.name+".ts"), "\uFEFF")
			assertSourceMapContract(t, text, mapJSON, source)
			if got := annotateSourceMap(text, mapJSON); got != test.want {
				t.Errorf("performance map:\ngot:\n%swant:\n%s", got, test.want)
			}
			if mapJSON != string(golden[path+".map"]) {
				t.Errorf("stale performance map golden: %s", path+".map")
			}
		})
	}
}
