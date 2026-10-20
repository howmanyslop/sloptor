package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"path/filepath"

	"rotor/internal/rojo"
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

func incrementalSaltWithFlamework(dir string, program *compiler.Program, opts ProjectOptions, pathTranslatorBuildInfoPath string, flameworkInputs *FlameworkIncrementalInputs) (string, error) {
	options := program.Options()
	projectReferencePaths := opts.projectReferencePaths
	if projectReferencePaths == nil {
		projectReferencePaths = createCrossProjectImportPathMap(program, opts)
	}
	rojoConfigPath, _, err := resolveRojoConfigPath(dir, opts.RojoConfigPath)
	if err != nil {
		return "", err
	}
	includePath, err := resolveIncludePath(dir, opts.IncludePath)
	if err != nil {
		return "", err
	}
	flameworkSalt, err := flameworkIncrementalSalt(flameworkInputs)
	if err != nil {
		return "", err
	}
	// Import paths come from the Rojo resolver, which reads project files the
	// manifest does not hash, such as a dependency's default.project.json.
	rojoResolver, err := rojoResolverFingerprint(opts.rojoCache, rojoConfigPath)
	if err != nil {
		return "", err
	}
	packageLayout, err := opts.packageLayout.fingerprint(dir)
	if err != nil {
		return "", err
	}
	// External transformers rewrite the output of every file they see, and
	// nothing else in this payload notices them: tsgo drops
	// `compilerOptions.plugins` while parsing options, and the plugin code
	// lives outside the project. Upgrading a transformer therefore has to
	// invalidate the whole manifest.
	plugins, err := transformerPluginFingerprints(dir, options.ConfigFilePath)
	if err != nil {
		return "", err
	}
	payload, err := tsjson.Marshal(struct {
		Version               string                         `json:"version"`
		CompilerOptions       *core.CompilerOptions          `json:"compilerOptions"`
		ConfigFilePath        string                         `json:"configFilePath"`
		OutDir                string                         `json:"outDir"`
		TsBuildInfoFile       string                         `json:"tsBuildInfoFile"`
		PathTranslatorTarget  string                         `json:"pathTranslatorBuildInfoPath"`
		Type                  string                         `json:"type"`
		RojoConfigPath        string                         `json:"rojoConfigPath"`
		IncludePath           string                         `json:"includePath"`
		LuaExtension          bool                           `json:"luaExtension"`
		Declaration           bool                           `json:"declaration"`
		EmitDeclarationOnly   bool                           `json:"emitDeclarationOnly"`
		OptimizedArrayAppends bool                           `json:"optimizedArrayAppends"`
		NoOptimizedLoops      bool                           `json:"noOptimizedLoops"`
		MinifyOutput          bool                           `json:"minifyOutput"`
		FlameworkSalt         string                         `json:"flameworkSalt,omitempty"`
		TransformerPlugins    []transformerPluginFingerprint `json:"transformerPlugins,omitempty"`
		ProjectReferencePaths map[string]string              `json:"projectReferencePaths,omitempty"`
		RojoResolver          string                         `json:"rojoResolver,omitempty"`
		PackageLayout         string                         `json:"packageLayout,omitempty"`
	}{
		Version:               "rotor-incremental-v2",
		CompilerOptions:       options,
		ConfigFilePath:        options.ConfigFilePath,
		OutDir:                options.OutDir,
		TsBuildInfoFile:       options.TsBuildInfoFile,
		PathTranslatorTarget:  pathTranslatorBuildInfoPath,
		Type:                  string(opts.Type),
		RojoConfigPath:        rojoConfigPath,
		IncludePath:           includePath,
		LuaExtension:          !opts.LuaExtension,
		Declaration:           options.Declaration.IsTrue(),
		EmitDeclarationOnly:   opts.EmitDeclarationOnly,
		OptimizedArrayAppends: opts.OptimizedArrayAppends,
		NoOptimizedLoops:      opts.NoOptimizedLoops,
		MinifyOutput:          opts.MinifyOutput,
		FlameworkSalt:         flameworkSalt,
		TransformerPlugins:    plugins,
		ProjectReferencePaths: projectReferencePaths,
		RojoResolver:          rojoResolver,
		PackageLayout:         packageLayout,
	}, tsjson.Deterministic(true))
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(payload)
	return hex.EncodeToString(sum[:]), nil
}

// rojoResolverFingerprint hashes the resolver state that maps files to Roblox
// paths, so a build whose Rojo paths changed rebuilds every file. It leaves out
// the walked configs and directories: those only validate the resolver cache,
// and the first build's own outDir joins them. It is empty without a Rojo
// project.
func rojoResolverFingerprint(cache *rojo.RojoResolverCache, rojoConfigPath string) (string, error) {
	if cache == nil || rojoConfigPath == "" {
		return "", nil
	}
	state := cache.Load(rojoConfigPath).GetState()
	payload, err := tsjson.Marshal(struct {
		RbxPath            rojo.RbxPath                 `json:"rbxPath"`
		Partitions         []rojo.PartitionInfo         `json:"partitions"`
		FilePaths          []rojo.ResolverFilePathState `json:"filePaths"`
		IsolatedContainers []rojo.RbxPath               `json:"isolatedContainers"`
		IsGame             bool                         `json:"isGame"`
	}{
		RbxPath:            state.RbxPath,
		Partitions:         state.Partitions,
		FilePaths:          state.FilePaths,
		IsolatedContainers: state.IsolatedContainers,
		IsGame:             state.IsGame,
	}, tsjson.Deterministic(true))
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(payload)
	return hex.EncodeToString(sum[:]), nil
}
