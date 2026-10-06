package compile

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestSidecarNodeOptions(t *testing.T) {
	for _, test := range []struct {
		name    string
		options string
		want    string
	}{
		{"empty", "", ""},
		{"unrelated options", ` --require "hook with spaces.js"  --trace-warnings `, ` --require "hook with spaces.js"  --trace-warnings `},
		{"equals", `--experimental-package-map=/project/node_modules/.package-map.json`, ""},
		{"quoted path", `--trace-warnings --experimental-package-map="C:\\project with spaces\\.package-map.json" --max-old-space-size=2048`, `--trace-warnings --max-old-space-size=2048`},
		{"separate value", `--experimental-package-map "/project with spaces/.package-map.json" --trace-warnings`, `--trace-warnings`},
		{"quoted option", `"--experimental-package-map=/project with spaces/map.json" --trace-warnings`, `--trace-warnings`},
		{"repeated maps", `--experimental-package-map=a --trace-warnings --experimental-package-map b`, `--trace-warnings`},
		{"empty quoted argument", `--experimental-package-map "" a --trace-warnings`, `--trace-warnings`},
		{"underscore alias", `--experimental_package_map=a --trace-warnings`, `--trace-warnings`},
		{"preserve quoted preload", `--require "C:\\hooks\\with spaces.js" --experimental-package-map=a --trace-warnings`, `--require "C:\\hooks\\with spaces.js" --trace-warnings`},
		{"preserve escaped quote", `--require "hook\"name.js" --experimental-package-map=a`, `--require "hook\"name.js"`},
		{"preserve similar option", `--experimental-package-map-extra=a`, `--experimental-package-map-extra=a`},
		{"preserve flag in path", `--require "hooks/--experimental-package-map=a.js"`, `--require "hooks/--experimental-package-map=a.js"`},
		{"unterminated quote", `--experimental-package-map=a --require "hook`, `--experimental-package-map=a --require "hook`},
		{"invalid escape", `--experimental-package-map=a --require "hook\`, `--experimental-package-map=a --require "hook\`},
		{"missing value", `--experimental-package-map`, `--experimental-package-map`},
		{"missing value before option", `--experimental-package-map --trace-warnings`, `--experimental-package-map --trace-warnings`},
		{"missing value before short option", `--experimental-package-map -r hook.js`, `--experimental-package-map -r hook.js`},
		{"value starting with hyphen", `--experimental-package-map -map.json`, `--experimental-package-map -map.json`},
	} {
		t.Run(test.name, func(t *testing.T) {
			if got := sidecarNodeOptions(test.options); got != test.want {
				t.Fatalf("sidecarNodeOptions(%q) = %q, want %q", test.options, got, test.want)
			}
		})
	}
}

func TestSidecarEnvRemovesPackageMapWithoutNodeModules(t *testing.T) {
	options := `--trace-warnings --experimental-package-map="/project with spaces/map.json"`
	t.Setenv("NODE_OPTIONS", options)
	root := t.TempDir()
	env := sidecarEnv(filepath.Join(root, "project"), filepath.Join(root, "sidecar"))
	if got := envValue(env, "NODE_OPTIONS"); got != "--trace-warnings" {
		t.Fatalf("child NODE_OPTIONS = %q, want --trace-warnings", got)
	}
	if got := os.Getenv("NODE_OPTIONS"); got != options {
		t.Fatalf("parent NODE_OPTIONS = %q, want %q", got, options)
	}
}

func TestSidecarSessionWithInheritedPackageMap(t *testing.T) {
	for _, projectTypeScript := range []bool{true, false} {
		name := "sidecar TypeScript fallback"
		if projectTypeScript {
			name = "project TypeScript"
		}
		t.Run(name, func(t *testing.T) {
			dir := filepath.Join(t.TempDir(), "project with spaces")
			pluginDir := filepath.Join(dir, "node_modules", "test-transformer")
			if err := os.MkdirAll(pluginDir, 0o755); err != nil {
				t.Fatal(err)
			}
			if projectTypeScript {
				symlinkFixtureTypeScript(t, dir)
			}
			for file, contents := range map[string]string{
				"package.json":                           `{"name":"mapped-project"}`,
				"package-map.json":                       `{"packages":{"project":{"url":"./","dependencies":{}}}}`,
				"tsconfig.json":                          `{"compilerOptions":{"noLib":true,"types":[],"plugins":[{"transform":"test-transformer","prefix":"mapped"}]},"files":["main.ts"]}`,
				"main.ts":                                `export const phase = "start";`,
				"node_modules/test-transformer/index.js": prefixStringPlugin,
			} {
				if err := os.WriteFile(filepath.Join(dir, filepath.FromSlash(file)), []byte(contents), 0o644); err != nil {
					t.Fatal(err)
				}
			}
			options := `--experimental-package-map="` + filepath.ToSlash(filepath.Join(dir, "package-map.json")) + `" --max-old-space-size=2048`
			t.Setenv("NODE_OPTIONS", options)
			session, err := spawnSidecarSession(dir, repoSidecarDir(t))
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(session.close)
			ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			defer cancel()
			response, err := session.roundTrip(ctx, sidecarRequest{
				Protocol:         1,
				Operation:        "transform",
				ProjectDir:       dir,
				TsConfigPath:     filepath.Join(dir, "tsconfig.json"),
				CompileFileNames: []string{filepath.Join(dir, "main.ts")},
				ChangedFiles:     []sidecarChangedFile{},
			})
			if err != nil {
				t.Fatal(err)
			}
			if len(response.Diagnostics) != 0 {
				t.Fatalf("diagnostics: %+v", response.Diagnostics)
			}
			if len(response.Transformed) != 1 || !strings.Contains(response.Transformed[0].Text, `"mapped:start"`) {
				t.Fatalf("transformed: %+v, want mapped:start", response.Transformed)
			}
			if got := os.Getenv("NODE_OPTIONS"); got != options {
				t.Fatalf("parent NODE_OPTIONS = %q, want %q", got, options)
			}
		})
	}
}
