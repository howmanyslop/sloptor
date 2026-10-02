package compile

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"

	"rotor/tsgo/vfs/osvfs"
)

type SolutionProject struct {
	ConfigPath  string
	References  []string
	Options     ProjectOptions
	Coordinator bool
}

type SolutionGraph struct {
	Projects                []SolutionProject
	coordinatorConfigChains []solutionConfigChain
}

type solutionConfigChain struct {
	projectPath string
	configPaths []string
	references  []string
}

type SolutionProjectDrainer interface {
	Drain(project SolutionProject) (*BuildResult, []string, error)
}

type SolutionProjectState struct {
	Project        SolutionProject
	Result         *BuildResult
	UpToDate       bool
	BlockedBy      string
	Err            error
	forceFullBuild bool
}

type SolutionProjectStatus string

const (
	SolutionProjectSuccess   SolutionProjectStatus = "success"
	SolutionProjectNoChange  SolutionProjectStatus = "no-change"
	SolutionProjectSatisfied SolutionProjectStatus = "satisfied"
	SolutionProjectBlocked   SolutionProjectStatus = "blocked"
	SolutionProjectFailed    SolutionProjectStatus = "failed"
)

// SolutionProjectSelection is the external scheduler's ownership decision for
// one drain. A nil selection keeps the legacy behavior of building the entire
// discovered graph.
type SolutionProjectSelection struct {
	Selected  []string
	Satisfied []string
}

// SolutionProjectResult owns the terminal outcome for one emitted config.
// Diagnostics never flow into blocked dependants; blockers identify the
// dependency outcomes that prevented work from running.
type SolutionProjectResult struct {
	ConfigPath  string
	Status      SolutionProjectStatus
	Blockers    []string
	Diagnostics []DiagnosticInfo
	Outputs     []string
	OutputCount int
	Timings     ProjectBuildTimings
}

// SolutionRoot supplies the effective entry options for one requested root.
// Most callers should use NewSolutionCoordinatorForRoots with shared options;
// API adapters use this form when each real root has its own rbxts options.
type SolutionRoot struct {
	ConfigPath string
	Options    ProjectOptions
}

type SolutionCoordinator struct {
	graph                *SolutionGraph
	drainer              SolutionProjectDrainer
	states               map[string]SolutionProjectState
	projectPaths         map[string]string
	coordinatorPaths     map[string]string
	writeRoots           map[string][]string
	waitOnlyDependencies map[string][]string
	builders             int
	timings              *BuildTimings
}

type solutionProjectVisit uint8

const (
	solutionProjectUnvisited solutionProjectVisit = iota
	solutionProjectVisiting
	solutionProjectVisited
)

func BuildSolutionGraph(tsConfigPath string, entry ProjectOptions) (*SolutionGraph, error) {
	return BuildSolutionGraphForRoots([]string{tsConfigPath}, entry)
}

// BuildSolutionGraphForRoots discovers the dependency-first union of roots.
// Each physical config has one canonical identity even when roots or
// references reach it through lexical, symlink, or case aliases.
func BuildSolutionGraphForRoots(tsConfigPaths []string, entry ProjectOptions) (*SolutionGraph, error) {
	roots := make([]solutionGraphRoot, len(tsConfigPaths))
	for index, configPath := range tsConfigPaths {
		roots[index] = solutionGraphRoot{configPath: configPath, options: entry}
	}
	return buildSolutionGraphForRoots(roots)
}

type solutionGraphRoot struct {
	configPath string
	options    ProjectOptions
}

