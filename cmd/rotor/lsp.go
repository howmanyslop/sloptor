package main

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"sync"

	"github.com/spf13/cobra"

	"rotor/internal/compile"
	"rotor/internal/logservice"
	"rotor/internal/lsp"
)

// newLSPCommand is `sloptor lsp`: a stdio language server that publishes the
// transformer diagnostics `sloptor diagnostics` reports, over the editor's
// unsaved buffers.
func newLSPCommand(streams cliStreams) *cobra.Command {
	// Clients pass --stdio by convention. It is the only transport, so the flag
	// is accepted and ignored.
	var stdio bool
	cmd := &cobra.Command{
		Use:   "lsp",
		Short: "run a language server that reports sloptor diagnostics to editors",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			// stdout carries the protocol, so everything else goes to stderr.
			previousLog := logservice.Output
			logservice.Output = streams.err
			defer func() { logservice.Output = previousLog }()

			ctx := cmd.Context()
			if ctx == nil {
				ctx = context.Background()
			}
			// admitBuild saves and restores process-wide runtime settings (GC
			// percent, memory limit, GOMAXPROCS) around each compile. Two
			// projects' censuses overlapping in this one long-lived process
			// would interleave those saves and restores and leave the one-shot
			// policy applied for good, so censuses run one at a time. Each one
			// is already parallel inside.
			var censusMu sync.Mutex
			err := lsp.Serve(ctx, streams.in, streams.out, lsp.Options{
				FindTsConfig: func(fileName string) (string, error) {
					tsConfigPath, err := findTsConfigPath(filepath.Dir(fileName))
					if err != nil {
						return "", err
					}
					if !hasRbxtsTypes(filepath.Dir(tsConfigPath)) {
						return "", errNotRobloxTS
					}
					return tsConfigPath, nil
				},
				Census: func(ctx context.Context, tsConfigPath string, overlays map[string]string) (*compile.ProjectDiagnostics, error) {
					censusMu.Lock()
					defer censusMu.Unlock()
					// Each census is a one-shot compile, admitted like `sloptor build`.
					defer admitBuild(ctx, streams)()
					opts, err := diagnosticsCompileOptions(tsConfigPath)
					if err != nil {
						return nil, err
					}
					opts.Overlays = overlays
					// An editor's open buffers include files this project does not
					// hold; those are not a failed run.
					opts.IgnoreUnmatchedOverlays = true
					return compile.CompileProjectDiagnostics(filepath.Dir(tsConfigPath), opts)
				},
			})
			if err != nil {
				return runtimeFailure(err)
			}
			return nil
		},
	}
	cmd.Flags().BoolVar(&stdio, "stdio", false, "use stdio for transport (the only transport; accepted for editor compatibility)")
	return cmd
}

var errNotRobloxTS = errors.New("not a roblox-ts project")

// hasRbxtsTypes reports whether node_modules/@rbxts exists in dir or any
// ancestor. A roblox-ts project always installs @rbxts/types, while a plain
// TypeScript project (an Astro site, say) has none, and a census of it only
// fails with "rootDir must be defined". The editor opens those files too, so the
// server uses this to leave them alone. Ancestors count because a monorepo can
// hoist node_modules above the project.
func hasRbxtsTypes(dir string) bool {
	for {
		if info, err := os.Stat(filepath.Join(dir, "node_modules", "@rbxts")); err == nil && info.IsDir() {
			return true
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return false
		}
		dir = parent
	}
}
