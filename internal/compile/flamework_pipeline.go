package compile

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"rotor/internal/config"
	"rotor/internal/flamework"
	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
)

type compilePipelineResult struct {
	prepared         *preparedTransformerProgram
	flameworkProject *flamework.Project
}

type flameworkPipeline struct {
	config  *config.FlameworkConfig
	project *flamework.Project
	plugins []transformerPluginConfig
	prefix  []transformerPluginConfig
	suffix  []transformerPluginConfig
}

type dirtyFlameworkIncrementalStateError struct{}

func (dirtyFlameworkIncrementalStateError) Error() string {
	return "Flamework cannot be built in a dirty environment, please delete your tsbuildinfo"
}

func prepareCompilePipeline(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string, opts ProjectOptions) (*compilePipelineResult, []string, error) {
	pipeline, diags, err := prepareFlameworkPipeline(dir, program, opts)
	if err != nil {
		return nil, diags, err
	}
	return runCompilePipeline(dir, program, sourceFiles, overlays, pipeline)
}

func prepareFlameworkPipeline(dir string, program *compiler.Program, opts ProjectOptions) (*flameworkPipeline, []string, error) {
	configured, diags, err := prepareFlameworkConfig(dir, program.CommandLine())
	if err != nil {
		return nil, diags, err
	}
	if configured == nil {
		return nil, nil, nil
	}
	if err := rejectDirtyFlameworkIncrementalState(dir, program); err != nil {
		return nil, []string{err.Error()}, fmt.Errorf("compile: native Flamework incremental state: %w", err)
	}

	plugins, err := effectiveTransformerPlugins(program.Options().ConfigFilePath)
	if err != nil {
		return nil, []string{err.Error()}, errors.New("compile: resolve effective transformer plugins")
	}
	prefix, suffix, err := splitTransformerPlugins(plugins, configured.After)
	if err != nil {
		return nil, []string{filepath.Join(filepath.FromSlash(dir), "rotor.toml") + ": " + err.Error()}, errors.New("compile: invalid flamework.after")
	}
	// OpenProject takes a single root; Flamework's createPathTranslator
	// resolves that from rootDir or rootDirs (plus commonSourceDirectory).
	project, err := flamework.OpenProject(flamework.ProjectOptions{
		ProjectDir:       filepath.FromSlash(dir),
		RootDir:          createPathTranslator(program, false).RootDir,
		OutDir:           filepath.FromSlash(program.Options().OutDir),
		IncludeDirectory: opts.IncludePath,
		RojoConfigPath:   opts.RojoConfigPath,
		Declaration:      program.Options().GetEmitDeclarations(),
		Config:           *configured,
	})
	if err != nil {
		return nil, []string{err.Error()}, fmt.Errorf("compile: open native Flamework project: %w", err)
	}
	return &flameworkPipeline{config: configured, project: project, plugins: plugins, prefix: prefix, suffix: suffix}, nil, nil
}

func rejectDirtyFlameworkIncrementalState(dir string, program *compiler.Program) error {
	options := program.Options()
	if !options.IsIncremental() || options.TsBuildInfoFile == "" {
		return nil
	}
	if _, err := os.Stat(filepath.Join(filepath.FromSlash(dir), "flamework.build")); !errors.Is(err, os.ErrNotExist) {
		return nil
	}
	if _, err := os.Stat(createPathTranslator(program, true).BuildInfoOutputPath); err == nil {
		return dirtyFlameworkIncrementalStateError{}
	}
	return nil
}

func runCompilePipeline(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string, pipeline *flameworkPipeline) (*compilePipelineResult, []string, error) {
	if pipeline == nil {
		prepared, diags, err := prepareTransformerProgram(dir, program, sourceFiles, overlays)
		if err != nil {
			return nil, diags, err
		}
		return &compilePipelineResult{prepared: prepared}, nil, nil
	}
	// An incremental build that selected nothing has no source to transform and
	// no declaration to emit, so every stage below would hand the worker an
	// empty compileFileNames list. The worker still builds its whole
	// LanguageService program before discovering there is nothing to do (see
	// tools/sidecar/lib/session.js handleRequest), which costs seconds per
	// project. prepareTransformerProgram already short-circuits the same way
	// for the non-Flamework route.
	if len(sourceFiles) == 0 {
		return &compilePipelineResult{
			prepared:         &preparedTransformerProgram{program: program, sourceFiles: sourceFiles, flamework: pipeline.config},
			flameworkProject: pipeline.project,
		}, nil, nil
	}
	state := newSidecarBuildState()
	originalProgram := program
	originalFiles := sourceFiles
	currentFiles := sourceFiles
	traces := diagnosticTraces(nil)
	if len(pipeline.prefix) > 0 {
		prepared, stageDiags, err := applyExternalTransformerStage(dir, program, currentFiles, overlays, traces, pipeline.prefix, state)
		if err != nil {
			return nil, stageDiags, err
		}
		program = prepared.program
		currentFiles = prepared.sourceFiles
		traces = prepared.sourceTraces
	}
	prepared, infos, err := applyNativeFlameworkTransform(dir, program, currentFiles, overlays, traces, pipeline.project)
	if err != nil {
		return nil, diagnosticInfoMessages(infos), err
	}
	program = prepared.program
	currentFiles = prepared.sourceFiles
	traces = prepared.sourceTraces
	state.absorbSourceFiles(currentFiles)
	if len(pipeline.suffix) > 0 {
		next, stageDiags, stageErr := applyExternalTransformerStage(dir, program, currentFiles, overlays, traces, pipeline.suffix, state)
		if stageErr != nil {
			return nil, stageDiags, stageErr
		}
		prepared = next
	}
	declarations, diags, err := runDeclarationTransformerStage(dir, originalProgram, originalFiles, overlays, pipeline.plugins, state)
	if err != nil {
		return nil, diags, err
	}
	prepared.declarations = declarations
	prepared.flamework = pipeline.config
	state.applyTo(prepared)
	return &compilePipelineResult{prepared: prepared, flameworkProject: pipeline.project}, nil, nil
}

