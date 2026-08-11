package main

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/spf13/cobra"

	"rotor/internal/config"
	"rotor/internal/migrate"
)

func newMigrateCommand(streams cliStreams) *cobra.Command {
	cmd := &cobra.Command{
		Use:                   "migrate",
		Short:                 "migrate legacy project configuration",
		Args:                  cobra.NoArgs,
		DisableFlagsInUseLine: true,
		RunE: func(_ *cobra.Command, _ []string) error {
			return usageFailure("a migration subcommand is required")
		},
	}
	cmd.AddCommand(newMigrateConfigCommand(streams), newMigrateFlameworkCommand(streams))
	return cmd
}

func newMigrateConfigCommand(streams cliStreams) *cobra.Command {
	var force bool
	cmd := &cobra.Command{
		Use:                   "config [path] [--force]",
		Short:                 "convert a legacy rotor.config.ts to rotor.toml",
		Args:                  cobra.MaximumNArgs(1),
		DisableFlagsInUseLine: true,
		RunE: func(cmd *cobra.Command, argv []string) error {
			dir := "."
			if len(argv) > 0 {
				dir = argv[0]
			}
			return runMigrateCommand(streams, dir, force)
		},
	}
	cmd.Flags().SortFlags = false
	addBoolFlag(cmd, &force, "force", "f", false, "overwrite an existing rotor.toml")
	return cmd
}

func newMigrateFlameworkCommand(streams cliStreams) *cobra.Command {
	var removePackage bool
	cmd := &cobra.Command{
		Use:                   "flamework [tsconfig-file] [--remove-package]",
		Short:                 "migrate the Flamework transformer to native Rotor configuration",
		Args:                  cobra.MaximumNArgs(1),
		DisableFlagsInUseLine: true,
		RunE: func(cmd *cobra.Command, argv []string) error {
			tsconfigPath := "tsconfig.json"
			if len(argv) == 1 {
				tsconfigPath = argv[0]
			}
			return runMigrateFlamework(cmd, streams, tsconfigPath, removePackage)
		},
	}
	cmd.Flags().SortFlags = false
	addBoolFlag(cmd, &removePackage, "remove-package", "", false, "remove rbxts-transformer-flamework from the owning workspace")
	return cmd
}

