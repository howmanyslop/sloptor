package compile

import (
	"encoding/json"
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
	updated, reused := program.UpdateProgramFiles(changed, host, nil)
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
	program, _ := input.program.UpdateProgramFiles(changed, host, nil)

	remapped, err := remapProgramSourceFiles(program, input.sourceFiles)
	if err != nil {
		return nil, nil, nil, err
	}
	return program, remapped, composed, nil
}

type nativeOverlayHost struct {
	compiler.CompilerHost
	fs vfs.FS
}

func newNativeOverlayHost(base compiler.CompilerHost, overlays map[string]string) compiler.CompilerHost {
	baseFS := base.FS()
	caseSensitive := baseFS.UseCaseSensitiveFileNames()
	aliases := overlayAliases(overlays, "", caseSensitive)
	fs := wrapvfs.Wrap(baseFS, wrapvfs.Replacements{
		FileExists: func(path string) bool {
			if _, ok := overlayText(overlays, aliases, path, caseSensitive); ok {
				return true
			}
			return baseFS.FileExists(path)
		},
		ReadFile: func(path string) (string, bool) {
			if text, ok := overlayText(overlays, aliases, path, caseSensitive); ok {
				return text, true
			}
			return baseFS.ReadFile(path)
		},
	})
	return nativeOverlayHost{CompilerHost: base, fs: fs}
}

func (host nativeOverlayHost) FS() vfs.FS {
	return host.fs
}

func (host nativeOverlayHost) GetSourceFile(opts ast.SourceFileParseOptions) *ast.SourceFile {
	text, ok := host.fs.ReadFile(opts.FileName)
	if !ok {
		return nil
	}
	return parser.ParseSourceFile(opts, text, core.GetScriptKindFromFileName(opts.FileName))
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
