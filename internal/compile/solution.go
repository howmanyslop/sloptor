package compile

import (
	"errors"
	"fmt"
	"path/filepath"
)

type SolutionProject struct {
	ConfigPath  string
	References  []string
	Options     ProjectOptions
	Coordinator bool
}

type SolutionGraph struct {
	Projects []SolutionProject
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

type SolutionCoordinator struct {
	graph                *SolutionGraph
	drainer              SolutionProjectDrainer
	states               map[string]SolutionProjectState
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
	projects := map[string]SolutionProject{}
	visits := map[string]solutionProjectVisit{}
	stack := []string{}

	var visit func(string, ProjectOptions, bool) error
	visit = func(configPath string, options ProjectOptions, inheritEntryTypeAndRojo bool) error {
		configPath, err := filepath.Abs(configPath)
		if err != nil {
			return fmt.Errorf("compile: resolve project reference %q: %w", configPath, err)
		}
		configPath = filepath.Clean(configPath)
		switch visits[configPath] {
		case solutionProjectVisiting:
			return solutionCycleError(stack, configPath)
		case solutionProjectVisited:
			return nil
		}

		visits[configPath] = solutionProjectVisiting
		stack = append(stack, configPath)
		defer func() {
			stack = stack[:len(stack)-1]
		}()

		references, coordinator, err := readProjectReferencePaths(configPath)
		if err != nil {
			return fmt.Errorf("compile: read project reference %q: %w", configPath, err)
		}
		project := SolutionProject{
			ConfigPath:  configPath,
			References:  references,
			Options:     options,
			Coordinator: coordinator,
		}
		projects[configPath] = project
		if len(stack) == 1 {
			inheritEntryTypeAndRojo = coordinator
		}
		for _, reference := range references {
			referenceOptions, err := ProjectOptionsForReferencedConfig(options, reference, inheritEntryTypeAndRojo)
			if err != nil {
				return fmt.Errorf("compile: read referenced project options %q: %w", reference, err)
			}
			if err := visit(reference, referenceOptions, inheritEntryTypeAndRojo); err != nil {
				return err
			}
		}
		visits[configPath] = solutionProjectVisited
		if !coordinator {
			projects[configPath] = project
		}
		return nil
	}

	rootPath, err := filepath.Abs(tsConfigPath)
	if err != nil {
		return nil, fmt.Errorf("compile: resolve solution config %q: %w", tsConfigPath, err)
	}
	if err := visit(rootPath, entry, false); err != nil {
		return nil, err
	}

	graph := &SolutionGraph{}
	for _, configPath := range postOrderProjectPaths(rootPath, projects) {
		project := projects[configPath]
		if !project.Coordinator {
			graph.Projects = append(graph.Projects, project)
		}
	}
	return graph, nil
}

func NewSolutionCoordinator(tsConfigPath string, entry ProjectOptions) (*SolutionCoordinator, error) {
	graph, err := BuildSolutionGraph(tsConfigPath, entry)
	if err != nil {
		return nil, err
	}
	importPathMap, metadata := populateCrossProjectMetadata(graph)
	drainer := &solutionBuildDrainer{importPathMap: importPathMap}
	return newSolutionCoordinator(graph, drainer, metadata, effectiveSolutionBuilders(entry), entry.Timings)
}

func NewSolutionCoordinatorWithDrainer(tsConfigPath string, entry ProjectOptions, drainer SolutionProjectDrainer) (*SolutionCoordinator, error) {
	if drainer == nil {
		return nil, errors.New("compile: solution project drainer is nil")
	}
	graph, err := BuildSolutionGraph(tsConfigPath, entry)
	if err != nil {
		return nil, err
	}
	_, metadata := populateCrossProjectMetadata(graph)
	return newSolutionCoordinator(graph, drainer, metadata, effectiveSolutionBuilders(entry), entry.Timings)
}

func newSolutionCoordinator(graph *SolutionGraph, drainer SolutionProjectDrainer, metadata solutionWriteMetadata, builders int, timings *BuildTimings) (*SolutionCoordinator, error) {
	states := make(map[string]SolutionProjectState, len(graph.Projects))
	for _, project := range graph.Projects {
		states[project.ConfigPath] = SolutionProjectState{Project: project}
	}
	if timings != nil {
		timings.initProjects(graph.Projects)
		timings.setConcurrencyMetadata(builders, entryBuilders(graph), entryCheckers(graph))
	}
	return &SolutionCoordinator{
		graph:                graph,
		drainer:              drainer,
		states:               states,
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
	configPath, err := filepath.Abs(tsConfigPath)
	if err != nil {
		return SolutionProjectState{}, false
	}
	state, ok := c.states[filepath.Clean(configPath)]
	return state, ok
}

func (c *SolutionCoordinator) Invalidate(paths ...string) []string {
	reverse := map[string][]string{}
	for _, project := range c.graph.Projects {
		for _, reference := range project.References {
			reverse[reference] = append(reverse[reference], project.ConfigPath)
		}
	}
	direct := map[string]struct{}{}
	queue := []string{}
	for _, path := range paths {
		absolute, err := filepath.Abs(path)
		if err == nil {
			configPath := filepath.Clean(absolute)
			if _, ok := c.states[configPath]; ok {
				direct[configPath] = struct{}{}
				queue = append(queue, configPath)
			}
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
	for _, project := range graph.Projects {
		state := c.states[project.ConfigPath]
		state.Project = project
		states[project.ConfigPath] = state
	}
	c.graph = graph
	c.states = states
	importPathMap, metadata := populateCrossProjectMetadata(graph)
	c.waitOnlyDependencies = metadata.waitOnlyDependencies
	c.builders = effectiveSolutionBuilders(entry)
	if _, ok := c.drainer.(*solutionBuildDrainer); ok {
		c.drainer = &solutionBuildDrainer{importPathMap: importPathMap}
	}
	return nil
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