func runMigrateFlamework(cmd *cobra.Command, streams cliStreams, tsconfigPath string, removePackage bool) error {
	tsconfigChanges, tsconfigErr := migrate.PlanFlameworkTSConfigTree(tsconfigPath)
	pluginAbsent := errors.Is(tsconfigErr, migrate.ErrNoFlameworkPlugin)
	if tsconfigErr != nil && !pluginAbsent {
		return runtimeFailure(tsconfigErr)
	}

	fileChanges := make([]migrate.FileChange, 0, len(tsconfigChanges)+1)
	cleanupTargets := []string{tsconfigPath}
	tomlPlans := make(map[string]flameworkTOMLPlan)
	for _, tsconfigChange := range tsconfigChanges {
		fileChanges = append(fileChanges, migrate.FileChange{
			Path: tsconfigChange.Path, Original: tsconfigChange.Original, Updated: tsconfigChange.Updated, Existed: true,
		})
		cleanupTargets = append(cleanupTargets, tsconfigChange.Path)
		tomlPath := filepath.Join(filepath.Dir(tsconfigChange.Path), config.ConfigFileName)
		if absolute, err := filepath.Abs(tomlPath); err == nil {
			tomlPath = absolute
		}
		if existing, ok := tomlPlans[tomlPath]; ok {
			if !sameFlameworkOptions(existing.Options, tsconfigChange.Options) {
				return runtimeFailure(fmt.Errorf(
					"migration conflict: %s requires [flamework] %s but %s requires %s; one rotor.toml cannot represent both",
					filepath.Base(existing.Path), describeFlameworkOptions(existing.Options), filepath.Base(tsconfigChange.Path), describeFlameworkOptions(tsconfigChange.Options),
				))
			}
			continue
		}
		tomlPlans[tomlPath] = flameworkTOMLPlan{Path: tsconfigChange.Path, Options: tsconfigChange.Options}
		tomlChange, tomlStatus, tomlErr := migrate.MergeFlameworkTOML(tomlPath, tsconfigChange.Options)
		alreadyMigrated := tomlStatus == migrate.MergeAlreadyMigrated
		if tomlErr != nil && !alreadyMigrated {
			return runtimeFailure(tomlErr)
		}
		if alreadyMigrated {
			if len(tsconfigChanges) == 1 {
				return runtimeFailure(errors.New("migration conflict: rotor.toml already has [flamework] while tsconfig still has rbxts-transformer-flamework"))
			}
			existing, exists, readErr := migrate.ExistingFlameworkOptions(tomlPath)
			if readErr != nil {
				return runtimeFailure(fmt.Errorf("read existing [flamework] configuration: %w", readErr))
			}
			if !exists || !sameFlameworkOptions(existing, tsconfigChange.Options) {
				return runtimeFailure(fmt.Errorf(
					"migration conflict: rotor.toml has [flamework] %s, but %s requires %s",
					describeFlameworkOptions(existing), filepath.Base(tsconfigChange.Path), describeFlameworkOptions(tsconfigChange.Options),
				))
			}
			continue
		}
		fileChanges = append(fileChanges, tomlChange)
	}

	alreadyMigrated := false
	if pluginAbsent {
		tomlPath := filepath.Join(filepath.Dir(tsconfigPath), config.ConfigFileName)
		if absolute, err := filepath.Abs(tomlPath); err == nil {
			tomlPath = absolute
		}
		_, tomlStatus, tomlErr := migrate.MergeFlameworkTOML(tomlPath, migrate.FlameworkOptions{})
		alreadyMigrated = tomlStatus == migrate.MergeAlreadyMigrated
		if tomlErr != nil && !alreadyMigrated {
			return runtimeFailure(tomlErr)
		}
		if !alreadyMigrated {
			return runtimeFailure(errors.New("nothing to migrate: no rbxts-transformer-flamework plugin or [flamework] table was found"))
		}
	}

	cleanups := make([]migrate.PackageCleanup, 0, len(cleanupTargets))
	cleanupCommands := make(map[string]struct{})
	for _, target := range cleanupTargets {
		cleanup, err := migrate.PlanPackageCleanup(target)
		if err != nil {
			return runtimeFailure(fmt.Errorf("preflight package cleanup: %w", err))
		}
		if _, exists := cleanupCommands[cleanup.DisplayCommand]; exists {
			continue
		}
		cleanupCommands[cleanup.DisplayCommand] = struct{}{}
		cleanups = append(cleanups, cleanup)
	}

	receipt := migrate.Receipt{}
	var err error
	if !alreadyMigrated {
		receipt, err = migrate.Commit(filepath.Dir(tsconfigPath), fileChanges)
		if err != nil {
			return runtimeFailure(err)
		}
	}

	if !removePackage {
		if len(tsconfigChanges) > 1 {
			fmt.Fprintf(streams.out, "Migrated %d tsconfig files.\n", len(tsconfigChanges))
		}
		fmt.Fprintln(streams.out, "Migration complete. Optional package cleanup:")
		for _, cleanup := range cleanups {
			fmt.Fprintf(streams.out, "  %s\n", cleanup.DisplayCommand)
		}
		for _, backup := range receipt.Backups {
			fmt.Fprintf(streams.out, "Backup: %s\n", backup)
		}
		return nil
	}
	for _, cleanup := range cleanups {
		if err := cleanup.Execute(cmd.Context()); err != nil {
			backupText := "none"
			if len(receipt.Backups) > 0 {
				backupText = strings.Join(receipt.Backups, ", ")
			}
			return runtimeFailure(fmt.Errorf("%w; backups: %s", err, backupText))
		}
	}
	fmt.Fprintln(streams.out, "Migration complete; removed rbxts-transformer-flamework from the owning workspace.")
	for _, backup := range receipt.Backups {
		fmt.Fprintf(streams.out, "Backup: %s\n", backup)
	}
	return nil
}

type flameworkTOMLPlan struct {
	Path    string
	Options migrate.FlameworkOptions
}

