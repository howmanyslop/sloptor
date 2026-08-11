package main

import (
	"bytes"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"rotor/internal/config"
)

// legacyConfigTS is a representative legacy rotor.config.ts covering assets +
// deploy with nested places, an experience, a gamepass, and a social link.
const legacyConfigTS = `import { defineConfig } from "@rotor-rbx/rotor";

export default defineConfig({
	assets: {
		paths: ["assets/**/*.png", "assets/**/*.ogg"],
		output: { luau: "src/shared/assets.luau", types: "src/shared/assets.d.ts" },
		creator: { type: "group", id: 12345 },
	},
	deploy: {
		environments: {
			prod: {
				universeId: 333,
				places: {
					start: { file: "build/game.rbxl", placeId: 444, name: "Start", maxPlayers: 30, versionType: "saved" },
				},
				experience: { name: "My Game", playability: "public" },
				gamepasses: { vip: { name: "VIP", price: 250, icon: "assets/vip.png" } },
				socials: { discord: { title: "Join", url: "https://discord.gg/x", type: "discord" } },
			},
		},
	},
});
`

func runMigrate(t *testing.T, args []string) (code int, stdout, stderr string) {
	t.Helper()
	var out, errBuf bytes.Buffer
	code = execute(append([]string{"migrate"}, args...), cliStreams{in: strings.NewReader(""), out: &out, err: &errBuf})
	return code, out.String(), errBuf.String()
}

func TestMigrateRoundTrip(t *testing.T) {
	dir := t.TempDir()
	tsPath := filepath.Join(dir, "rotor.config.ts")
	if err := os.WriteFile(tsPath, []byte(legacyConfigTS), 0o644); err != nil {
		t.Fatal(err)
	}

	// Capture the legacy config BEFORE migrating — migrate renames the .ts
	// away, so this must happen first.
	want, err := config.LoadLegacyTS(dir)
	if err != nil {
		t.Fatalf("LoadLegacyTS (pre-migrate): %v", err)
	}

	code, out, errOut := runMigrate(t, []string{"config", dir})
	if code != 0 {
		t.Fatalf("migrate exit %d\nstdout:\n%s\nstderr:\n%s", code, out, errOut)
	}

	// rotor.toml written with the hosted #:schema directive on the first line.
	tomlData, err := os.ReadFile(filepath.Join(dir, "rotor.toml"))
	if err != nil {
		t.Fatalf("rotor.toml not written: %v", err)
	}
	if !strings.HasPrefix(string(tomlData), "#:schema https://") {
		t.Errorf("rotor.toml missing hosted #:schema directive:\n%s", tomlData)
	}

	// The legacy file is renamed to .bak (and no longer present).
	if fileExists(tsPath) {
		t.Error("rotor.config.ts should have been renamed away")
	}
	if !fileExists(tsPath + ".bak") {
		t.Error("rotor.config.ts.bak not created")
	}

	// Round-trip: the migrated rotor.toml loads to the same Config the legacy
	// path produced (ignoring the Warnings field, which is load-context).
	got, err := config.Load(dir)
	if err != nil {
		t.Fatalf("Load(rotor.toml): %v", err)
	}
	want.Warnings, got.Warnings = nil, nil
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("round-trip mismatch:\n got = %+v\nwant = %+v", got, want)
	}
	if errs := got.Validate(); len(errs) != 0 {
		t.Errorf("migrated config does not validate: %v", errs)
	}
}

func TestMigrateNoLegacyConfig(t *testing.T) {
	dir := t.TempDir()
	code, _, errOut := runMigrate(t, []string{"config", dir})
	if code != 1 {
		t.Fatalf("migrate with no legacy config: exit %d, want 1", code)
	}
	if !strings.Contains(errOut, "no rotor.config.ts") {
		t.Fatalf("error should mention the missing legacy config:\n%s", errOut)
	}
}

func TestMigrateRefusesExistingToml(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "rotor.config.ts"), []byte(legacyConfigTS), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "rotor.toml"), []byte("# existing\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// Without --force: refuse, leave everything alone.
	code, _, errOut := runMigrate(t, []string{"config", dir})
	if code != 1 {
		t.Fatalf("migrate over existing rotor.toml: exit %d, want 1", code)
	}
	if !strings.Contains(errOut, "already exists") {
		t.Fatalf("error should mention the existing rotor.toml:\n%s", errOut)
	}
	if got := mustReadFile(t, filepath.Join(dir, "rotor.toml")); got != "# existing\n" {
		t.Errorf("rotor.toml was overwritten without --force:\n%s", got)
	}

	// With --force: overwrite and migrate.
	code, _, errOut = runMigrate(t, []string{"config", dir, "--force"})
	if code != 0 {
		t.Fatalf("migrate --force: exit %d\n%s", code, errOut)
	}
	if got := mustReadFile(t, filepath.Join(dir, "rotor.toml")); got == "# existing\n" {
		t.Error("rotor.toml should have been replaced with --force")
	}
}

func TestMigrateHelp(t *testing.T) {
	code, out, _ := runMigrate(t, []string{"-h"})
	if code != 0 {
		t.Fatalf("migrate -h: exit %d", code)
	}
	if !strings.Contains(out, "config") || !strings.Contains(out, "flamework") {
		t.Errorf("help missing usage:\n%s", out)
	}
}

func TestMigrateRejectsRemovedDirectSyntax(t *testing.T) {
	// Given: a directory that would have been accepted by the old command.
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "rotor.config.ts"), []byte(legacyConfigTS), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: migrate is invoked without a subcommand.
	code, _, _ := runMigrate(t, []string{dir})

	// Then: Cobra rejects the removed syntax without changing the project.
	if code != 1 {
		t.Fatalf("migrate direct syntax exit %d, want usage exit 1", code)
	}
	if !fileExists(filepath.Join(dir, "rotor.config.ts")) || fileExists(filepath.Join(dir, "rotor.toml")) {
		t.Fatal("removed direct syntax mutated the project")
	}
}

func TestMigrateFlameworkUsesExplicitTSConfigFile(t *testing.T) {
	// Given: a non-default tsconfig containing the legacy transformer.
	dir := t.TempDir()
	tsconfigPath := filepath.Join(dir, "tsconfig.base.json")
	data := "{\n  \"compilerOptions\": {\n    \"plugins\": [{ \"transform\": \"rbxts-transformer-flamework\" }]\n  }\n}\n"
	if err := os.WriteFile(tsconfigPath, []byte(data), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "package.json"), []byte(`{"name":"game","packageManager":"npm@11.0.0"}`), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "package-lock.json"), []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// When: the explicit file is passed to the new subcommand.
	code, _, errOut := runMigrate(t, []string{"flamework", tsconfigPath})

	// Then: it is used as a file, not treated as a directory.
	if code != 0 {
		t.Fatalf("migrate flamework explicit file exit %d: %s", code, errOut)
	}
	if strings.Contains(errOut, "not a directory") {
		t.Fatalf("explicit tsconfig was treated as a directory: %s", errOut)
	}
	if got := mustReadFile(t, tsconfigPath); strings.Contains(got, "rbxts-transformer-flamework") {
		t.Fatalf("legacy plugin remains after migration:\n%s", got)
	}
	if !fileExists(filepath.Join(dir, "rotor.toml")) {
		t.Fatal("rotor.toml not created")
	}
}
