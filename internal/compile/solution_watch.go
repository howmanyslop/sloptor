package compile

import (
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strings"

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
	return sets
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
	dir := filepath.Dir(state.Project.ConfigPath)
	if rojoPath, _, rojoErr := resolveRojoConfigPath(dir, state.Project.Options.RojoConfigPath); rojoErr == nil && rojoPath != "" {
		artifactDirectories := solutionWatchArtifactDirectories(
			dir,
			translator.OutDir,
			state.Project.Options.IncludePath,
		)
		assetDirectories = append(assetDirectories, filterRojoWatchDirectories(dir, newRojoWatchState(c.metadata.references.resolver(rojoPath)), artifactDirectories)...)
	}
	changes := map[string]bool{}
	for _, event := range events {
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

func solutionWatchArtifactDirectories(projectDir, outputDir, includePath string) []string {
	directories := []string{filepath.Clean(filepath.FromSlash(outputDir))}
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
	if resolved, err := fsutil.EvalSymlinks(directory); err == nil {
		return resolved
	}
	return filepath.Join(canonicalWatchDirectoryParent(directory), filepath.Base(directory))
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
	return canonicalWatchDirectoryParent(parent)
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
