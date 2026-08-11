package config

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/BurntSushi/toml"
	"github.com/dop251/goja"
	"github.com/evanw/esbuild/pkg/api"
)

// ErrNotFound is returned by Load when the project has no rotor.toml.
// Callers treat the config as optional.
var ErrNotFound = errors.New("rotor.toml not found")

// ConfigFileName is the primary config file rotor loads.
const ConfigFileName = "rotor.toml"

// legacyConfigFileNames lists accepted legacy (TypeScript/JavaScript) config
// file names, in priority order. They are read only by `rotor migrate`.
var legacyConfigFileNames = []string{"rotor.config.ts", "rotor.config.js"}

// evalTimeout bounds legacy config evaluation so a buggy config (infinite
// loop) cannot hang the CLI.
const evalTimeout = 10 * time.Second

// virtualModuleSource is what the virtual "rotor/config" module resolves to
// (legacy TypeScript path only).
const virtualModuleSource = `export const defineConfig = (c) => c;`

// Load reads the project's rotor.toml into a *Config.
//
// Decoding is via github.com/BurntSushi/toml into the typed structs (toml
// tags). Unknown keys are tolerated for forward compatibility: BurntSushi's
// MetaData.Undecoded() reports them, and Load records each as a warning on the
// returned Config rather than failing.
//
// Returns (nil, ErrNotFound) when rotor.toml does not exist. Validation
// (enums, price >= 0, ...) is left to Config.Validate, exactly as before.
func Load(projectDir string) (*Config, error) {
	absDir, err := filepath.Abs(projectDir)
	if err != nil {
		return nil, fmt.Errorf("config: resolving project dir: %w", err)
	}

	configPath := filepath.Join(absDir, ConfigFileName)
	if info, err := os.Stat(configPath); err != nil || info.IsDir() {
		return nil, ErrNotFound
	}

	data, err := os.ReadFile(configPath)
	if err != nil {
		return nil, fmt.Errorf("config: reading %s: %w", ConfigFileName, err)
	}

	cfg := &Config{}
	meta, err := toml.Decode(string(data), cfg)
	if err != nil {
		return nil, fmt.Errorf("config: %s: %w", ConfigFileName, err)
	}
	if err := validateFlameworkTOMLKeys(meta.Undecoded()); err != nil {
		return nil, fmt.Errorf("config: %s: %w", ConfigFileName, err)
	}
	cfg.applyFlameworkDefaults()

	// Unknown keys are forward-compatible: collect them as sorted warnings,
	// skipping the taplo schema directive that lives as a leading comment (it
	// is not a key, so it never appears here, but keep this robust).
	var warnings []string
	undecoded := meta.Undecoded()
	keys := make([]string, 0, len(undecoded))
	for _, k := range undecoded {
		keys = append(keys, k.String())
	}
	sort.Strings(keys)
	for _, k := range keys {
		warnings = append(warnings, fmt.Sprintf("%s: unknown key %q (ignored)", ConfigFileName, k))
	}
	cfg.Warnings = warnings
	return cfg, nil
}

// MarshalTOML encodes cfg as a TOML document body (no #:schema directive — the
// caller prepends that). It is used by `rotor migrate` to serialize a config
// loaded from the legacy TypeScript path. The Warnings field carries the
// `toml:"-"` tag and is omitted.
func MarshalTOML(cfg *Config) (string, error) {
	var buf strings.Builder
	enc := toml.NewEncoder(&buf)
	enc.Indent = ""
	if err := enc.Encode(cfg); err != nil {
		return "", err
	}
	return buf.String(), nil
}

