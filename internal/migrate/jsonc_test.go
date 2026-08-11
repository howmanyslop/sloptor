package migrate

import (
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

func TestPlanFlameworkTSConfig_materializes_effective_plugins_and_preserves_target_JSONC(t *testing.T) {
	// Given
	dir := t.TempDir()
	base := filepath.Join(dir, "base.json")
	target := filepath.Join(dir, "tsconfig.base.json")
	writeMigrationFixture(t, base, `{
  "compilerOptions": {
    "plugins": [
      { "transform": "first" },
      {
        "transform": "rbxts-transformer-flamework",
        "noSemanticDiagnostics": true,
        "salt": "fixture",
        "hashPrefix": "game",
        "preloadIds": true,
        "obfuscation": true,
        "idGenerationMode": "tiny",
        "optimizations": { "guardGenerationDedupLimit": 7 }
      },
      { "transform": "last", "afterDeclarations": true }
    ]
  }
}
`)
	original := "\ufeff{\r\n\t// keep this comment\r\n\t\"extends\": \"./base.json\",\r\n\t\"include\": [\"src\"],\r\n}\r\n"
	writeMigrationFixture(t, target, original)

	// When
	change, err := PlanFlameworkTSConfig(target)
	// Then
	if err != nil {
		t.Fatalf("PlanFlameworkTSConfig() error = %v", err)
	}
	if change.Path != target || string(change.Original) != original {
		t.Fatalf("change source = (%q, %q), want (%q, original bytes)", change.Path, change.Original, target)
	}
	if change.Status != TSConfigChangePlanned {
		t.Fatalf("change.Status = %q, want %q", change.Status, TSConfigChangePlanned)
	}
	if got := change.Options; got.After != "first" || !got.NoSemanticDiagnostics || got.Salt != "fixture" || got.HashPrefix != "game" || !got.PreloadIDs || !got.Obfuscation || got.IDGenerationMode != "tiny" || got.Optimizations.GuardGenerationDedupLimit == nil || *got.Optimizations.GuardGenerationDedupLimit != 7 {
		t.Fatalf("change.Options = %#v, want all supported options and after=first", got)
	}
	updated := string(change.Updated)
	for _, want := range []string{"\ufeff{\r\n", "\t// keep this comment\r\n", `"transform": "first"`, `"transform": "last"`, "\r\n}\r\n"} {
		if !strings.Contains(updated, want) {
			t.Fatalf("updated JSONC missing %q:\n%s", want, updated)
		}
	}
	if strings.Contains(updated, "rbxts-transformer-flamework") {
		t.Fatalf("updated JSONC retains Flamework plugin:\n%s", updated)
	}
}

func TestPlanFlameworkTSConfig_removes_local_plugin_without_reformatting_file(t *testing.T) {
	// Given
	dir := t.TempDir()
	target := filepath.Join(dir, "tsconfig.json")
	original := `{
    "compilerOptions": {
        // plugin order is intentional
        "plugins": [
            { "transform": "before" },
            // retain the migration note
            { "transform": "rbxts-transformer-flamework", "salt": "x" } /* retain, separator note */,
            { "transform": "after" },
        ],
        "strict": true,
    },
}
`
	writeMigrationFixture(t, target, original)

	// When
	change, err := PlanFlameworkTSConfig(target)
	// Then
	if err != nil {
		t.Fatalf("PlanFlameworkTSConfig() error = %v", err)
	}
	updated := string(change.Updated)
	for _, want := range []string{"// plugin order is intentional", "// retain the migration note", "/* retain, separator note */", `"strict": true`, `"transform": "before"`, `"transform": "after"`, "        \"plugins\"", "        ],"} {
		if !strings.Contains(updated, want) {
			t.Fatalf("updated JSONC missing %q:\n%s", want, updated)
		}
	}
	if strings.Contains(updated, "rbxts-transformer-flamework") {
		t.Fatalf("updated JSONC retains Flamework plugin:\n%s", updated)
	}
	writeMigrationFixture(t, target, updated)
	if _, err := PlanFlameworkTSConfig(target); !errors.Is(err, ErrNoFlameworkPlugin) {
		t.Fatalf("updated JSONC parse error = %v, want only no-plugin status", err)
	}
}

func TestPlanFlameworkTSConfig_rejects_ambiguous_or_lossy_inputs(t *testing.T) {
	tests := []struct {
		name    string
		files   map[string]string
		target  string
		wantErr string
		cause   error
	}{
		{name: "missing extends", files: map[string]string{"tsconfig.json": `{"extends":"./missing"}`}, target: "tsconfig.json", wantErr: "missing"},
		{name: "extends cycle", files: map[string]string{"tsconfig.json": `{"extends":"./base.json"}`, "base.json": `{"extends":"./tsconfig.json"}`}, target: "tsconfig.json", wantErr: "cycle"},
		{name: "duplicate flamework", files: map[string]string{"tsconfig.json": `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework"},{"transform":"rbxts-transformer-flamework"}]}}`}, target: "tsconfig.json", wantErr: "exactly one", cause: ErrMultipleFlameworkPlugins},
		{name: "unknown future option", files: map[string]string{"tsconfig.json": `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework","futureOption":true}]}}`}, target: "tsconfig.json", wantErr: "unknown"},
		{name: "duplicate option key", files: map[string]string{"tsconfig.json": `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework","salt":"a","salt":"b"}]}}`}, target: "tsconfig.json", wantErr: "duplicate"},
		{name: "lossy transformer phase", files: map[string]string{"tsconfig.json": `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework","after":true}]}}`}, target: "tsconfig.json", wantErr: "cannot represent"},
		{name: "unrepresentable anchor", files: map[string]string{"tsconfig.json": `{"compilerOptions":{"plugins":[{"transform":"same"},{"transform":"same"},{"transform":"rbxts-transformer-flamework"}]}}`}, target: "tsconfig.json", wantErr: "duplicate transformer"},
		{name: "malformed option", files: map[string]string{"tsconfig.json": `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework","optimizations":{"guardGenerationDedupLimit":1.5}}]}}`}, target: "tsconfig.json", wantErr: "integer"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// Given
			dir := t.TempDir()
			for name, contents := range test.files {
				writeMigrationFixture(t, filepath.Join(dir, name), contents)
			}

			// When
			_, err := PlanFlameworkTSConfig(filepath.Join(dir, test.target))

			// Then
			if err == nil || !strings.Contains(err.Error(), test.wantErr) {
				t.Fatalf("PlanFlameworkTSConfig() error = %v, want containing %q", err, test.wantErr)
			}
			if test.cause != nil && !errors.Is(err, test.cause) {
				t.Fatalf("PlanFlameworkTSConfig() error = %v, want cause %v", err, test.cause)
			}
			if errors.Is(err, ErrMultipleFlameworkPlugins) {
				var migrationError *JSONCMigrationError
				if !errors.As(err, &migrationError) || migrationError.Count != 2 {
					t.Fatalf("PlanFlameworkTSConfig() typed error = %#v, want count 2", migrationError)
				}
			}
		})
	}
}

