package compile

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"sort"
	"strings"

	"golang.org/x/text/unicode/norm"

	"rotor/internal/fsutil"
	"rotor/internal/rojo"
)

type SolutionWatchSet struct {
	ProjectPath         string
	TsConfigPaths       []string
	RootDirs            []string
	RojoConfigs         []string
	RojoDirectories     []string
	ArtifactDirectories []string
	PackageInputs       *PackageWatchInputs
	MacroFiles          []string
}

type WatchAssetEvent struct {
	Path    string
	Deleted bool
}

// WatchSets lists the paths to watch per project. Each project's set is
// cached until the project is invalidated or the solution reloads, and is
// derived from the parsed config, so no program is built.
func (c *SolutionCoordinator) WatchSets() []SolutionWatchSet {
	sets := make([]SolutionWatchSet, 0, len(c.graph.Projects)+len(c.graph.coordinatorConfigChains))
	rojoStates := map[string]rojoWatchState{}
	for _, project := range c.graph.Projects {
		set, ok := c.watchSets[project.ConfigPath]
		if !ok {
			set = c.projectWatchSet(project, rojoStates)
			c.watchSets[project.ConfigPath] = set
		}
		sets = append(sets, set)
	}
	for _, chain := range c.graph.coordinatorConfigChains {
		sets = append(sets, SolutionWatchSet{
			ProjectPath:   chain.projectPath,
			TsConfigPaths: canonicalWatchPaths(chain.configPaths),
		})
	}
	filterSolutionWatchArtifacts(sets, canonicalWatchDirectory)
	return sets
}

func filterSolutionWatchArtifacts(sets []SolutionWatchSet, canonicalize func(string) string) {
	// Resolve repeated inherited paths once per call. A later call must see
	// filesystem changes, including symlink retargets without invalidation.
	canonicalPaths := map[string]string{}
	canonical := func(path string) string {
		path = filepath.Clean(filepath.FromSlash(path))
		if resolved, ok := canonicalPaths[path]; ok {
			return resolved
		}
		resolved := canonicalize(path)
		canonicalPaths[path] = resolved
		return resolved
	}
	// An inherited Rojo config can walk another project's generated files.
	// Keep the cached sets project-local, then apply solution ownership to the
	// returned Rojo paths. Source roots remain explicit inputs, even when they
	// point at another project's declarations or generated sources.
	artifacts := []string{}
	seenArtifacts := map[string]struct{}{}
	for _, set := range sets {
		for _, path := range set.ArtifactDirectories {
			resolved := canonical(path)
			if _, ok := seenArtifacts[resolved]; !ok {
				seenArtifacts[resolved] = struct{}{}
				artifacts = append(artifacts, resolved)
			}
		}
	}
	for index := range sets {
		sets[index].RojoDirectories = slices.DeleteFunc(slices.Clone(sets[index].RojoDirectories), func(path string) bool {
			return pathWithinAnyRoot(canonical(path), artifacts)
		})
	}
}

// projectWatchSet reads the cached config unless the project is invalidated
// and not yet refreshed. rojoStates shares Rojo parses between projects.
func (c *SolutionCoordinator) projectWatchSet(project SolutionProject, rojoStates map[string]rojoWatchState) SolutionWatchSet {
	set := SolutionWatchSet{ProjectPath: project.ConfigPath}
	_, set.TsConfigPaths, _ = ReadRbxtsOptionsWithChain(project.ConfigPath)
	if len(set.TsConfigPaths) == 0 {
		set.TsConfigPaths = []string{project.ConfigPath}
	}
	set.TsConfigPaths = canonicalWatchPaths(set.TsConfigPaths)
	var config *projectConfig
	_, pending := c.pendingProjects[project.ConfigPath]
	if info := c.metadata.infos[project.ConfigPath]; info != nil && !pending {
		config = info.config
	} else {
		config, _, _ = parseProjectConfig(project.ConfigPath)
		c.metadata.configParses++
	}
	if config == nil {
		return set
	}
	dir := config.GetCurrentDirectory()
	set.RootDirs = getRootDirs(config)
	pathTranslator := createPathTranslator(config, !project.Options.LuaExtension)
	set.ArtifactDirectories = solutionWatchArtifactDirectories(
		dir,
		pathTranslator.OutDir,
		config.Options().DeclarationDir,
		project.Options.IncludePath,
	)
	rojoPath, _, rojoErr := resolveRojoConfigPath(dir, project.Options.RojoConfigPath)
	if rojoErr == nil && rojoPath != "" {
		state, ok := rojoStates[rojoPath]
		if !ok {
			state = newRojoWatchState(c.metadata.references.resolver(rojoPath))
			rojoStates[rojoPath] = state
		}
		set.RojoConfigs = state.configs
		set.RojoDirectories = filterRojoWatchDirectories(dir, state, set.ArtifactDirectories)
	}
	if layout, err := preparePackageLayout(dir, config.Options(), project.Options); err == nil {
		set.PackageInputs = layout.watchInputs(dir, config.Options().TypeRoots)
	}
	set.MacroFiles = MacroWatchFiles(project.ConfigPath)
	return set
}