func buildSolutionGraphForRoots(roots []solutionGraphRoot) (*SolutionGraph, error) {
	if len(roots) == 0 {
		return nil, errors.New("compile: solution requires at least one root")
	}

	projects := map[string]SolutionProject{}
	configPathsByProject := map[string][]string{}
	rootByProject := map[string]string{}
	order := []string{}
	ordered := map[string]struct{}{}

	for _, root := range roots {
		rootPath, _, err := canonicalSolutionConfigPath(root.configPath)
		if err != nil {
			return nil, fmt.Errorf("compile: resolve solution config %q: %w", root.configPath, err)
		}
		_, rootIsCoordinator, _, err := readProjectReferencePaths(rootPath)
		if err != nil {
			return nil, fmt.Errorf("compile: read project reference %q: %w", rootPath, err)
		}
		var coordinatorRbxts *RbxtsOptions
		if rootIsCoordinator && root.options.SolutionArgv != nil {
			coordinatorRbxts, err = ReadRbxtsOptions(rootPath)
			if err != nil {
				return nil, fmt.Errorf("compile: read solution options %q: %w", rootPath, err)
			}
		}

		visits := map[string]solutionProjectVisit{}
		stack := []string{}
		var visit func(string, bool) (string, error)
		visit = func(candidate string, isRoot bool) (string, error) {
			configPath, configKey, err := canonicalSolutionConfigPath(candidate)
			if err != nil {
				return "", fmt.Errorf("compile: resolve project reference %q: %w", candidate, err)
			}
			switch visits[configKey] {
			case solutionProjectVisiting:
				return "", solutionCycleError(stack, configPath)
			case solutionProjectVisited:
				return projects[configKey].ConfigPath, nil
			}

			visits[configKey] = solutionProjectVisiting
			stack = append(stack, configPath)
			defer func() { stack = stack[:len(stack)-1] }()

			references, coordinator, configPaths, err := readProjectReferencePaths(configPath)
			if err != nil {
				return "", fmt.Errorf("compile: read project reference %q: %w", configPath, err)
			}
			options := root.options
			if !isRoot {
				// Derive from the root entry, not the referencing project: a
				// project's salt must not depend on which intermediate reached it.
				options, err = referencedProjectOptions(root.options, configPath, rootIsCoordinator, coordinatorRbxts)
				if err != nil {
					return "", fmt.Errorf("compile: read referenced project options %q: %w", configPath, err)
				}
			}

			if existing, ok := projects[configKey]; ok {
				if !sameSolutionProjectOptions(existing.Options, options) {
					return "", fmt.Errorf("compile: conflicting effective options for %s reached from %s and %s", existing.ConfigPath, rootByProject[configKey], rootPath)
				}
				configPath = existing.ConfigPath
			} else {
				projects[configKey] = SolutionProject{ConfigPath: configPath, Options: options, Coordinator: coordinator}
				configPathsByProject[configKey] = configPaths
				rootByProject[configKey] = rootPath
			}

			canonicalReferences := make([]string, 0, len(references))
			for _, reference := range references {
				canonicalReference, err := visit(reference, false)
				if err != nil {
					return "", err
				}
				canonicalReferences = append(canonicalReferences, canonicalReference)
			}
			project := projects[configKey]
			project.References = canonicalReferences
			projects[configKey] = project
			visits[configKey] = solutionProjectVisited
			if _, ok := ordered[configKey]; !ok {
				ordered[configKey] = struct{}{}
				order = append(order, configKey)
			}
			return project.ConfigPath, nil
		}
		if _, err := visit(rootPath, true); err != nil {
			return nil, err
		}
	}

	projectsByPath := make(map[string]SolutionProject, len(projects))
	for _, project := range projects {
		projectsByPath[project.ConfigPath] = project
	}
	graph := &SolutionGraph{}
	for _, configKey := range order {
		project := projects[configKey]
		if project.Coordinator {
			graph.coordinatorConfigChains = append(graph.coordinatorConfigChains, solutionConfigChain{
				projectPath: project.ConfigPath,
				configPaths: configPathsByProject[configKey],
				references:  project.References,
			})
			continue
		}
		project.References = emittingProjectReferences(project.References, projectsByPath)
		graph.Projects = append(graph.Projects, project)
	}
	return graph, nil
}

func canonicalSolutionConfigPath(path string) (string, string, error) {
	absolute, err := filepath.Abs(filepath.FromSlash(path))
	if err != nil {
		return "", "", err
	}
	canonical := filepath.Clean(absolute)
	identity := filepath.Clean(filepath.FromSlash(osvfs.FS().Realpath(filepath.ToSlash(canonical))))
	if pathHasExplicitSymlink(canonical) {
		canonical = identity
	}
	key := identity
	if !osvfs.FS().UseCaseSensitiveFileNames() {
		key = strings.ToLower(key)
	}
	return canonical, key, nil
}

