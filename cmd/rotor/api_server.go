package main

import (
	"context"
	"path/filepath"
	"strings"
	"time"

	"github.com/spf13/cobra"

	"rotor/internal/buildapi"
	"rotor/internal/compile"
	"rotor/internal/logservice"
	"rotor/tsgo/vfs/osvfs"
)

func newAPIServerCommand(streams cliStreams) *cobra.Command {
	return &cobra.Command{
		Use:    "api-server",
		Hidden: true,
		Args:   cobra.NoArgs,
		RunE: func(_ *cobra.Command, _ []string) error {
			previousLog := logservice.Output
			logservice.Output = streams.err
			defer func() { logservice.Output = previousLog }()

			server := buildapi.NewServer(version, runAPIBuild)
			if err := server.Run(streams.in, streams.out); err != nil {
				return runtimeFailure(err)
			}
			return nil
		},
	}
}

func runAPIBuild(ctx context.Context, request buildapi.BuildRequest) (buildapi.BuildResult, error) {
	started := time.Now()
	timings := compile.NewBuildTimings()
	timings.SetProductVersion(version)
	requestedRoots := request.Roots
	roots := make([]compile.SolutionRoot, 0, len(requestedRoots))
	for _, requestedRoot := range requestedRoots {
		tsConfigPath, err := findTsConfigPath(requestedRoot)
		if err != nil {
			return apiBuildSetupFailure(request, started, err), nil
		}
		declared, err := readRbxtsOptionsChecked(tsConfigPath)
		if err != nil {
			return apiBuildSetupFailure(request, started, err), nil
		}
		opts := mergeProjectOptions(defaultProjectOptions, declared)
		opts.watch = false
		// An empty argv layer makes referenced projects derive from defaults and
		// their own rbxts options, exactly like `build --build` with no flags.
		opts.argv = &partialProjectOptions{}
		compileOptions := projectCompileOptions(tsConfigPath, opts)
		compileOptions.Timings = timings
		compileOptions.Context = ctx
		roots = append(roots, compile.SolutionRoot{ConfigPath: tsConfigPath, Options: compileOptions})
	}

	coordinator, err := compile.NewSolutionCoordinatorForRootOptions(roots)
	if err != nil {
		return apiBuildSetupFailure(request, started, err), nil
	}
	_, projects, _, buildErr := coordinator.DrainWithProjectResultsForSelection(request.Selection)
	if ctx.Err() != nil {
		return buildapi.BuildResult{}, ctx.Err()
	}
	timings.SetOK(buildErr == nil)
	return buildapi.ResultFromSolution(projects, timings, time.Since(started), buildErr), nil
}

// Setup errors occur before the coordinator can produce owned outcomes. Give
// every selected config a terminal failure so callers can match their results.
func apiBuildSetupFailure(request buildapi.BuildRequest, started time.Time, buildErr error) buildapi.BuildResult {
	selected := request.Roots
	if request.Selection != nil {
		selected = request.Selection.Selected
	}
	projects := make([]compile.SolutionProjectResult, 0, len(selected))
	seen := map[string]bool{}
	for _, selectedPath := range selected {
		configPath, err := findTsConfigPath(selectedPath)
		if err != nil {
			configPath, _ = filepath.Abs(filepath.FromSlash(selectedPath))
		}
		identity := osvfs.FS().Realpath(filepath.ToSlash(configPath))
		if !osvfs.FS().UseCaseSensitiveFileNames() {
			identity = strings.ToLower(identity)
		}
		if seen[identity] {
			continue
		}
		seen[identity] = true
		projects = append(projects, compile.SolutionProjectResult{
			ConfigPath:  configPath,
			Status:      compile.SolutionProjectFailed,
			Diagnostics: []compile.DiagnosticInfo{{Message: buildErr.Error()}},
		})
	}
	result := buildapi.ResultFromSolution(projects, nil, time.Since(started), buildErr)
	if len(result.Projects) > 0 {
		result.Diagnostics = result.Projects[0].Diagnostics
	}
	result.Telemetry.SelectedProjects = len(projects)
	return result
}
