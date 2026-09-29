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

func runAPIBuild(_ context.Context, project string) (buildapi.BuildResult, error) {
	started := time.Now()
	tsConfigPath, err := findTsConfigPath(project)
	if err != nil {
		return buildapi.ResultFromCompile(project, nil, nil, time.Since(started), err), nil
	}
	declared, err := readRbxtsOptionsChecked(tsConfigPath)
	if err != nil {
		return buildapi.ResultFromCompile(project, nil, nil, time.Since(started), err), nil
	}
	opts := mergeProjectOptions(defaultProjectOptions, declared, nil)
	opts.watch = false

	dir := filepath.Dir(tsConfigPath)
	timings := compile.NewBuildTimings()
	timings.SetProductVersion(version)
	compileOptions := projectCompileOptions(tsConfigPath, opts)
	compileOptions.Timings = timings

	result, messages, buildErr := compile.BuildProjectWithOptions(dir, compileOptions)
	diagnostics := buildDiagnostics(result, messages)
	return buildapi.ResultFromCompile(dir, result, diagnostics, time.Since(started), buildErr), nil
}
