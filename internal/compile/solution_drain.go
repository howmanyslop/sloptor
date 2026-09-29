package compile

import (
	"fmt"
	"path/filepath"
	"runtime/trace"
	"sort"
)

type solutionBuildDrainer struct {
	importPathMap map[string]string
	persists      []func() error
}

func (c *SolutionCoordinator) Drain() (*BuildResult, []string, error) {
	result, _, messages, err := c.DrainWithProjectResults()
	return result, messages, err
}

// DrainWithProjectResults drains one union and returns a terminal, owned
// outcome for every emitted project in deterministic graph order.
func (c *SolutionCoordinator) DrainWithProjectResults() (*BuildResult, []SolutionProjectResult, []string, error) {
	if c.timings != nil {
		defer c.timings.finish()
	}
	result := &BuildResult{Outputs: map[string]string{}}
	indexByConfigPath := make(map[string]int, len(c.graph.Projects))
	for index, project := range c.graph.Projects {
		indexByConfigPath[project.ConfigPath] = index
	}
	tasks := make([]solutionTask, len(c.graph.Projects))
	for index, project := range c.graph.Projects {
		tasks[index] = solutionTask{index: index}
		for _, reference := range project.References {
			if predecessor, ok := indexByConfigPath[reference]; ok {
				tasks[index].predecessors = append(tasks[index].predecessors, predecessor)
			}
		}
		for _, dependency := range c.waitOnlyDependencies[project.ConfigPath] {
			if predecessor, ok := indexByConfigPath[dependency]; ok {
				tasks[index].waitOnly = append(tasks[index].waitOnly, predecessor)
			}
		}
	}

	type drainOutcome struct {
		skip     bool
		blockers []string
		result   *BuildResult
		messages []string
		err      error
		persists []func() error
	}
	outcomes := make([]drainOutcome, len(c.graph.Projects))
	cache := newSolutionCompileCache()
	if c.timings != nil {
		defer func() {
			c.timings.addParseCacheCounts(cache.hits.Load(), cache.misses.Load())
		}()
	}
	RunSolutionTasks(tasks, c.builders, func(index int) error {
		project := c.graph.Projects[index]
		state := c.states[project.ConfigPath]
		outcome := &outcomes[index]
		if state.UpToDate {
			outcome.skip = true
			if c.timings != nil {
				c.timings.setProjectStatus(project.ConfigPath, ProjectTimingStatusSkipped, "")
			}
			return nil
		}
		for _, predecessor := range tasks[index].predecessors {
			if outcomes[predecessor].err != nil {
				outcome.blockers = append(outcome.blockers, c.graph.Projects[predecessor].ConfigPath)
			}
		}
		if len(outcome.blockers) > 0 {
			outcome.err = fmt.Errorf("compile: project %s blocked by failed dependency %s", project.ConfigPath, outcome.blockers[0])
			if c.timings != nil {
				c.timings.setProjectStatus(project.ConfigPath, ProjectTimingStatusBlocked, outcome.blockers[0])
			}
			return outcome.err
		}

		if state.forceFullBuild {
			project.Options.forceFullBuild = true
		}
		var child *BuildTimings
		if c.timings != nil {
			child = c.timings.newProject(project.ConfigPath)
		}
		if child != nil {
			ctx, task := trace.NewTask(child.context(), "solution project")
			child.ctx = ctx
			defer task.End()
			project.Options.Timings = child
		}
		var persists []func() error
		var dependencyPersists []func() error
		project.Options.pendingSolutionPersists = &persists
		project.Options.pendingSolutionDependencyPersists = &dependencyPersists
		project.Options.deferRojoCachePersist = true
		project.Options.compileCache = cache
		built, messages, err := c.drainer.Drain(project)
		outcome.result = built
		outcome.messages = messages
		outcome.err = err
		if child != nil {
			if err != nil {
				child.setProjectStatus(project.ConfigPath, ProjectTimingStatusFailed, "")
			}
			if !child.finished {
				child.finish()
			}
			for i, persist := range persists {
				persists[i] = timedPersist(child, persist)
			}
			for i, persist := range dependencyPersists {
				dependencyPersists[i] = timedPersist(child, persist)
			}
		}
		outcome.persists = persists
		if err == nil {
			for _, persist := range dependencyPersists {
				if err := persist(); err != nil {
					outcome.err = fmt.Errorf("compile: publish dependency state for project %s: %w", project.ConfigPath, err)
					outcome.messages = append(outcome.messages, outcome.err.Error())
					break
				}
			}
		}
		if outcome.err != nil && (outcome.result == nil || len(outcome.result.Diagnostics) == 0) {
			diagnostics := stringDiagnostics(outcome.messages)
			if len(diagnostics) == 0 {
				diagnostics = []DiagnosticInfo{{Message: outcome.err.Error()}}
			}
			if outcome.result == nil {
				outcome.result = &BuildResult{Outputs: map[string]string{}}
			}
			outcome.result.Diagnostics = diagnostics
		}
		return outcome.err
	})

	var firstErr error
	projectResults := make([]SolutionProjectResult, 0, len(c.graph.Projects))
	for index, project := range c.graph.Projects {
		outcome := outcomes[index]
		if outcome.skip {
			projectResults = append(projectResults, SolutionProjectResult{
				ConfigPath: project.ConfigPath,
				Status:     SolutionProjectNoChange,
				Timings:    c.projectTiming(project.ConfigPath, ProjectTimingStatusSkipped),
			})
			continue
		}
		if appender, ok := c.drainer.(interface{ appendPersists([]func() error) }); ok {
			appender.appendPersists(outcome.persists)
		}
		state := c.states[project.ConfigPath]
		state.Result = outcome.result
		if outcome.result != nil {
			mergeSolutionBuildResult(result, project, outcome.result)
		}
		if outcome.err != nil {
			if len(outcome.blockers) > 0 {
				state.BlockedBy = outcome.blockers[0]
				state.Err = outcome.err
				result.Diagnostics = append(result.Diagnostics, DiagnosticInfo{Message: outcome.err.Error()})
			} else {
				state.Err = outcome.err
				if outcome.result == nil || len(outcome.result.Diagnostics) == 0 {
					for _, message := range outcome.messages {
						result.Diagnostics = append(result.Diagnostics, DiagnosticInfo{Message: message})
					}
				}
			}
			if firstErr == nil {
				firstErr = outcome.err
			}
		} else {
			state.BlockedBy = ""
			state.Err = nil
			state.UpToDate = true
			state.forceFullBuild = false
		}
		c.states[project.ConfigPath] = state

		projectResult := SolutionProjectResult{
			ConfigPath: project.ConfigPath,
			Blockers:   append([]string(nil), outcome.blockers...),
		}
		if outcome.result != nil {
			projectResult.Diagnostics = append([]DiagnosticInfo(nil), outcome.result.Diagnostics...)
			projectResult.Outputs = make([]string, 0, len(outcome.result.Outputs))
			for output := range outcome.result.Outputs {
				projectResult.Outputs = append(projectResult.Outputs, filepath.ToSlash(output))
			}
			sort.Strings(projectResult.Outputs)
			projectResult.OutputCount = len(projectResult.Outputs)
		}
		switch {
		case len(outcome.blockers) > 0:
			projectResult.Status = SolutionProjectBlocked
			projectResult.Diagnostics = nil
		case outcome.err != nil:
			projectResult.Status = SolutionProjectFailed
		default:
			projectResult.Status = SolutionProjectSuccess
		}
		projectResult.Timings = c.projectTiming(project.ConfigPath, string(projectResult.Status))
		if projectResult.Status == SolutionProjectSuccess &&
			projectResult.Timings.Counts.TotalSources > 0 &&
			projectResult.Timings.Counts.SelectedSources == 0 &&
			projectResult.Timings.Counts.EmittedEntries == 0 {
			projectResult.Status = SolutionProjectNoChange
			projectResult.Timings.Status = string(SolutionProjectNoChange)
		}
		projectResults = append(projectResults, projectResult)
	}
	if firstErr != nil {
		return result, projectResults, diagnosticInfoMessages(result.Diagnostics), firstErr
	}
	if drainer, ok := c.drainer.(interface{ persist() error }); ok {
		if err := drainer.persist(); err != nil {
			return result, projectResults, diagnosticInfoMessages(result.Diagnostics), err
		}
	}
	return result, projectResults, nil, nil
}

