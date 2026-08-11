package config

import (
	"encoding/json"
	"os"
	"regexp"
	"strings"
	"testing"
)

func TestLoadFlameworkConfig(t *testing.T) {
	dir := t.TempDir()
	writeConfigFile(t, dir, ConfigFileName, `[flamework]
after = "rbxts-transform-env"
noSemanticDiagnostics = true
obfuscation = true
hashPrefix = "game"
salt = "salt"
preloadIds = true

[flamework.optimizations]
guardGenerationDedupLimit = 0
`)

	cfg, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Flamework == nil {
		t.Fatal("flamework section missing")
	}
	if cfg.Flamework.IDGenerationMode != "obfuscated" {
		t.Fatalf("flamework.idGenerationMode = %q, want obfuscated", cfg.Flamework.IDGenerationMode)
	}
	if cfg.Flamework.After != "rbxts-transform-env" || !cfg.Flamework.NoSemanticDiagnostics ||
		!cfg.Flamework.Obfuscation || cfg.Flamework.HashPrefix != "game" ||
		cfg.Flamework.Salt != "salt" || !cfg.Flamework.PreloadIDs ||
		cfg.Flamework.Optimizations.GuardGenerationDedupLimit == nil ||
		*cfg.Flamework.Optimizations.GuardGenerationDedupLimit != 0 {
		t.Fatalf("flamework = %+v", cfg.Flamework)
	}
}

func TestLoadFlameworkGuardGenerationDedupLimitPresence(t *testing.T) {
	for _, tc := range []struct {
		name        string
		content     string
		wantPresent bool
	}{
		{
			name: "explicit zero is present",
			content: `[flamework.optimizations]
guardGenerationDedupLimit = 0
`,
			wantPresent: true,
		},
		{
			name:        "omitted is absent",
			content:     "[flamework.optimizations]\n",
			wantPresent: false,
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			dir := t.TempDir()
			writeConfigFile(t, dir, ConfigFileName, tc.content)

			cfg, err := Load(dir)
			if err != nil {
				t.Fatal(err)
			}

			limit := cfg.Flamework.Optimizations.GuardGenerationDedupLimit
			if (limit != nil) != tc.wantPresent {
				t.Fatalf("guardGenerationDedupLimit presence = %t, want %t", limit != nil, tc.wantPresent)
			}
			if tc.wantPresent && *limit != 0 {
				t.Fatalf("guardGenerationDedupLimit = %d, want 0", *limit)
			}
		})
	}
}

func TestFlameworkSchema(t *testing.T) {
	var root map[string]any
	if err := json.Unmarshal([]byte(Schema), &root); err != nil {
		t.Fatal(err)
	}
	properties := root["properties"].(map[string]any)
	flamework := properties["flamework"].(map[string]any)
	if flamework["additionalProperties"] != false {
		t.Fatalf("flamework.additionalProperties = %v, want false", flamework["additionalProperties"])
	}
	flameworkProperties := flamework["properties"].(map[string]any)
	idGenerationMode := flameworkProperties["idGenerationMode"].(map[string]any)
	if idGenerationMode["default"] != "full" {
		t.Fatalf("flamework.idGenerationMode.default = %v, want full", idGenerationMode["default"])
	}
	optimizations := flameworkProperties["optimizations"].(map[string]any)
	if optimizations["additionalProperties"] != false {
		t.Fatalf("flamework.optimizations.additionalProperties = %v, want false", optimizations["additionalProperties"])
	}
}

func TestFlameworkSchemaRejectsReservedHashPrefix(t *testing.T) {
	// Given
	var root map[string]any
	if err := json.Unmarshal([]byte(Schema), &root); err != nil {
		t.Fatal(err)
	}
	properties := root["properties"].(map[string]any)
	flamework := properties["flamework"].(map[string]any)
	flameworkProperties := flamework["properties"].(map[string]any)
	hashPrefix := flameworkProperties["hashPrefix"].(map[string]any)
	pattern, ok := hashPrefix["pattern"].(string)
	if !ok {
		t.Fatal("flamework.hashPrefix must define a pattern that rejects the reserved $ prefix")
	}

	// When
	re, err := regexp.Compile(pattern)
	if err != nil {
		t.Fatalf("flamework.hashPrefix.pattern %q is invalid: %v", pattern, err)
	}

	// Then
	if re.MatchString("$internal") {
		t.Fatalf("flamework.hashPrefix.pattern %q accepts a reserved prefix", pattern)
	}
	if !re.MatchString("") {
		t.Fatalf("flamework.hashPrefix.pattern %q rejects the valid empty prefix", pattern)
	}
}

func TestLoadEmptyFlameworkConfigIsPresentWithFullDefault(t *testing.T) {
	dir := t.TempDir()
	writeConfigFile(t, dir, ConfigFileName, "[flamework]\n")

	cfg, err := Load(dir)
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Flamework == nil {
		t.Fatal("empty flamework section must remain present")
	}
	if cfg.Flamework.IDGenerationMode != "full" {
		t.Fatalf("flamework.idGenerationMode = %q, want full", cfg.Flamework.IDGenerationMode)
	}
}

func TestValidateFlameworkConfig(t *testing.T) {
	negative := -1
	zero := 0
	for _, tc := range []struct {
		name string
		cfg  *FlameworkConfig
		want string
	}{
		{"invalid id mode", &FlameworkConfig{IDGenerationMode: "random"}, "idGenerationMode"},
		{"reserved hash prefix", &FlameworkConfig{HashPrefix: "$internal"}, "hashPrefix"},
		{"negative guard dedup limit", &FlameworkConfig{Optimizations: FlameworkOptimizations{GuardGenerationDedupLimit: &negative}}, "guardGenerationDedupLimit"},
		{"zero guard dedup limit", &FlameworkConfig{Optimizations: FlameworkOptimizations{GuardGenerationDedupLimit: &zero}}, ""},
		{"accepted modes", &FlameworkConfig{IDGenerationMode: "tiny"}, ""},
	} {
		t.Run(tc.name, func(t *testing.T) {
			errs := (&Config{Flamework: tc.cfg}).Validate()
			if tc.want == "" {
				if len(errs) != 0 {
					t.Fatalf("Validate() = %v, want clean", errs)
				}
				return
			}
			if len(errs) != 1 || !strings.Contains(errs[0].Error(), tc.want) {
				t.Fatalf("Validate() = %v, want one error containing %q", errs, tc.want)
			}
		})
	}
}

func TestLoadFlameworkRejectsUnknownKeys(t *testing.T) {
	for _, content := range []string{
		"[flamework]\nenabled = true\n",
		"[flamework.optimizations]\nunknown = 1\n",
	} {
		dir := t.TempDir()
		if err := os.WriteFile(dir+"/"+ConfigFileName, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
		if _, err := Load(dir); err == nil || !strings.Contains(err.Error(), "flamework") {
			t.Fatalf("Load(%q) = %v, want Flamework unknown-key error", content, err)
		}
	}
}
