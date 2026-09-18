package main

import (
	"context"
	"fmt"
	"time"

	"github.com/spf13/cobra"

	"rotor/internal/compile"
)

func newDaemonCommand(streams cliStreams) *cobra.Command {
	cmd := &cobra.Command{
		Use:                   "daemon",
		Short:                 "inspect or stop persistent transformer workers",
		Args:                  cobra.NoArgs,
		DisableFlagsInUseLine: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			return usageFailure("a daemon subcommand is required")
		},
	}
	cmd.AddCommand(newDaemonStatusCommand(streams), newDaemonStopCommand(streams))
	return cmd
}

func newDaemonStatusCommand(streams cliStreams) *cobra.Command {
	return &cobra.Command{
		Use:                   "status",
		Short:                 "show persistent transformer workers",
		Args:                  cobra.NoArgs,
		DisableFlagsInUseLine: true,
		RunE: func(cmd *cobra.Command, _ []string) error {
			ctx, cancel := contextWithDaemonTimeout(cmd)
			defer cancel()
			infos, err := compile.SidecarDaemonStatus(ctx)
			if err != nil {
				return runtimeFailure(err)
			}
			if len(infos) == 0 {
				fmt.Fprintln(streams.out, "no sidecar daemons running")
				return nil
			}
			for _, info := range infos {
				fmt.Fprintf(streams.out, "sidecar daemon %s: running (pid %d, %d workers)\n", info.ID, info.PID, info.WorkerCount)
			}
			return nil
		},
	}
}

func newDaemonStopCommand(streams cliStreams) *cobra.Command {
	return &cobra.Command{
		Use:                   "stop",
		Short:                 "stop all persistent transformer workers",
		Args:                  cobra.NoArgs,
		DisableFlagsInUseLine: true,
		RunE: func(cmd *cobra.Command, _ []string) error {
			ctx, cancel := contextWithDaemonTimeout(cmd)
			defer cancel()
			stopped, err := compile.StopSidecarDaemons(ctx)
			if err != nil {
				return runtimeFailure(err)
			}
			switch stopped {
			case 0:
				fmt.Fprintln(streams.out, "no sidecar daemons running")
			case 1:
				fmt.Fprintln(streams.out, "stopped 1 sidecar daemon")
			default:
				fmt.Fprintf(streams.out, "stopped %d sidecar daemons\n", stopped)
			}
			return nil
		},
	}
}

func newInternalSidecarDaemonCommand() *cobra.Command {
	var runtimeDir string
	var id string
	cmd := &cobra.Command{
		Use:                   "__sidecar-daemon",
		Hidden:                true,
		Args:                  cobra.NoArgs,
		DisableFlagsInUseLine: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			if runtimeDir == "" || id == "" {
				return usageFailure("internal sidecar daemon requires --runtime-dir and --id")
			}
			if err := compile.RunSidecarDaemon(runtimeDir, id); err != nil {
				return runtimeFailure(err)
			}
			return nil
		},
	}
	cmd.Flags().StringVar(&runtimeDir, "runtime-dir", "", "internal daemon runtime directory")
	cmd.Flags().StringVar(&id, "id", "", "internal daemon identifier")
	return cmd
}

func contextWithDaemonTimeout(cmd *cobra.Command) (context.Context, context.CancelFunc) {
	return context.WithTimeout(cmd.Context(), 5*time.Second)
}