func (c *SolutionCoordinator) DrainAssets(tsConfigPath string, events []WatchAssetEvent) error {
	state, ok := c.ProjectState(tsConfigPath)
	if !ok {
		return nil
	}
	config, _, err := parseProjectConfig(state.Project.ConfigPath)
	if err != nil {
		return err
	}
	translator := createPathTranslator(config, !state.Project.Options.LuaExtension)
	assetDirectories := append([]string{}, getRootDirs(config)...)
	canonicalSourceDirectories := canonicalWatchPaths(assetDirectories)
	artifacts := []string{}
	for _, set := range c.WatchSets() {
		artifacts = append(artifacts, canonicalWatchPaths(set.ArtifactDirectories)...)
		if set.ProjectPath == state.Project.ConfigPath {
			assetDirectories = append(assetDirectories, set.RojoDirectories...)
		}
	}
	changes := map[string]bool{}
	excludedArtifacts := slices.DeleteFunc(slices.Clone(artifacts), func(artifact string) bool {
		for _, source := range canonicalSourceDirectories {
			if pathWithinAnyRoot(source, []string{artifact}) {
				return true
			}
		}
		return false
	})
	for _, event := range events {
		canonicalPath := canonicalWatchDirectory(event.Path)
		if pathWithinAnyRoot(canonicalPath, excludedArtifacts) {
			continue
		}
		changes[rebaseWatchAssetPath(event.Path, assetDirectories)] = event.Deleted
	}
	paths := make([]string, 0, len(changes))
	for path := range changes {
		paths = append(paths, path)
	}
	sort.Strings(paths)
	for _, path := range paths {
		if !pathWithinAnyRoot(path, assetDirectories) || watchCompilablePath(path) {
			continue
		}
		deleted := changes[path]
		if !deleted {
			if _, err := os.Stat(path); os.IsNotExist(err) {
				deleted = true
			}
		}
		if deleted {
			tryRemoveOutput(translator, translator.GetOutputPath(path), config.Options().SourceMap.IsTrue())
			continue
		}
		if err := copyItem(translator, path); err != nil {
			return err
		}
	}
	return nil
}

func solutionWatchArtifactDirectories(projectDir, outputDir, declarationDir, includePath string) []string {
	directories := []string{filepath.Clean(filepath.FromSlash(outputDir))}
	if declarationDir != "" {
		directories = appendDistinctWatchDirectory(directories, declarationDir)
	}
	if includeDir, err := resolveIncludePath(projectDir, includePath); err == nil {
		directories = appendDistinctWatchDirectory(directories, includeDir)
	}
	return appendDistinctWatchDirectory(directories, filepath.Join(filepath.FromSlash(projectDir), ".rotor"))
}

func appendDistinctWatchDirectory(directories []string, directory string) []string {
	directory = filepath.Clean(filepath.FromSlash(directory))
	if slices.Contains(directories, directory) {
		return directories
	}
	return append(directories, directory)
}

func canonicalWatchDirectory(directory string) string {
	directory = filepath.Clean(filepath.FromSlash(directory))
	var canonical string
	if resolved, err := fsutil.EvalSymlinks(directory); err == nil {
		canonical = resolved
	} else {
		canonical = filepath.Join(canonicalWatchDirectoryParent(directory), filepath.Base(directory))
	}
	// fswatch's Darwin backend normalizes subscriptions and events to NFC.
	// Keep artifact ownership and asset rebasing in that same path domain.
	if runtime.GOOS == "darwin" {
		canonical = norm.NFC.String(canonical)
	}
	return canonical
}

func canonicalWatchPaths(paths []string) []string {
	canonicalPaths := make([]string, len(paths))
	for index, path := range paths {
		canonicalPaths[index] = canonicalWatchDirectory(path)
	}
	return canonicalPaths
}

func canonicalWatchDirectoryParent(directory string) string {
	parent := filepath.Dir(directory)
	if parent == directory {
		return directory
	}
	if resolved, err := fsutil.EvalSymlinks(parent); err == nil {
		return resolved
	}
	return canonicalWatchDirectory(parent)
}

