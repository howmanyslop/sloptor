package compile

import (
	"errors"
	"fmt"
	"path/filepath"
	"slices"

	"rotor/internal/flamework"
	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/printer"
	"rotor/tsgo/sourcemap"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs"
	"rotor/tsgo/vfs/wrapvfs"
)

type nativeSourceOverlay struct {
	fileName string
	text     string
	trace    *sourceTraceMap
}

type nativeProgramUpdate struct {
	program     *compiler.Program
	sourceFiles []*ast.SourceFile
	traces      diagnosticTraces
	overlays    []nativeSourceOverlay
}

// derivedProgramDefaultCheckers sizes the checker pool of every program rotor
// derives from a project's program by overlaying transformed source: the
// native Flamework visit, semantic diagnostics and rendering all fan out across
// it. The project's own program keeps the configured count, because its
// checkers emit declarations, whose text follows checker type order; a fixed
// count rather than a CPU-derived one keeps everything else alike on every
// machine too.
const derivedProgramDefaultCheckers = 8

// derivedProgramCheckers is the checker count for a program derived from
// program: derivedProgramDefaultCheckers, unless the project configured a
// count (--checkers or tsconfig `checkers`) or runs single-threaded, which it
// keeps.
func derivedProgramCheckers(program *compiler.Program) int {
	if program.Options().Checkers != nil || program.SingleThreaded() {
		return 0
	}
	return derivedProgramDefaultCheckers
}

func updateProgramWithTextOverlays(program *compiler.Program, overlays map[string]string) (*compiler.Program, bool, error) {
	if len(overlays) == 0 {
		return program, true, nil
	}

	caseSensitive := program.Host().FS().UseCaseSensitiveFileNames()
	order := make(map[string]int, len(program.SourceFiles()))
	for index, sourceFile := range program.SourceFiles() {
		order[normalizeOverlayPath(sourceFile.FileName(), caseSensitive)] = index
	}
	paths := make([]string, 0, len(overlays))
	for path := range overlays {
		paths = append(paths, path)
	}
	slices.SortStableFunc(paths, func(left, right string) int {
		return order[normalizeOverlayPath(left, caseSensitive)] - order[normalizeOverlayPath(right, caseSensitive)]
	})

	changed := make([]tspath.Path, 0, len(paths))
	hostOverlays := make(map[string]string, len(overlays))
	for _, path := range paths {
		original := program.GetSourceFile(path)
		if original == nil {
			return nil, false, fmt.Errorf("compile: overlay source missing from Program: %s", path)
		}
		hostOverlays[normalizeOverlayPath(path, caseSensitive)] = overlays[path]
		if original.Text() == overlays[path] {
			continue
		}
		changed = append(changed, original.Path())
	}
	if len(changed) == 0 {
		return program, true, nil
	}

	host := newNativeOverlayHost(program.Host(), hostOverlays)
	updated, reused := program.UpdateProgramFilesWithCheckers(changed, host, nil, derivedProgramCheckers(program))
	return updated, reused, nil
}

func updateNativeFlameworkProgram(input nativeProgramUpdate) (*compiler.Program, []*ast.SourceFile, diagnosticTraces, error) {
	if len(input.overlays) == 0 {
		return input.program, input.sourceFiles, input.traces, nil
	}

	caseSensitive := input.program.Host().FS().UseCaseSensitiveFileNames()
	order := make(map[string]int, len(input.sourceFiles))
	for index, sourceFile := range input.sourceFiles {
		order[normalizeOverlayPath(sourceFile.FileName(), caseSensitive)] = index
	}
	overlays := slices.Clone(input.overlays)
	slices.SortStableFunc(overlays, func(left, right nativeSourceOverlay) int {
		return order[normalizeOverlayPath(left.fileName, caseSensitive)] - order[normalizeOverlayPath(right.fileName, caseSensitive)]
	})

	texts := make(map[string]string, len(overlays))
	composed := make(diagnosticTraces, len(input.traces)+len(overlays))
	for key, trace := range input.traces {
		composed[key] = trace
	}
	changed := make([]tspath.Path, 0, len(overlays))
	for _, overlay := range overlays {
		texts[normalizeOverlayPath(overlay.fileName, caseSensitive)] = overlay.text
		key := normalizeSourceFilePath(overlay.fileName)
		composed[key] = composeSourceTraceMaps(overlay.trace, input.traces[key])
		original := input.program.GetSourceFile(overlay.fileName)
		if original == nil {
			return nil, nil, nil, fmt.Errorf("compile: native Flamework source missing from Program: %s", overlay.fileName)
		}
		changed = append(changed, original.Path())
	}

	host := newNativeOverlayHost(input.program.Host(), texts)
	program, _ := input.program.UpdateProgramFilesWithCheckers(changed, host, nil, derivedProgramCheckers(input.program))

	remapped, err := remapProgramSourceFiles(program, input.sourceFiles)
	if err != nil {
		return nil, nil, nil, err
	}
	return program, remapped, composed, nil
}