func TestPlanFlameworkTSConfig_resolves_monorepo_and_package_extends(t *testing.T) {
	tests := []struct {
		name        string
		extends     string
		basePath    string
		packageJSON string
	}{
		{name: "parent monorepo config", extends: "../tsconfig.base.json", basePath: "tsconfig.base.json"},
		{name: "node modules package config", extends: "shared-config", basePath: "project/node_modules/shared-config/base.json", packageJSON: `{"tsconfig":"base.json"}`},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			// Given
			dir := t.TempDir()
			target := filepath.Join(dir, "project", "tsconfig.json")
			writeMigrationFixture(t, filepath.Join(dir, test.basePath), `{"compilerOptions":{"plugins":[{"transform":"before"},{"transform":"rbxts-transformer-flamework"}]}}`)
			if test.packageJSON != "" {
				writeMigrationFixture(t, filepath.Join(dir, "project", "node_modules", "shared-config", "package.json"), test.packageJSON)
			}
			writeMigrationFixture(t, target, `{"extends":`+strconv.Quote(test.extends)+`}`)

			// When
			change, err := PlanFlameworkTSConfig(target)
			// Then
			if err != nil {
				t.Fatalf("PlanFlameworkTSConfig() error = %v", err)
			}
			compactUpdated := strings.ReplaceAll(string(change.Updated), " ", "")
			if change.Options.After != "before" || !strings.Contains(compactUpdated, `"transform":"before"`) {
				t.Fatalf("resolved migration = (%#v, %s), want inherited plugin materialized", change.Options, change.Updated)
			}
		})
	}
}