func sameFlameworkOptions(left, right migrate.FlameworkOptions) bool {
	leftLimit, rightLimit := left.Optimizations.GuardGenerationDedupLimit, right.Optimizations.GuardGenerationDedupLimit
	if left.IDGenerationMode == "" {
		left.IDGenerationMode = "full"
	}
	if right.IDGenerationMode == "" {
		right.IDGenerationMode = "full"
	}
	return left.After == right.After &&
		left.IDGenerationMode == right.IDGenerationMode &&
		left.HashPrefix == right.HashPrefix &&
		left.Salt == right.Salt &&
		left.NoSemanticDiagnostics == right.NoSemanticDiagnostics &&
		left.Obfuscation == right.Obfuscation &&
		left.PreloadIDs == right.PreloadIDs &&
		(leftLimit == nil && rightLimit == nil || leftLimit != nil && rightLimit != nil && *leftLimit == *rightLimit)
}

func describeFlameworkOptions(options migrate.FlameworkOptions) string {
	return fmt.Sprintf("after=%q, idGenerationMode=%q, hashPrefix=%q, salt=%q", options.After, options.IDGenerationMode, options.HashPrefix, options.Salt)
}

func runMigrateCommand(streams cliStreams, dir string, force bool) error {
	start := time.Now()
	u := newUI(streams.out)
	u.banner("migrate")

	// Find the legacy config so we can name it in messages and rename it later.
	legacyPath := ""
	for _, name := range []string{"rotor.config.ts", "rotor.config.js"} {
		candidate := filepath.Join(dir, name)
		if fileExists(candidate) {
			legacyPath = candidate
			break
		}
	}
	if legacyPath == "" {
		return runtimeFailure(fmt.Errorf(
			"no rotor.config.ts (or rotor.config.js) found in %s\n    migrate converts an existing TypeScript config to rotor.toml;\n    there is nothing to migrate here. Use `sloptor init` to start fresh.", dir,
		))
	}

	tomlPath := filepath.Join(dir, config.ConfigFileName)
	if fileExists(tomlPath) && !force {
		return runtimeFailure(fmt.Errorf(
			"%s already exists\n    refusing to overwrite it; re-run with --force to replace it.", config.ConfigFileName,
		))
	}

	cfg, err := config.LoadLegacyTS(dir)
	if err != nil {
		if errors.Is(err, config.ErrNotFound) {
			return runtimeFailure(fmt.Errorf("no legacy config found in %s", dir))
		}
		return runtimeFailure(fmt.Errorf("could not load %s: %w", filepath.Base(legacyPath), err))
	}
	for _, w := range cfg.Warnings {
		newUI(streams.err).warn(w)
	}

	body, err := config.MarshalTOML(cfg)
	if err != nil {
		return runtimeFailure(fmt.Errorf("could not serialize config to TOML: %w", err))
	}
	out := config.SchemaDirective + "\n\n" + body
	if err := os.WriteFile(tomlPath, []byte(out), 0o644); err != nil {
		return runtimeFailure(fmt.Errorf("writing %s: %w", config.ConfigFileName, err))
	}

	events := []uiEvent{{Status: eventWrote, Target: config.ConfigFileName}}

	// Rename the legacy files to .bak so they stop being picked up but are not
	// lost. A .bak that already exists is overwritten (idempotent re-runs).
	if err := backup(legacyPath); err != nil {
		newUI(streams.err).warn(fmt.Sprintf("could not rename %s: %v", filepath.Base(legacyPath), err))
	} else {
		events = append(events, uiEvent{
			Status: eventWrote,
			Target: filepath.Base(legacyPath) + ".bak",
			Detail: filepath.Base(legacyPath) + " → " + filepath.Base(legacyPath) + ".bak",
		})
	}
	dtsPath := filepath.Join(dir, "rotor-config.d.ts")
	if fileExists(dtsPath) {
		if err := backup(dtsPath); err != nil {
			newUI(streams.err).warn(fmt.Sprintf("could not rename %s: %v", filepath.Base(dtsPath), err))
		} else {
			events = append(events, uiEvent{Status: eventWrote, Target: "rotor-config.d.ts.bak", Detail: "rotor-config.d.ts → rotor-config.d.ts.bak"})
		}
	}

	events = append(events, uiEvent{
		Status:  eventFinished,
		Target:  "migration complete",
		Detail:  "review " + config.ConfigFileName + " and commit it",
		Elapsed: time.Since(start),
	})
	u.events(events)
	fmt.Fprintln(streams.out)
	return nil
}

// backup renames path to path + ".bak", replacing any existing .bak.
func backup(path string) error {
	bak := path + ".bak"
	_ = os.Remove(bak)
	return os.Rename(path, bak)
}