// LoadLegacyTS finds and evaluates the project's legacy rotor.config.ts (or
// rotor.config.js). It is the ONLY remaining user of the goja/esbuild
// pipeline and is called exclusively by `rotor migrate` to convert an old
// config to rotor.toml.
//
// Pipeline: esbuild bundles the config (Bundle=true, Platform=neutral,
// Format=CommonJS, Target=ES2017 so the output is goja-safe, inline
// sourcemap), with a plugin that resolves the virtual module "rotor/config"
// and rejects bare npm imports. The bundle is then evaluated in goja and the
// default export is converted into *Config.
//
// CommonJS was chosen over IIFE because extracting the export is trivial: we
// pre-define `module`/`exports` in the goja runtime and read
// `module.exports.default` (or `module.exports` itself for configs written
// as plain CommonJS).
//
// Returns (nil, ErrNotFound) when no legacy config file exists. Non-fatal
// issues (unknown top-level keys) are reported via the returned Config's
// Warnings field.
func LoadLegacyTS(projectDir string) (*Config, error) {
	absDir, err := filepath.Abs(projectDir)
	if err != nil {
		return nil, fmt.Errorf("config: resolving project dir: %w", err)
	}

	configPath := ""
	for _, name := range legacyConfigFileNames {
		candidate := filepath.Join(absDir, name)
		if info, err := os.Stat(candidate); err == nil && !info.IsDir() {
			configPath = candidate
			break
		}
	}
	if configPath == "" {
		return nil, ErrNotFound
	}

	bundle, err := bundleConfig(absDir, configPath)
	if err != nil {
		return nil, err
	}

	raw, err := evaluateBundle(configPath, bundle)
	if err != nil {
		return nil, err
	}

	return decodeConfig(configPath, raw)
}

// bundleConfig runs esbuild over the config entry point and returns the
// bundled CommonJS source (with an inline sourcemap for goja stack traces).
func bundleConfig(projectDir, configPath string) (string, error) {
	configBase := filepath.Base(configPath)

	plugin := api.Plugin{
		Name: "rotor-config",
		Setup: func(build api.PluginBuild) {
			// Resolve the virtual config module in-memory. Accepted
			// specifiers: the npm package name (canonical — configs write
			// `import { defineConfig } from "@rotor-rbx/rotor"`), its
			// /config subpath, and the original short "rotor/config".
			build.OnResolve(api.OnResolveOptions{Filter: `^(rotor/config|@rotor-rbx/rotor(/config)?)$`},
				func(args api.OnResolveArgs) (api.OnResolveResult, error) {
					return api.OnResolveResult{
						Path:      "rotor/config",
						Namespace: "rotor-virtual",
					}, nil
				})
			build.OnLoad(api.OnLoadOptions{Filter: `.*`, Namespace: "rotor-virtual"},
				func(args api.OnLoadArgs) (api.OnLoadResult, error) {
					contents := virtualModuleSource
					return api.OnLoadResult{
						Contents: &contents,
						Loader:   api.LoaderJS,
					}, nil
				})
			// Reject bare (npm) imports. Relative and absolute paths fall
			// through to esbuild's default resolver; "rotor/config" was
			// already claimed by the callback above.
			build.OnResolve(api.OnResolveOptions{Filter: `^[^./]`},
				func(args api.OnResolveArgs) (api.OnResolveResult, error) {
					if args.Kind == api.ResolveEntryPoint || filepath.IsAbs(args.Path) {
						return api.OnResolveResult{}, nil
					}
					return api.OnResolveResult{}, fmt.Errorf(
						"npm imports are not supported in rotor.config.ts (cannot import %q); only relative imports of project files and the virtual \"@rotor-rbx/rotor\" module are allowed",
						args.Path,
					)
				})
		},
	}

	result := api.Build(api.BuildOptions{
		EntryPoints:   []string{configPath},
		AbsWorkingDir: projectDir,
		Bundle:        true,
		Write:         false,
		Platform:      api.PlatformNeutral,
		Format:        api.FormatCommonJS,
		Target:        api.ES2017,
		Sourcemap:     api.SourceMapInline,
		Plugins:       []api.Plugin{plugin},
		LogLevel:      api.LogLevelSilent,
	})

	if len(result.Errors) > 0 {
		msgs := make([]string, 0, len(result.Errors))
		for _, m := range result.Errors {
			if m.Location != nil {
				msgs = append(msgs, fmt.Sprintf("%s:%d:%d: %s",
					m.Location.File, m.Location.Line, m.Location.Column, m.Text))
			} else {
				msgs = append(msgs, m.Text)
			}
		}
		return "", fmt.Errorf("config: %s: %s", configBase, strings.Join(msgs, "; "))
	}
	if len(result.OutputFiles) == 0 {
		return "", fmt.Errorf("config: %s: esbuild produced no output", configBase)
	}
	return string(result.OutputFiles[0].Contents), nil
}