func rebaseWatchAssetPath(path string, assetDirectories []string) string {
	path = filepath.Clean(filepath.FromSlash(path))
	canonicalPath := canonicalWatchDirectory(path)
	for _, directory := range assetDirectories {
		relative, err := filepath.Rel(canonicalWatchDirectory(directory), canonicalPath)
		if err != nil || relative == ".." || strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
			continue
		}
		return filepath.Join(filepath.Clean(filepath.FromSlash(directory)), relative)
	}
	return path
}

// rojoWatchState is a Rojo config's walked paths, canonicalized once and
// shared by every project that uses the config.
type rojoWatchState struct {
	configs              []string
	directories          []string
	canonicalDirectories []string
}

func newRojoWatchState(resolver *rojo.RojoResolver) rojoWatchState {
	state := resolver.GetState()
	return rojoWatchState{
		configs:              canonicalWatchPaths(state.WalkedConfigs),
		directories:          state.WalkedDirectories,
		canonicalDirectories: canonicalWatchPaths(state.WalkedDirectories),
	}
}

func filterRojoWatchDirectories(projectDir string, state rojoWatchState, artifactDirectories []string) []string {
	canonicalProjectDir := canonicalWatchDirectory(projectDir)
	filtered := make([]string, 0, len(state.directories))
	for index, directory := range state.directories {
		directory = rebaseRojoWatchDirectory(directory, state.canonicalDirectories[index], projectDir, canonicalProjectDir)
		if pathWithinAnyRoot(directory, artifactDirectories) {
			continue
		}
		filtered = append(filtered, directory)
	}
	return filtered
}

func rebaseRojoWatchDirectory(directory, canonicalDirectory, projectDir, canonicalProjectDir string) string {
	relative, err := filepath.Rel(canonicalProjectDir, canonicalDirectory)
	if err != nil {
		return filepath.Clean(filepath.FromSlash(directory))
	}
	return filepath.Join(filepath.Clean(filepath.FromSlash(projectDir)), relative)
}

func (c *SolutionCoordinator) ReloadForWatch(tsConfigPath string, entry ProjectOptions) (func() error, error) {
	graph, err := BuildSolutionGraph(tsConfigPath, entry)
	if err != nil {
		return nil, err
	}
	luaExtensions := make(map[string]bool, len(graph.Projects))
	for _, project := range graph.Projects {
		luaExtensions[project.ConfigPath] = project.Options.LuaExtension
	}
	// Only a changed Lua extension leaves stale outputs, so only those
	// projects need the program that lists their emitted sources. A project
	// gone from the new graph may have no config left, so it is skipped.
	previous := make(map[string]solutionWatchProjectOutputs, len(c.graph.Projects))
	for _, project := range c.graph.Projects {
		luaExtension, ok := luaExtensions[project.ConfigPath]
		if !ok {
			continue
		}
		if luaExtension == project.Options.LuaExtension {
			if _, _, err := parseProjectConfig(project.ConfigPath); err != nil {
				return nil, err
			}
			continue
		}
		outputs, err := solutionWatchOutputPaths(project)
		if err != nil {
			return nil, err
		}
		previous[project.ConfigPath] = solutionWatchProjectOutputs{
			luaExtension: project.Options.LuaExtension,
			paths:        outputs,
		}
	}
	if err := c.reload(graph, entry); err != nil {
		return nil, err
	}

	staleOutputs := []string{}
	for _, project := range c.graph.Projects {
		old, ok := previous[project.ConfigPath]
		if !ok || old.luaExtension == project.Options.LuaExtension {
			continue
		}
		staleOutputs = append(staleOutputs, old.paths...)
	}
	sort.Strings(staleOutputs)
	return func() error {
		for _, path := range staleOutputs {
			if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
				return fmt.Errorf("remove stale solution watch output %q: %w", path, err)
			}
			if err := os.Remove(path + ".map"); err != nil && !os.IsNotExist(err) {
				return fmt.Errorf("remove stale solution watch source map %q: %w", path+".map", err)
			}
		}
		return nil
	}, nil
}

type solutionWatchProjectOutputs struct {
	luaExtension bool
	paths        []string
}

func solutionWatchOutputPaths(project SolutionProject) ([]string, error) {
	_, program, _, err := newProjectProgram(filepath.Dir(project.ConfigPath), project.ConfigPath)
	if err != nil {
		return nil, err
	}
	translator := createPathTranslator(program, !project.Options.LuaExtension)
	paths := make([]string, 0, len(projectSourceFiles(program)))
	for _, sourceFile := range projectSourceFiles(program) {
		paths = append(paths, translator.GetOutputPath(sourceFile.FileName()))
	}
	return paths, nil
}

func watchCompilablePath(path string) bool {
	return strings.HasSuffix(path, ".d.ts") || strings.HasSuffix(path, ".ts") || strings.HasSuffix(path, ".tsx")
}
