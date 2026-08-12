package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"rotor/internal/compile"
	"rotor/internal/config"
)

func appendTransformerDoctorChecks(checks []doctorCheck, nodeModules string, hasNodeModules bool, transforms []string) []doctorCheck {
	nodeStatus := doctorInfo
	nodeHint := ""
	if len(transforms) > 0 {
		nodeStatus = doctorFail
		nodeHint = "external transformer plugins require Node.js (https://nodejs.org)"
	}
	nodeVersion, nodeOK := toolVersion("node", "--version")
	if nodeOK {
		checks = append(checks, doctorCheck{status: doctorOK, label: "Node.js", detail: nodeVersion})
	} else {
		checks = append(checks, doctorCheck{status: nodeStatus, label: "Node.js", detail: "not on PATH", hint: nodeHint})
	}
	if len(transforms) == 0 {
		return checks
	}
	if hasNodeModules {
		checks = append(checks, packageCheck(nodeModules, "typescript", doctorFail,
			"external transformer plugins resolve the project's own typescript package; npm install -D typescript"))
	}
	for _, name := range transforms {
		if dirExists(filepath.Join(nodeModules, filepath.FromSlash(name))) {
			checks = append(checks, doctorCheck{status: doctorOK, label: "transformer " + name, detail: "installed"})
			continue
		}
		checks = append(checks, doctorCheck{status: doctorWarn, label: "transformer " + name, detail: "not found in node_modules", hint: "npm install -D " + name})
	}
	if sidecarDir, err := compile.ResolveSidecarDir(); err == nil {
		checks = append(checks, doctorCheck{status: doctorOK, label: "transformer sidecar", detail: sidecarDir})
	} else {
		checks = append(checks, doctorCheck{status: doctorFail, label: "transformer sidecar", detail: err.Error(), hint: "the embedded worker could not be extracted to the user cache dir"})
	}
	return checks
}

func nativeFlameworkCheck(dir string) (doctorCheck, bool) {
	cfg, err := config.Load(dir)
	if err != nil || cfg.Flamework == nil {
		return doctorCheck{}, false
	}
	if validationErrors := cfg.ValidateFlamework(); len(validationErrors) > 0 {
		return doctorCheck{status: doctorFail, label: "native Flamework", detail: validationErrors[0].Error(), hint: "correct [flamework] in rotor.toml before building"}, true
	}
	return doctorCheck{status: doctorOK, label: "native Flamework", detail: "enabled (no Node.js sidecar required)"}, true
}

func legacyFlameworkPluginConfigured(transforms []string) bool {
	for _, transform := range transforms {
		if transform == "rbxts-transformer-flamework" {
			return true
		}
	}
	return false
}

func legacyFlameworkDoctorCheck(nativeEnabled bool) doctorCheck {
	detail := "rbxts-transformer-flamework is no longer supported; run `sloptor migrate flamework`"
	if nativeEnabled {
		detail = "native [flamework] cannot be combined with rbxts-transformer-flamework; remove it from tsconfig.json"
	}
	return doctorCheck{status: doctorFail, label: "transformer rbxts-transformer-flamework", detail: detail}
}

func tsconfigTransformerPlugins(path string) []string {
	plugins, err := inspectTransformerPlugins(path)
	if err != nil {
		return nil
	}
	return plugins.transforms
}

type transformerPluginInspection struct{ transforms []string }

type tsconfigPluginDeclaration struct {
	extends    string
	declared   bool
	transforms []string
}

func inspectTransformerPlugins(tsConfigPath string) (transformerPluginInspection, error) {
	path := tsConfigPath
	seen := map[string]struct{}{}
	for {
		resolved, err := filepath.Abs(path)
		if err != nil {
			return transformerPluginInspection{}, fmt.Errorf("resolving tsconfig %q: %w", path, err)
		}
		if _, found := seen[resolved]; found {
			return transformerPluginInspection{}, fmt.Errorf("%s: circular extends chain", filepath.Base(resolved))
		}
		seen[resolved] = struct{}{}
		declaration, err := readTSConfigPluginDeclaration(resolved)
		if err != nil {
			return transformerPluginInspection{}, err
		}
		if declaration.declared {
			return transformerPluginInspection{transforms: declaration.transforms}, nil
		}
		if declaration.extends == "" {
			return transformerPluginInspection{}, nil
		}
		path, err = resolveTSConfigExtends(resolved, declaration.extends)
		if err != nil {
			return transformerPluginInspection{}, err
		}
	}
}

