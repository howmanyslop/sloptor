package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"path/filepath"
	"slices"
	"strings"

	"rotor/internal/rojo"
	"rotor/tsgo/collections"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	tsjson "rotor/tsgo/json"
	"rotor/tsgo/tspath"
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
	encodePath := incrementalPathEncoder(dir, program)
	// Authored paths can have different resolution semantics through a link.
	// Only compiler-resolved input filenames need package-store aliases.
	relativePath := func(name string) string { return relativeIncrementalPath(dir, name) }
	options := portableIncrementalOptions(program.Options(), program.GetCurrentDirectory(), relativePath)
	// State-file directories cannot affect emitted code. Anchor their names
	// consistently whether the cache is inside the project or on another drive.
	options.TsBuildInfoFile = incrementalBuildInfoIdentity(program.Options().TsBuildInfoFile)
	buildInfoTarget := incrementalBuildInfoIdentity(pathTranslatorBuildInfoPath)
	projectReferencePaths := opts.projectReferenceSaltPaths
	if projectReferencePaths == nil {
		projectReferencePaths = createProjectReferencePathMap(program, opts, false)
	}
	portableReferences := make(map[string]string, len(projectReferencePaths))
	for source, output := range projectReferencePaths {
		portableReferences[encodePath(source)] = relativePath(output)
	}
	rojoConfigPath, _, err := resolveRojoConfigPath(dir, opts.RojoConfigPath)
	if err != nil {
		return "", err
	}
	includePath, err := resolveIncludePath(dir, opts.IncludePath)
	if err != nil {
		return "", err
	}
	flameworkSalt, err := flameworkIncrementalSalt(portableFlameworkIncrementalInputs(flameworkInputs, encodePath))
	if err != nil {
		return "", err
	}
	// Import paths come from the Rojo resolver, which reads project files the
	// manifest does not hash, such as a dependency's default.project.json.
	rojoResolver, err := rojoResolverFingerprint(opts.rojoCache, rojoConfigPath, relativePath)
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
	plugins, err := transformerPluginFingerprints(dir, program.Options().ConfigFilePath)
	if err != nil {
		return "", err
	}
	// nativeTypes is omitzero, not omitempty: tsgo's json shim ignores
	// omitempty, and dropping the field entirely while the option is off keeps
	// the payload (and so every existing manifest) byte-identical for a
	// project that never opts in.
	payload, err := tsjson.Marshal(struct {
		MacroInputs           map[string]string              `json:"macroInputs,omitempty"`
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
		NativeTypes           bool                           `json:"nativeTypes,omitzero"`
		NoOptimizedLoops      bool                           `json:"noOptimizedLoops"`
		FlameworkSalt         string                         `json:"flameworkSalt,omitempty"`
		TransformerPlugins    []transformerPluginFingerprint `json:"transformerPlugins,omitempty"`
		ProjectReferencePaths map[string]string              `json:"projectReferencePaths,omitempty"`
		RojoResolver          string                         `json:"rojoResolver,omitempty"`
		PackageLayout         string                         `json:"packageLayout,omitempty"`
	}{
		MacroInputs:           macroInputValues(dir, program),
		Version:               "rotor-incremental-v5",
		CompilerOptions:       options,
		ConfigFilePath:        options.ConfigFilePath,
		OutDir:                options.OutDir,
		TsBuildInfoFile:       options.TsBuildInfoFile,
		PathTranslatorTarget:  buildInfoTarget,
		Type:                  string(opts.Type),
		RojoConfigPath:        relativePath(rojoConfigPath),
		IncludePath:           relativePath(includePath),
		LuaExtension:          !opts.LuaExtension,
		Declaration:           options.Declaration.IsTrue(),
		EmitDeclarationOnly:   opts.EmitDeclarationOnly,
		OptimizedArrayAppends: opts.OptimizedArrayAppends,
		NativeTypes:           effectiveNativeTypes(dir, options.ConfigFilePath, opts.NativeTypes),
		NoOptimizedLoops:      opts.NoOptimizedLoops,
		FlameworkSalt:         flameworkSalt,
		TransformerPlugins:    plugins,
		ProjectReferencePaths: portableReferences,
		RojoResolver:          rojoResolver,
		PackageLayout:         packageLayout,
	}, tsjson.Deterministic(true))
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(payload)
	return hex.EncodeToString(sum[:]), nil
}

func incrementalBuildInfoIdentity(name string) string {
	if name == "" {
		return ""
	}
	return "buildinfo://" + filepath.Base(filepath.FromSlash(name))
}

// rojoResolverFingerprint hashes the resolver state that maps files to Roblox
// paths, so a build whose Rojo paths changed rebuilds every file. It leaves out
// the walked configs and directories: those only validate the resolver cache,
// and the first build's own outDir joins them. It is empty without a Rojo
// project.
func rojoResolverFingerprint(cache *rojo.RojoResolverCache, rojoConfigPath string, encodePath func(string) string) (string, error) {
	if cache == nil || rojoConfigPath == "" {
		return "", nil
	}
	state := portableIncrementalRojoState(cache.Load(rojoConfigPath).GetState(), encodePath)
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

func portableIncrementalRojoState(state rojo.ResolverState, encodePath func(string) string) rojo.ResolverState {
	for i := range state.Partitions {
		state.Partitions[i].FsPath = encodePath(state.Partitions[i].FsPath)
	}
	for i := range state.FilePaths {
		state.FilePaths[i].Path = encodePath(state.FilePaths[i].Path)
	}
	// Native separators sort differently on Windows. Exact mappings have no
	// precedence, so sort them again after encoding; partition order matters.
	slices.SortFunc(state.FilePaths, func(a, b rojo.ResolverFilePathState) int {
		return strings.Compare(a.Path, b.Path)
	})
	return state
}

func portableIncrementalOptions(options *core.CompilerOptions, currentDirectory string, encodePath func(string) string) *core.CompilerOptions {
	result := options.Clone()
	// sourceRoot and mapRoot are authored URL metadata, not path-type
	// options. In particular, /sources must mean the same thing on every OS.
	for _, value := range []*string{
		&result.ConfigFilePath, &result.OutDir, &result.TsBuildInfoFile,
		&result.RootDir, &result.BaseUrl, &result.PathsBasePath,
		&result.DeclarationDir, &result.OutFile, &result.Project,
		&result.GenerateCpuProfile, &result.GenerateTrace, &result.PprofDir,
	} {
		*value = encodePath(*value)
	}
	result.RootDirs = slices.Clone(options.RootDirs)
	for i, name := range result.RootDirs {
		result.RootDirs[i] = encodePath(name)
	}
	result.TypeRoots = slices.Clone(options.TypeRoots)
	for i, name := range result.TypeRoots {
		result.TypeRoots[i] = encodePath(name)
	}
	if options.Paths != nil {
		result.Paths = &collections.OrderedMap[string, []string]{}
		base := options.GetPathsBasePath(currentDirectory)
		for key, values := range options.Paths.Entries() {
			values = slices.Clone(values)
			for i, name := range values {
				// Combine without cleaning: substitution happens before
				// resolving '..' in a wildcard target.
				values[i] = encodePath(tspath.CombinePaths(base, name))
			}
			result.Paths.Set(key, values)
		}
	}
	return result
}
