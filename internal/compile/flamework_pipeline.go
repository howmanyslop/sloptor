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
	"rotor/tsgo/printer"
	"rotor/tsgo/sourcemap"
	"rotor/tsgo/tspath"
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
	project, err := flamework.OpenProject(flamework.ProjectOptions{
		ProjectDir:       filepath.FromSlash(dir),
		RootDir:          filepath.FromSlash(program.Options().RootDir),
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
	originalProgram := program
	originalFiles := sourceFiles
	currentFiles := sourceFiles
	traces := diagnosticTraces(nil)
	if len(pipeline.prefix) > 0 {
		prepared, stageDiags, err := applyExternalTransformerStage(dir, program, currentFiles, overlays, traces, pipeline.prefix)
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
	if len(pipeline.suffix) > 0 {
		next, stageDiags, stageErr := applyExternalTransformerStage(dir, program, currentFiles, overlays, traces, pipeline.suffix)
		if stageErr != nil {
			return nil, stageDiags, stageErr
		}
		prepared = next
	}
	declarations, diags, err := runDeclarationTransformerStage(dir, originalProgram, originalFiles, overlays, pipeline.plugins)
	if err != nil {
		return nil, diags, err
	}
	prepared.declarations = declarations
	prepared.flamework = pipeline.config
	return &compilePipelineResult{prepared: prepared, flameworkProject: pipeline.project}, nil, nil
}

func applyExternalTransformerStage(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string, traces diagnosticTraces, plugins []transformerPluginConfig) (*preparedTransformerProgram, []string, error) {
	transformed, diags, err := applyTransformerSidecarWithPlugins(dir, program, sourceFiles, overlays, rawTransformerPlugins(plugins), false)
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

func runDeclarationTransformerStage(dir string, program *compiler.Program, sourceFiles []*ast.SourceFile, overlays map[string]string, plugins []transformerPluginConfig) ([]sidecarOutputFile, []string, error) {
	if len(plugins) == 0 && !declarationUsesPathAliases(program) {
		return nil, nil, nil
	}
	transformed, diags, err := applyTransformerSidecarWithPlugins(dir, program, sourceFiles, overlays, rawTransformerPlugins(plugins), true)
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

	programOverlays := normalizeOverlays(overlays)
	composed := make(diagnosticTraces, len(sourceFiles))
	for key, trace := range traces {
		composed[key] = trace
	}
	caseSensitive := program.Host().FS().UseCaseSensitiveFileNames()
	for index, metadata := range result.Sources {
		text, trace, err := printFlameworkSource(result.Files[index], metadata)
		if err != nil {
			return nil, nil, err
		}
		key := normalizeSourceFilePath(metadata.FileName())
		programOverlays[normalizeOverlayPath(metadata.FileName(), caseSensitive)] = text
		composed[key] = composeSourceTraceMaps(trace, traces[key])
	}

	transformedProgram, diags, err := newProjectProgramWithOverlay(dir, program.Options().ConfigFilePath, programOverlays, program.Options().Checkers)
	if err != nil {
		return nil, stringDiagnostics(diags), err
	}
	remapped, err := remapProgramSourceFiles(transformedProgram, sourceFiles)
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

func printFlameworkSource(sourceFile *ast.SourceFile, metadata flamework.SourceMetadata) (string, *sourceTraceMap, error) {
	writer := printer.NewTextWriter("\n", 0)
	generator := sourcemap.NewGenerator(
		filepath.Base(sourceFile.FileName()),
		"",
		filepath.Dir(sourceFile.FileName()),
		tspath.ComparePathsOptions{UseCaseSensitiveFileNames: true, CurrentDirectory: filepath.Dir(sourceFile.FileName())},
	)
	printer.NewPrinter(printer.PrinterOptions{SourceMap: true, InlineSources: true}, printer.PrintHandlers{}, metadata.EmitContext()).Write(sourceFile.AsNode(), metadata.Original(), writer, generator)
	textWriter, ok := writer.(interface{ String() string })
	if !ok {
		return "", nil, errors.New("compile: TypeScript printer writer does not expose text")
	}
	raw, err := json.Marshal(generator.RawSourceMap())
	if err != nil {
		return "", nil, fmt.Errorf("compile: encode native Flamework trace: %w", err)
	}
	trace, err := newSourceTraceMap(string(raw), metadata.Trace().OriginalFileName(), metadata.Trace().OriginalText())
	if err != nil {
		return "", nil, err
	}
	return textWriter.String(), trace, nil
}