func pathHasExplicitSymlink(path string) bool {
	volume := filepath.VolumeName(path)
	current := volume + string(filepath.Separator)
	remainder := strings.TrimPrefix(path, current)
	for _, part := range strings.Split(remainder, string(filepath.Separator)) {
		if part == "" {
			continue
		}
		current = filepath.Join(current, part)
		info, err := os.Lstat(current)
		if err != nil {
			return false
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return true
		}
	}
	return false
}

func sameSolutionProjectOptions(left, right ProjectOptions) bool {
	left.Timings = nil
	right.Timings = nil
	left.Context = nil
	right.Context = nil
	left.TsConfigPath = ""
	right.TsConfigPath = ""
	left.SolutionArgv, right.SolutionArgv = nil, nil
	left.rojoCache, right.rojoCache = nil, nil
	left.crossProjectImportPathMap, right.crossProjectImportPathMap = nil, nil
	left.pendingSolutionPersists, right.pendingSolutionPersists = nil, nil
	left.pendingSolutionDependencyPersists, right.pendingSolutionDependencyPersists = nil, nil
	left.compileCache, right.compileCache = nil, nil
	left.solutionOverlays, right.solutionOverlays = nil, nil
	left.census, right.census = nil, nil
	left.deferRojoCachePersist, right.deferRojoCachePersist = false, false
	left.forceFullBuild, right.forceFullBuild = false, false
	return reflect.DeepEqual(left, right)
}

func emittingProjectReferences(references []string, projects map[string]SolutionProject) []string {
	result := make([]string, 0, len(references))
	seen := map[string]struct{}{}
	expandedCoordinators := map[string]struct{}{}
	var appendReference func(string)
	appendReference = func(configPath string) {
		project, ok := projects[configPath]
		if !ok || !project.Coordinator {
			if _, ok := seen[configPath]; ok {
				return
			}
			seen[configPath] = struct{}{}
			result = append(result, configPath)
			return
		}
		if _, ok := expandedCoordinators[configPath]; ok {
			return
		}
		expandedCoordinators[configPath] = struct{}{}
		for _, reference := range project.References {
			appendReference(reference)
		}
	}
	for _, reference := range references {
		appendReference(reference)
	}
	return result
}

func NewSolutionCoordinator(tsConfigPath string, entry ProjectOptions) (*SolutionCoordinator, error) {
	return NewSolutionCoordinatorForRoots([]string{tsConfigPath}, entry)
}

func NewSolutionCoordinatorWithDrainer(tsConfigPath string, entry ProjectOptions, drainer SolutionProjectDrainer) (*SolutionCoordinator, error) {
	return NewSolutionCoordinatorForRootsWithDrainer([]string{tsConfigPath}, entry, drainer)
}

func NewSolutionCoordinatorForRoots(tsConfigPaths []string, entry ProjectOptions) (*SolutionCoordinator, error) {
	roots := make([]SolutionRoot, len(tsConfigPaths))
	for index, configPath := range tsConfigPaths {
		roots[index] = SolutionRoot{ConfigPath: configPath, Options: entry}
	}
	return NewSolutionCoordinatorForRootOptions(roots)
}

func NewSolutionCoordinatorForRootOptions(roots []SolutionRoot) (*SolutionCoordinator, error) {
	graphRoots := make([]solutionGraphRoot, len(roots))
	for index, root := range roots {
		graphRoots[index] = solutionGraphRoot{configPath: root.ConfigPath, options: root.Options}
	}
	graph, err := buildSolutionGraphForRoots(graphRoots)
	if err != nil {
		return nil, err
	}
	importPathMap, metadata := populateCrossProjectMetadata(graph)
	drainer := &solutionBuildDrainer{
		importPathMap:        importPathMap,
		restoredDeclarations: metadata.restoredDeclarations,
		restoredMetadataErrs: metadata.restoredMetadataErrs,
	}
	return newSolutionCoordinator(graph, drainer, metadata, solutionRootsBuilders(roots), solutionRootsTimings(roots))
}

