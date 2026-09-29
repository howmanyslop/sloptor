package main

import (
	"context"
	"path/filepath"
	"time"

	"github.com/spf13/cobra"

	"rotor/internal/buildapi"
	"rotor/internal/compile"
	"rotor/internal/logservice"
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

func runAPIBuild(ctx context.Context, requestedRoots []string) (buildapi.BuildResult, error) {
	started := time.Now()
	timings := compile.NewBuildTimings()
	timings.SetProductVersion(version)
	roots := make([]compile.SolutionRoot, 0, len(requestedRoots))
	for _, requestedRoot := range requestedRoots {
		tsConfigPath, err := findTsConfigPath(requestedRoot)
		if err != nil {
			return buildapi.ResultFromCompile(requestedRoot, nil, nil, time.Since(started), err), nil
		}
		declared, err := readRbxtsOptionsChecked(tsConfigPath)
		if err != nil {
			return buildapi.ResultFromCompile(requestedRoot, nil, nil, time.Since(started), err), nil
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
		return buildapi.ResultFromCompile(filepath.Dir(roots[0].ConfigPath), nil, nil, time.Since(started), err), nil
	}
	_, projects, _, buildErr := coordinator.DrainWithProjectResults()
	if ctx.Err() != nil {
		return buildapi.BuildResult{}, ctx.Err()
	}
	timings.SetOK(buildErr == nil)
	return buildapi.ResultFromSolution(projects, timings, time.Since(started), buildErr), nil
}
