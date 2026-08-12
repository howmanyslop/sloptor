package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"path/filepath"

	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	tsjson "rotor/tsgo/json"
)

func outputManifestPath(projectDir, configPath string) string {
	canonical, err := filepath.Abs(configPath)
	if err != nil {
		canonical = filepath.Clean(configPath)
	}
	sum := sha256.Sum256([]byte(filepath.ToSlash(canonical)))
	return filepath.Join(projectDir, ".rotor", "cache", "output-manifests", hex.EncodeToString(sum[:])+".json")
}

func incrementalSaltWithFlamework(program *compiler.Program, opts ProjectOptions, pathTranslatorBuildInfoPath string, flameworkInputs *FlameworkIncrementalInputs) (string, error) {
	options := program.Options()
	flameworkSalt, err := flameworkIncrementalSalt(flameworkInputs)
	if err != nil {
		return "", err
	}
	payload, err := tsjson.Marshal(struct {
		Version              string                `json:"version"`
		CompilerOptions      *core.CompilerOptions `json:"compilerOptions"`
		ConfigFilePath       string                `json:"configFilePath"`
		OutDir               string                `json:"outDir"`
		TsBuildInfoFile      string                `json:"tsBuildInfoFile"`
		PathTranslatorTarget string                `json:"pathTranslatorBuildInfoPath"`
		Type                 string                `json:"type"`
		RojoConfigPath       string                `json:"rojoConfigPath"`
		IncludePath          string                `json:"includePath"`
		LuaExtension         bool                  `json:"luaExtension"`
		Declaration          bool                  `json:"declaration"`
		EmitDeclarationOnly  bool                  `json:"emitDeclarationOnly"`
		NoOptimizedLoops     bool                  `json:"noOptimizedLoops"`
		MinifyOutput         bool                  `json:"minifyOutput"`
		FlameworkSalt        string                `json:"flameworkSalt,omitempty"`
	}{
		Version:              "rotor-incremental-v2",
		CompilerOptions:      options,
		ConfigFilePath:       options.ConfigFilePath,
		OutDir:               options.OutDir,
		TsBuildInfoFile:      options.TsBuildInfoFile,
		PathTranslatorTarget: pathTranslatorBuildInfoPath,
		Type:                 string(opts.Type),
		RojoConfigPath:       opts.RojoConfigPath,
		IncludePath:          opts.IncludePath,
		LuaExtension:         !opts.LuaExtension,
		Declaration:          options.Declaration.IsTrue(),
		EmitDeclarationOnly:  opts.EmitDeclarationOnly,
		NoOptimizedLoops:     opts.NoOptimizedLoops,
		MinifyOutput:         opts.MinifyOutput,
		FlameworkSalt:        flameworkSalt,
	})
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(payload)
	return hex.EncodeToString(sum[:]), nil
}