func NewSolutionCoordinatorForRootsWithDrainer(tsConfigPaths []string, entry ProjectOptions, drainer SolutionProjectDrainer) (*SolutionCoordinator, error) {
	if drainer == nil {
		return nil, errors.New("compile: solution project drainer is nil")
	}
	graph, err := BuildSolutionGraphForRoots(tsConfigPaths, entry)
	if err != nil {
		return nil, err
	}
	_, metadata := populateCrossProjectMetadata(graph)
	return newSolutionCoordinator(graph, drainer, metadata, effectiveSolutionBuilders(entry), entry.Timings)
}

func solutionRootsBuilders(roots []SolutionRoot) int {
	if len(roots) == 0 {
		return effectiveSolutionBuilders(ProjectOptions{})
	}
	return effectiveSolutionBuilders(roots[0].Options)
}

func solutionRootsTimings(roots []SolutionRoot) *BuildTimings {
	if len(roots) == 0 {
		return nil
	}
	return roots[0].Options.Timings
}

func newSolutionCoordinator(graph *SolutionGraph, drainer SolutionProjectDrainer, metadata solutionWriteMetadata, builders int, timings *BuildTimings) (*SolutionCoordinator, error) {
	states := make(map[string]SolutionProjectState, len(graph.Projects))
	projectPaths := make(map[string]string, len(graph.Projects))
	for _, project := range graph.Projects {
		states[project.ConfigPath] = SolutionProjectState{Project: project}
		_, key, err := canonicalSolutionConfigPath(project.ConfigPath)
		if err != nil {
			return nil, err
		}
		projectPaths[key] = project.ConfigPath
	}
	if timings != nil {
		timings.initProjects(graph.Projects)
		timings.setConcurrencyMetadata(builders, entryBuilders(graph), entryCheckers(graph))
	}
	return &SolutionCoordinator{
		graph:                graph,
		drainer:              drainer,
		states:               states,
		projectPaths:         projectPaths,
		coordinatorPaths:     solutionCoordinatorPaths(graph),
		writeRoots:           metadata.writeRoots,
		waitOnlyDependencies: metadata.waitOnlyDependencies,
		builders:             builders,
		timings:              timings,
	}, nil
}

func entryBuilders(graph *SolutionGraph) *int {
	if graph == nil || len(graph.Projects) == 0 {
		return nil
	}
	return graph.Projects[0].Options.Builders
}

func entryCheckers(graph *SolutionGraph) *int {
	if graph == nil || len(graph.Projects) == 0 {
		return nil
	}
	return graph.Projects[0].Options.Checkers
}

func (c *SolutionCoordinator) ProjectState(tsConfigPath string) (SolutionProjectState, bool) {
	_, key, err := canonicalSolutionConfigPath(tsConfigPath)
	if err != nil {
		return SolutionProjectState{}, false
	}
	configPath, ok := c.projectPaths[key]
	if !ok {
		return SolutionProjectState{}, false
	}
	state, ok := c.states[configPath]
	return state, ok
}