type nativeOverlayHost struct {
	compiler.CompilerHost
	fs          vfs.FS
	readOverlay func(string) (string, bool)
}

func newNativeOverlayHost(base compiler.CompilerHost, overlays map[string]string) compiler.CompilerHost {
	baseFS := base.FS()
	caseSensitive := baseFS.UseCaseSensitiveFileNames()
	aliases := overlayAliases(overlays, "", caseSensitive)
	readOverlay := func(path string) (string, bool) {
		return overlayText(overlays, aliases, path, caseSensitive)
	}
	fs := wrapvfs.Wrap(baseFS, wrapvfs.Replacements{
		FileExists: func(path string) bool {
			if _, ok := readOverlay(path); ok {
				return true
			}
			return baseFS.FileExists(path)
		},
		ReadFile: func(path string) (string, bool) {
			if text, ok := readOverlay(path); ok {
				return text, true
			}
			return baseFS.ReadFile(path)
		},
	})
	return nativeOverlayHost{CompilerHost: base, fs: fs, readOverlay: readOverlay}
}

func (host nativeOverlayHost) FS() vfs.FS {
	return host.fs
}

func (host nativeOverlayHost) GetSourceFile(opts ast.SourceFileParseOptions) *ast.SourceFile {
	text, ok := host.readOverlay(opts.FileName)
	if !ok {
		// Preserve the underlying host's parse cache and any host-specific source
		// loading behavior for files this overlay does not change.
		return host.CompilerHost.GetSourceFile(opts)
	}
	return parser.ParseSourceFile(opts, text, core.GetScriptKindFromFileName(opts.FileName))
}

func printFlameworkSource(sourceFile *ast.SourceFile, metadata flamework.SourceMetadata) (string, *sourceTraceMap, error) {
	return printNativeSource(sourceFile, metadata.Original(), metadata.EmitContext(), metadata.Trace().OriginalFileName(), metadata.Trace().OriginalText())
}

// printNativeSource prints a natively transformed source file and the trace
// map that takes its positions back to originalText.
func printNativeSource(sourceFile, original *ast.SourceFile, emitContext *printer.EmitContext, originalFileName, originalText string) (string, *sourceTraceMap, error) {
	writer := printer.NewTextWriter("\n", 0)
	generator := sourcemap.NewGenerator(
		filepath.Base(sourceFile.FileName()),
		"",
		filepath.Dir(sourceFile.FileName()),
		tspath.ComparePathsOptions{UseCaseSensitiveFileNames: true, CurrentDirectory: filepath.Dir(sourceFile.FileName())},
	)
	printer.NewPrinter(printer.PrinterOptions{SourceMap: true, InlineSources: true}, printer.PrintHandlers{}, emitContext).Write(sourceFile.AsNode(), original, writer, generator)
	textWriter, ok := writer.(interface{ String() string })
	if !ok {
		return "", nil, errors.New("compile: TypeScript printer writer does not expose text")
	}
	raw := generator.RawSourceMap()
	trace, err := newSourceTraceMapFromMappings(raw.Version, raw.Mappings, originalFileName, originalText)
	if err != nil {
		return "", nil, err
	}
	return textWriter.String(), trace, nil
}