func (c *SolutionCoordinator) projectTiming(configPath, fallbackStatus string) ProjectBuildTimings {
	timing := ProjectBuildTimings{ConfigPath: configPath, Status: fallbackStatus}
	if c.timings == nil {
		return timing
	}
	c.timings.mu.Lock()
	defer c.timings.mu.Unlock()
	if index, ok := c.timings.projectIndex[configPath]; ok {
		return c.timings.Projects[index]
	}
	return timing
}

func EffectiveSolutionBuilders(entry ProjectOptions) int {
	return effectiveSolutionBuilders(entry)
}

func effectiveSolutionBuilders(entry ProjectOptions) int {
	if entry.SingleThreaded != nil && *entry.SingleThreaded {
		return 1
	}
	if entry.Builders == nil {
		return 4
	}
	return *entry.Builders
}

func BuildSolutionWithOptions(tsConfigPath string, entry ProjectOptions) (*BuildResult, []string, error) {
	coordinator, err := NewSolutionCoordinator(tsConfigPath, entry)
	if err != nil {
		return nil, nil, err
	}
	return coordinator.Drain()
}

func mergeSolutionBuildResult(result *BuildResult, project SolutionProject, built *BuildResult) {
	for path, text := range built.Outputs {
		result.Outputs[filepath.Join(filepath.Dir(project.ConfigPath), path)] = text
	}
	result.EmittedFiles = append(result.EmittedFiles, built.EmittedFiles...)
	result.Diagnostics = append(result.Diagnostics, built.Diagnostics...)
	result.WroteRotorTypes = result.WroteRotorTypes || built.WroteRotorTypes
	result.WroteLockfile = result.WroteLockfile || built.WroteLockfile
}

func (d *solutionBuildDrainer) Drain(project SolutionProject) (*BuildResult, []string, error) {
	options := project.Options
	options.TsConfigPath = project.ConfigPath
	options.crossProjectImportPathMap = d.importPathMap
	options.deferRojoCachePersist = true
	return BuildProjectWithOptions(filepath.Dir(project.ConfigPath), options)
}

func (d *solutionBuildDrainer) appendPersists(persists []func() error) {
	d.persists = append(d.persists, persists...)
}

func (d *solutionBuildDrainer) persist() error {
	for index, persist := range d.persists {
		if err := persist(); err != nil {
			d.persists = d.persists[index:]
			return fmt.Errorf("persist solution build state: %w", err)
		}
	}
	d.persists = nil
	return nil
}