func readTSConfigPluginDeclaration(path string) (tsconfigPluginDeclaration, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return tsconfigPluginDeclaration{}, fmt.Errorf("%s: reading tsconfig: %w", filepath.Base(path), err)
	}
	var root map[string]json.RawMessage
	if err := json.Unmarshal([]byte(compile.StripJSONC(string(data))), &root); err != nil {
		return tsconfigPluginDeclaration{}, fmt.Errorf("%s: invalid JSON: %w", filepath.Base(path), err)
	}
	declaration := tsconfigPluginDeclaration{}
	if rawExtends, found := root["extends"]; found {
		if err := json.Unmarshal(rawExtends, &declaration.extends); err != nil || declaration.extends == "" {
			return tsconfigPluginDeclaration{}, fmt.Errorf("%s: extends must be a non-empty string", filepath.Base(path))
		}
	}
	rawOptions, found := root["compilerOptions"]
	if !found {
		return declaration, nil
	}
	var options map[string]json.RawMessage
	if err := json.Unmarshal(rawOptions, &options); err != nil {
		return tsconfigPluginDeclaration{}, fmt.Errorf("%s: compilerOptions must be an object", filepath.Base(path))
	}
	rawPlugins, found := options["plugins"]
	if !found {
		return declaration, nil
	}
	var plugins []json.RawMessage
	if err := json.Unmarshal(rawPlugins, &plugins); err != nil {
		return tsconfigPluginDeclaration{}, fmt.Errorf("%s: compilerOptions.plugins must be an array", filepath.Base(path))
	}
	declaration.declared = true
	for index, rawPlugin := range plugins {
		var plugin map[string]json.RawMessage
		if err := json.Unmarshal(rawPlugin, &plugin); err != nil {
			return tsconfigPluginDeclaration{}, fmt.Errorf("%s: compilerOptions.plugins[%d] must be an object", filepath.Base(path), index)
		}
		rawTransform, found := plugin["transform"]
		if !found {
			continue
		}
		var transform string
		if err := json.Unmarshal(rawTransform, &transform); err != nil || transform == "" {
			return tsconfigPluginDeclaration{}, fmt.Errorf("%s: compilerOptions.plugins[%d].transform must be a non-empty string", filepath.Base(path), index)
		}
		declaration.transforms = append(declaration.transforms, transform)
	}
	return declaration, nil
}

func resolveTSConfigExtends(path, extends string) (string, error) {
	if filepath.IsAbs(extends) || strings.HasPrefix(extends, ".") {
		candidate := extends
		if !filepath.IsAbs(candidate) {
			candidate = filepath.Join(filepath.Dir(path), candidate)
		}
		if resolved, found := resolveDoctorConfigFile(candidate); found {
			return resolved, nil
		}
	} else {
		for dir := filepath.Dir(path); ; dir = filepath.Dir(dir) {
			if resolved, found := resolveDoctorConfigFile(filepath.Join(dir, "node_modules", filepath.FromSlash(extends))); found {
				return resolved, nil
			}
			parent := filepath.Dir(dir)
			if parent == dir {
				break
			}
		}
	}
	return "", fmt.Errorf("%s: extends %q could not be resolved", filepath.Base(path), extends)
}

func resolveDoctorConfigFile(path string) (string, bool) {
	for _, candidate := range []string{path, path + ".json"} {
		if fileExists(candidate) {
			return filepath.Clean(candidate), true
		}
	}
	data, err := os.ReadFile(filepath.Join(path, "package.json"))
	if err != nil {
		return "", false
	}
	var pkg struct {
		Main string `json:"main"`
	}
	if json.Unmarshal(data, &pkg) != nil || pkg.Main == "" {
		return "", false
	}
	return resolveDoctorConfigFile(filepath.Join(path, filepath.FromSlash(pkg.Main)))
}