// evaluateBundle runs the bundled CommonJS source in goja and returns the
// default-exported config object as a plain Go map.
//
// The bundle carries an inline sourcemap and goja's parser decodes
// data:application/json;base64 sourceMappingURL comments by default, so
// runtime error positions point at the original .ts file and line.
func evaluateBundle(configPath, bundle string) (map[string]any, error) {
	configBase := filepath.Base(configPath)

	prog, err := goja.Compile(configPath, bundle, false)
	if err != nil {
		return nil, fmt.Errorf("config: %s: compiling bundled config: %w", configBase, err)
	}

	vm := goja.New()
	timer := time.AfterFunc(evalTimeout, func() {
		vm.Interrupt(fmt.Sprintf("config evaluation exceeded %s", evalTimeout))
	})
	defer timer.Stop()

	moduleObj := vm.NewObject()
	exportsObj := vm.NewObject()
	if err := moduleObj.Set("exports", exportsObj); err != nil {
		return nil, fmt.Errorf("config: %s: %w", configBase, err)
	}
	if err := vm.Set("module", moduleObj); err != nil {
		return nil, fmt.Errorf("config: %s: %w", configBase, err)
	}
	if err := vm.Set("exports", exportsObj); err != nil {
		return nil, fmt.Errorf("config: %s: %w", configBase, err)
	}

	if _, err := vm.RunProgram(prog); err != nil {
		return nil, fmt.Errorf("config: %s: evaluating config: %w", configBase, evalError(err))
	}

	exported := moduleObj.Get("exports")
	if exported == nil || goja.IsUndefined(exported) || goja.IsNull(exported) {
		return nil, fmt.Errorf("config: %s: config file has no exports; expected `export default defineConfig({...})`", configBase)
	}

	// Prefer the ES default export; fall back to module.exports itself for
	// configs written as plain CommonJS (`module.exports = {...}`).
	value := exported
	if obj := exported.ToObject(vm); obj != nil {
		if def := obj.Get("default"); def != nil && !goja.IsUndefined(def) && !goja.IsNull(def) {
			value = def
		}
	}

	var raw map[string]any
	if err := vm.ExportTo(value, &raw); err != nil {
		return nil, fmt.Errorf("config: %s: default export must be an object: %w", configBase, err)
	}
	return raw, nil
}

// evalError unwraps goja exceptions so the message includes the JS error and
// its stack (positions are sourcemapped back to the original .ts file).
func evalError(err error) error {
	var exc *goja.Exception
	if errors.As(err, &exc) {
		return errors.New(exc.String())
	}
	return err
}

// knownTopLevelKeys are the config sections rotor understands today.
// Anything else warns (forward compatibility) instead of failing.
var knownTopLevelKeys = map[string]bool{
	"assets":    true,
	"deploy":    true,
	"flamework": true,
}

// decodeConfig converts the raw exported object into the typed Config via an
// encoding/json round-trip (camelCase keys map through the struct json tags),
// recording warnings for unknown top-level keys.
func decodeConfig(configPath string, raw map[string]any) (*Config, error) {
	configBase := filepath.Base(configPath)

	var warnings []string
	unknown := make([]string, 0)
	for key := range raw {
		if !knownTopLevelKeys[key] {
			unknown = append(unknown, key)
		}
	}
	sort.Strings(unknown)
	for _, key := range unknown {
		warnings = append(warnings, fmt.Sprintf("%s: unknown top-level key %q (ignored)", configBase, key))
	}

	data, err := json.Marshal(raw)
	if err != nil {
		return nil, fmt.Errorf("config: %s: config contains values that cannot be represented (functions, cycles, ...): %w", configBase, err)
	}
	cfg := &Config{}
	if err := json.Unmarshal(data, cfg); err != nil {
		return nil, fmt.Errorf("config: %s: config does not match the expected shape: %w", configBase, err)
	}
	cfg.applyFlameworkDefaults()
	cfg.Warnings = warnings
	return cfg, nil
}