func (c *SolutionCoordinator) Invalidate(paths ...string) []string {
	references := make(map[string][]string, len(c.graph.Projects)+len(c.graph.coordinatorConfigChains))
	for _, project := range c.graph.Projects {
		references[project.ConfigPath] = project.References
	}
	for _, chain := range c.graph.coordinatorConfigChains {
		references[chain.projectPath] = chain.references
	}
	reverse := map[string][]string{}
	for configPath, projectReferences := range references {
		for _, reference := range projectReferences {
			reverse[reference] = append(reverse[reference], configPath)
		}
	}
	direct := map[string]struct{}{}
	queue := []string{}
	addDirect := func(configPath string) {
		if _, ok := direct[configPath]; ok {
			return
		}
		direct[configPath] = struct{}{}
		queue = append(queue, configPath)
	}
	for _, path := range paths {
		configPath, key, err := canonicalSolutionConfigPath(path)
		if err != nil {
			continue
		}
		if projectPath, ok := c.projectPaths[key]; ok {
			configPath = projectPath
		}
		if _, ok := c.states[configPath]; ok {
			addDirect(configPath)
			continue
		}
		coordinatorReferences, ok := references[configPath]
		if !ok {
			continue
		}
		descendants := append([]string(nil), coordinatorReferences...)
		descendantSeen := map[string]struct{}{}
		for len(descendants) > 0 {
			descendant := descendants[0]
			descendants = descendants[1:]
			if _, ok := descendantSeen[descendant]; ok {
				continue
			}
			descendantSeen[descendant] = struct{}{}
			if _, ok := c.states[descendant]; ok {
				addDirect(descendant)
			}
			descendants = append(descendants, references[descendant]...)
		}
	}
	seen := map[string]struct{}{}
	for len(queue) > 0 {
		path := queue[0]
		queue = queue[1:]
		if _, ok := seen[path]; ok {
			continue
		}
		seen[path] = struct{}{}
		queue = append(queue, reverse[path]...)
	}
	result := []string{}
	for _, project := range c.graph.Projects {
		if _, ok := seen[project.ConfigPath]; ok {
			state := c.states[project.ConfigPath]
			state.UpToDate = false
			state.BlockedBy = ""
			state.Err = nil
			// Downstream projects must rebuild fully: their inputs (the
			// referenced project's outputs) changed, which the per-project
			// manifest cannot track. The directly invalidated project's own
			// source changed, so its manifest stays trustworthy.
			if _, ok := direct[project.ConfigPath]; !ok {
				state.forceFullBuild = true
			}
			c.states[project.ConfigPath] = state
			result = append(result, project.ConfigPath)
		}
	}
	return result
}

func (c *SolutionCoordinator) Reload(tsConfigPath string, entry ProjectOptions) error {
	graph, err := BuildSolutionGraph(tsConfigPath, entry)
	if err != nil {
		return err
	}
	states := make(map[string]SolutionProjectState, len(graph.Projects))
	projectPaths := make(map[string]string, len(graph.Projects))
	for _, project := range graph.Projects {
		_, key, err := canonicalSolutionConfigPath(project.ConfigPath)
		if err != nil {
			return err
		}
		previousPath := c.projectPaths[key]
		state := c.states[previousPath]
		state.Project = project
		states[project.ConfigPath] = state
		projectPaths[key] = project.ConfigPath
	}
	c.graph = graph
	c.states = states
	c.projectPaths = projectPaths
	c.coordinatorPaths = solutionCoordinatorPaths(graph)
	importPathMap, metadata := populateCrossProjectMetadata(graph)
	c.writeRoots = metadata.writeRoots
	c.waitOnlyDependencies = metadata.waitOnlyDependencies
	c.builders = effectiveSolutionBuilders(entry)
	if _, ok := c.drainer.(*solutionBuildDrainer); ok {
		c.drainer = &solutionBuildDrainer{
			importPathMap:        importPathMap,
			restoredDeclarations: metadata.restoredDeclarations,
			restoredMetadataErrs: metadata.restoredMetadataErrs,
		}
	}
	return nil
}

func solutionCoordinatorPaths(graph *SolutionGraph) map[string]string {
	paths := make(map[string]string, len(graph.coordinatorConfigChains))
	for _, chain := range graph.coordinatorConfigChains {
		_, key, err := canonicalSolutionConfigPath(chain.projectPath)
		if err == nil {
			paths[key] = chain.projectPath
		}
	}
	return paths
}

func postOrderProjectPaths(rootPath string, projects map[string]SolutionProject) []string {
	paths := []string{}
	visited := map[string]struct{}{}
	var visit func(string)
	visit = func(configPath string) {
		if _, ok := visited[configPath]; ok {
			return
		}
		visited[configPath] = struct{}{}
		project := projects[configPath]
		for _, reference := range project.References {
			visit(reference)
		}
		paths = append(paths, configPath)
	}
	visit(rootPath)
	return paths
}

func solutionCycleError(stack []string, repeated string) error {
	start := 0
	for index, configPath := range stack {
		if configPath == repeated {
			start = index
			break
		}
	}
	cycle := append([]string{}, stack[start:]...)
	cycle = append(cycle, repeated)
	return fmt.Errorf("compile: circular project reference %s", cycle)
}