func applyExternalTransformerStage(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string, traces diagnosticTraces, plugins []transformerPluginConfig, state *sidecarBuildState) (*preparedTransformerProgram, []string, error) {
	transformed, diags, err := applyTransformerSidecarWithPlugins(dir, program, sourceFiles, overlays, rawTransformerPlugins(plugins), sidecarEmitSources, state)
	if err != nil {
		return nil, diags, err
	}
	remapped := sourceFiles
	if transformed.program != program {
		remapped, err = remapProgramSourceFiles(transformed.program, sourceFiles)
		if err != nil {
			return nil, nil, err
		}
	}
	composed := make(diagnosticTraces, len(traces)+len(transformed.sourceTraces))
	for key, trace := range traces {
		composed[key] = trace
	}
	for key, trace := range transformed.sourceTraces {
		composed[key] = composeSourceTraceMaps(trace, traces[key])
	}
	transformed.sourceFiles = remapped
	transformed.sourceTraces = composed
	return transformed, nil, nil
}

func runDeclarationTransformerStage(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string, plugins []transformerPluginConfig, state *sidecarBuildState) ([]sidecarOutputFile, []string, error) {
	if len(plugins) == 0 && !declarationUsesPathAliases(program) {
		return nil, nil, nil
	}
	transformed, diags, err := applyTransformerSidecarWithPlugins(dir, program, sourceFiles, overlays, rawTransformerPlugins(plugins), sidecarEmitDeclarations, state)
	if err != nil {
		return nil, diags, err
	}
	return transformed.declarations, nil, nil
}

func rawTransformerPlugins(plugins []transformerPluginConfig) []json.RawMessage {
	raw := make([]json.RawMessage, len(plugins))
	for index, plugin := range plugins {
		raw[index] = plugin.marshalJSON()
	}
	return raw
}

func applyNativeFlameworkTransform(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string, traces diagnosticTraces, project *flamework.Project) (*preparedTransformerProgram, []DiagnosticInfo, error) {
	checker, release := program.GetTypeChecker(context.Background())
	defer release()
	result, err := flamework.Transform(flamework.TransformInput{
		Program: program,
		Checker: checker,
		Files:   sourceFiles,
		Project: project,
	})
	if err != nil {
		return nil, nil, fmt.Errorf("compile: native Flamework transform: %w", err)
	}
	if len(result.Diagnostics) > 0 {
		return nil, tsDiagnosticInfos(result.Diagnostics, traces), errors.New("compile: native Flamework diagnostics")
	}

	// Upstream always prints a fresh source-file update, so by default every
	// source is overlaid back into the Program (restoring printFlameworkSource
	// and the overlay/reparse path). Only flamework.skipUnchangedFiles opts
	// back into pointer-identity reuse of unchanged files.
	skipUnchanged := project.SkipUnchangedFiles()
	changed := make([]nativeSourceOverlay, 0, len(result.Sources))
	for index, metadata := range result.Sources {
		if skipUnchanged && !metadata.Changed() {
			continue
		}
		text, trace, err := printFlameworkSource(result.Files[index], metadata)
		if err != nil {
			return nil, nil, err
		}
		changed = append(changed, nativeSourceOverlay{fileName: metadata.FileName(), text: text, trace: trace})
	}
	transformedProgram, remapped, composed, err := updateNativeFlameworkProgram(nativeProgramUpdate{
		program: program, sourceFiles: sourceFiles, traces: traces, overlays: changed,
	})
	if err != nil {
		return nil, nil, err
	}
	return &preparedTransformerProgram{
		program:      transformedProgram,
		sourceFiles:  remapped,
		flamework:    nil,
		sourceTraces: composed,
	}, nil, nil
}