func TestPlanFlameworkTSConfig_reports_unchanged_after_migration(t *testing.T) {
	// Given
	dir := t.TempDir()
	target := filepath.Join(dir, "tsconfig.json")
	writeMigrationFixture(t, target, `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework"}]}}`)
	first, err := PlanFlameworkTSConfig(target)
	if err != nil {
		t.Fatalf("first PlanFlameworkTSConfig() error = %v", err)
	}
	writeMigrationFixture(t, target, string(first.Updated))

	// When
	_, err = PlanFlameworkTSConfig(target)

	// Then
	if err == nil || !errors.Is(err, ErrNoFlameworkPlugin) {
		t.Fatalf("second PlanFlameworkTSConfig() error = %v, want exactly one", err)
	}
	var migrationError *JSONCMigrationError
	if !errors.As(err, &migrationError) || migrationError.Count != 0 {
		t.Fatalf("second PlanFlameworkTSConfig() typed error = %#v, want count 0", migrationError)
	}
}

func TestPlanFlameworkTSConfig_rejects_unterminated_block_comment_after_root(t *testing.T) {
	// Given
	dir := t.TempDir()
	target := filepath.Join(dir, "tsconfig.json")
	writeMigrationFixture(t, target, `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework"}]}} /*`)

	// When
	_, err := PlanFlameworkTSConfig(target)

	// Then
	if err == nil {
		t.Fatal("PlanFlameworkTSConfig() error = nil, want unterminated block comment error")
	}
	var migrationError *JSONCMigrationError
	if !errors.As(err, &migrationError) {
		t.Fatalf("PlanFlameworkTSConfig() error = %v, want *JSONCMigrationError", err)
	}
	if migrationError.Path != target || !strings.Contains(migrationError.Reason, "unterminated block comment") {
		t.Fatalf("PlanFlameworkTSConfig() error = %#v, want target path and unterminated block comment", migrationError)
	}
}

func TestPlanFlameworkTSConfig_rejects_unterminated_block_comments_inside_trivia(t *testing.T) {
	for _, test := range []struct {
		name  string
		input string
	}{
		{name: "before root", input: `/*{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework"}]}}`},
		{name: "inside object", input: `{"compilerOptions" /*`},
		{name: "inside array", input: `{"compilerOptions":{"plugins":[/*`},
	} {
		t.Run(test.name, func(t *testing.T) {
			// Given
			dir := t.TempDir()
			target := filepath.Join(dir, "tsconfig.json")
			writeMigrationFixture(t, target, test.input)

			// When
			_, err := PlanFlameworkTSConfig(target)

			// Then
			var migrationError *JSONCMigrationError
			if !errors.As(err, &migrationError) || !strings.Contains(migrationError.Reason, "unterminated block comment") {
				t.Fatalf("PlanFlameworkTSConfig() error = %v, want typed unterminated block comment error", err)
			}
		})
	}
}

func TestPlanFlameworkTSConfig_accepts_terminated_block_comment_after_root(t *testing.T) {
	// Given
	dir := t.TempDir()
	target := filepath.Join(dir, "tsconfig.json")
	writeMigrationFixture(t, target, `{"compilerOptions":{"plugins":[{"transform":"rbxts-transformer-flamework"}]}} /* closed */`)

	// When
	change, err := PlanFlameworkTSConfig(target)
	// Then
	if err != nil {
		t.Fatalf("PlanFlameworkTSConfig() error = %v", err)
	}
	if change.Status != TSConfigChangePlanned {
		t.Fatalf("change.Status = %q, want %q", change.Status, TSConfigChangePlanned)
	}
}

func writeMigrationFixture(t *testing.T, path, contents string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
}
