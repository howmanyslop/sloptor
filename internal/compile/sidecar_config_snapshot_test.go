package compile

import (
	"os"
	"path/filepath"
	"testing"

	"rotor/tsgo/vfs/osvfs"
)

func TestProjectConfigSnapshotKeepsParsedRootAndExtends(t *testing.T) {
	dir := t.TempDir()
	configPath := filepath.Join(dir, "tsconfig.json")
	basePath := filepath.Join(dir, "base.json")
	sourcePath := filepath.Join(dir, "main.ts")
	rootBefore := "{\"extends\":[\"./base.json\"],\"files\":[\"main.ts\"]}\n"
	baseBefore := "{\"compilerOptions\":{\"allowSyntheticDefaultImports\":true,\"baseUrl\":\"src\",\"module\":\"CommonJS\",\"moduleDetection\":\"force\",\"moduleResolution\":\"Node\",\"noLib\":true,\"rootDir\":\".\",\"outDir\":\"out\",\"strict\":true,\"typeRoots\":[\"node_modules/@rbxts\"]}}\n"
	if err := os.WriteFile(sourcePath, []byte("export const value = 1;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(basePath, []byte(baseBefore), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(configPath, []byte(rootBefore), 0o644); err != nil {
		t.Fatal(err)
	}

	_, program, diags, err := newProjectProgramWithOptions(dir, "", ProjectOptions{})
	if err != nil {
		t.Fatalf("newProjectProgramWithOptions() error = %v, diagnostics = %v", err, diags)
	}
	snapshot := program.CommandLine().ConfigParseSnapshot()
	if got := snapshot[filepath.ToSlash(configPath)]; got != rootBefore {
		t.Fatalf("root snapshot = %q, want parsed text %q", got, rootBefore)
	}
	if got := snapshot[filepath.ToSlash(basePath)]; got != baseBefore {
		t.Fatalf("extends snapshot = %q, want parsed text %q", got, baseBefore)
	}
	if sanitized := SanitizeTSConfig(baseBefore); sanitized == baseBefore {
		t.Fatal("fixture must exercise a tsconfig option Go sanitizes")
	}

	if err := os.WriteFile(basePath, []byte("{\"compilerOptions\":{\"noLib\":false}}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(configPath, []byte("{\"files\":[\"main.ts\"]}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	snapshot[filepath.ToSlash(configPath)] = "changed by caller"
	if got := program.CommandLine().ConfigParseSnapshot()[filepath.ToSlash(configPath)]; got != rootBefore {
		t.Fatalf("root snapshot changed after parse = %q, want %q", got, rootBefore)
	}
	if got := program.CommandLine().ConfigParseSnapshot()[filepath.ToSlash(basePath)]; got != baseBefore {
		t.Fatalf("extends snapshot changed after parse = %q, want %q", got, baseBefore)
	}
}

func TestConfigSnapshotResolvesPackageExtendsAfterDeletion(t *testing.T) {
	dir := t.TempDir()
	packageDir := filepath.Join(dir, "node_modules", "fixture-config")
	if err := os.MkdirAll(packageDir, 0o755); err != nil {
		t.Fatal(err)
	}
	configPath := filepath.Join(dir, "tsconfig.json")
	packagePath := filepath.Join(dir, "node_modules", "fixture-config", "package.json")
	basePath := filepath.Join(packageDir, "tsconfig.json")
	base := "{\"compilerOptions\":{\"types\":[]}}\n"
	if err := os.WriteFile(filepath.Join(dir, "main.ts"), []byte("export const value = 1;\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(packagePath, []byte("{}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(basePath, []byte(base), 0o644); err != nil {
		t.Fatal(err)
	}
	root := "{\"extends\":\"fixture-config\",\"compilerOptions\":{\"allowSyntheticDefaultImports\":true,\"module\":\"CommonJS\",\"moduleDetection\":\"force\",\"moduleResolution\":\"Node\",\"noLib\":true,\"rootDir\":\".\",\"outDir\":\"out\",\"strict\":true,\"typeRoots\":[\"node_modules/@rbxts\"]},\"files\":[\"main.ts\"]}\n"
	if err := os.WriteFile(configPath, []byte(root), 0o644); err != nil {
		t.Fatal(err)
	}

	_, program, diags, err := newProjectProgramWithOptions(dir, "", ProjectOptions{})
	if err != nil {
		t.Fatalf("newProjectProgramWithOptions() error = %v, diagnostics = %v", err, diags)
	}
	snapshot := program.CommandLine().ConfigParseSnapshot()
	physicalBasePath := filepath.ToSlash(osvfs.FS().Realpath(basePath))
	if _, ok := snapshot[filepath.ToSlash(packagePath)]; !ok {
		t.Fatalf("package metadata was not captured: %v", snapshot)
	}
	if got := snapshot[filepath.ToSlash(osvfs.FS().Realpath(packagePath))]; got != "{}\n" {
		t.Fatalf("physical package metadata snapshot = %q, want captured package JSON", got)
	}
	if _, ok := snapshot[filepath.ToSlash(basePath)]; !ok {
		t.Fatalf("package config was not captured: %v", snapshot)
	}
	if got := snapshot[physicalBasePath]; got != base {
		t.Fatalf("physical package config snapshot = %q, want %q", got, base)
	}
	if err := os.RemoveAll(filepath.Dir(packagePath)); err != nil {
		t.Fatal(err)
	}
	if resolved, err := resolveExtendedConfig(configPath, "fixture-config", snapshot); err != nil {
		t.Fatalf("resolve snapshot package extends: %v", err)
	} else if resolved != basePath && resolved != filepath.FromSlash(physicalBasePath) {
		t.Fatalf("snapshot package extends = %q, want captured logical or physical config", resolved)
	}

	raw := readRawEnforcedOptionsFromSnapshot(configPath, snapshot)
	if !raw.hasTypes || len(raw.types) != 0 {
		t.Fatalf("package extends raw options = %#v, want inherited empty types", raw)
	}
}

func TestLegacyTransformerDetectionUsesEffectiveArrayPlugins(t *testing.T) {
	dir := t.TempDir()
	for name, text := range map[string]string{
		"first.json":    "{\"compilerOptions\":{\"plugins\":[{\"transform\":\"rbxts-transformer-flamework\"}]},\"rbxts\":{\"noInclude\":true}}\n",
		"second.json":   "{\"compilerOptions\":{\"plugins\":[]},\"rbxts\":{\"noInclude\":false}}\n",
		"main.ts":       "export const value = 1;\n",
		"tsconfig.json": "{\"extends\":[\"./first.json\",\"./second.json\"],\"compilerOptions\":{\"allowSyntheticDefaultImports\":true,\"module\":\"CommonJS\",\"moduleDetection\":\"force\",\"moduleResolution\":\"Node\",\"noLib\":true,\"rootDir\":\".\",\"outDir\":\"out\",\"strict\":true,\"typeRoots\":[\"node_modules/@rbxts\"]},\"files\":[\"main.ts\"]}\n",
	} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(text), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	_, program, diags, err := newProjectProgramWithOptions(dir, "", ProjectOptions{})
	if err != nil {
		t.Fatalf("newProjectProgramWithOptions() error = %v, diagnostics = %v", err, diags)
	}
	if projectUsesLegacyFlameworkTransformer(program.CommandLine()) {
		t.Fatal("legacy transformer detection used an overridden array ancestor")
	}
	if projectUsesTransformerPlugins(program.CommandLine()) {
		t.Fatal("transformer gate used an overridden array ancestor")
	}
	opts, err := ReadRbxtsOptions(filepath.Join(dir, "tsconfig.json"))
	if err != nil {
		t.Fatal(err)
	}
	if opts.NoInclude == nil || *opts.NoInclude {
		t.Fatalf("array rbxts merge = %#v, want later noInclude false", opts)
	}
}
