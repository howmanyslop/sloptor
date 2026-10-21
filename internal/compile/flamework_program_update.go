package compile

import (
	"errors"
	"fmt"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"sync"

	"rotor/internal/flamework"
	"rotor/internal/fsutil"
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

	updated, reused := updateProgramFilesWithOverlays(program, hostOverlays, changed)
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
	changed := make([]tspath.Path, 0, len(overlays))
	for _, overlay := range overlays {
		texts[normalizeOverlayPath(overlay.fileName, caseSensitive)] = overlay.text
		original := input.program.GetSourceFile(overlay.fileName)
		if original == nil {
			return nil, nil, nil, fmt.Errorf("compile: native Flamework source missing from Program: %s", overlay.fileName)
		}
		changed = append(changed, original.Path())
	}
	composed := composeNativeOverlayTraces(input.program, input.traces, overlays)

	program, _ := updateProgramFilesWithOverlays(input.program, texts, changed)

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
	preparsed   *preparsedOverlays
}

func newNativeOverlayHost(base compiler.CompilerHost, overlays map[string]string) compiler.CompilerHost {
	return newNativeOverlayHostWithAliases(base, overlays, overlayAliases(overlays, "", base.FS().UseCaseSensitiveFileNames()))
}

func newNativeOverlayHostWithAliases(base compiler.CompilerHost, overlays, aliases map[string]string) nativeOverlayHost {
	baseFS := base.FS()
	caseSensitive := baseFS.UseCaseSensitiveFileNames()
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
	if file := host.preparsed.take(opts, text); file != nil {
		return file
	}
	return parser.ParseSourceFile(opts, text, core.GetScriptKindFromFileName(opts.FileName))
}

// updateProgramFilesWithOverlays applies the changed overlay files with one
// UpdateProgramFiles call. That call asks for each changed file in turn through
// GetSourceFile(oldFile.ParseOptions()), so it parses them one at a time.
// Unless the Program is single-threaded, the overlay aliases resolve and the
// changed files parse first, in work groups, and the host gives each parse to
// the first request with the same options and text. UpdateProgramFiles still
// decides membership, order, reuse and rebuilds.
func updateProgramFilesWithOverlays(program *compiler.Program, overlays map[string]string, changed []tspath.Path) (*compiler.Program, bool) {
	base := program.Host()
	if program.SingleThreaded() {
		return program.UpdateProgramFiles(changed, newNativeOverlayHost(base, overlays), nil)
	}
	caseSensitive := base.FS().UseCaseSensitiveFileNames()
	host := newNativeOverlayHostWithAliases(base, overlays, parallelOverlayAliases(overlays, caseSensitive))
	host.preparsed = preparseOverlays(program, overlays, changed, caseSensitive)
	// The updated Program keeps this host; once released it parses every
	// request again, as a host without preparsed files does.
	defer host.preparsed.release()
	return program.UpdateProgramFiles(changed, host, nil)
}

// preparsedOverlays holds overlay parses made before UpdateProgramFiles asks
// for them. Only the first request with the same parse options and overlay text
// takes a parse; every other request parses as before.
type preparsedOverlays struct {
	mu    sync.Mutex
	files map[ast.SourceFileParseOptions]*overlayParse
}

type overlayParse struct {
	opts      ast.SourceFileParseOptions
	text      string
	file      *ast.SourceFile
	panicked  bool
	recovered any
}

// preparseOverlays parses the changed overlay files that UpdateProgramFiles can
// request: the changed paths, in order, with the parse options of the Program
// file at each path. It stops at the first path where the replacement loop
// rebuilds the whole Program before it asks for the file (a path the Program
// does not hold, or a package redirect). That is not always the first rebuild:
// a replacement whose references or other non-import shape changed, or whose
// imports resolve to a file the Program does not hold, rebuilds it too. The
// rebuild asks for later files with its own options or leaves them out of the
// graph, so a parse made here can go unused; release then drops it unseen.
// A request without a lexical overlay is left alone; it may resolve through an
// alias or the base host. Overlays that are not changed are never parsed here.
func preparseOverlays(program *compiler.Program, overlays map[string]string, changed []tspath.Path, caseSensitive bool) *preparsedOverlays {
	jobs := make([]overlayParse, 0, len(changed))
	seen := make(map[ast.SourceFileParseOptions]struct{}, len(changed))
	for _, path := range changed {
		oldFile := program.GetSourceFileByPath(path)
		// A redirect path maps to its target file, whose path differs.
		if oldFile == nil || oldFile.Path() != path || program.GetRedirectTargets(path) != nil {
			break
		}
		opts := oldFile.ParseOptions()
		text, ok := overlays[normalizeOverlayPath(opts.FileName, caseSensitive)]
		if _, repeated := seen[opts]; !ok || repeated {
			continue
		}
		seen[opts] = struct{}{}
		jobs = append(jobs, overlayParse{opts: opts, text: text})
	}
	if len(jobs) < 2 {
		return nil
	}
	wg := core.NewWorkGroup(false)
	for index := range jobs {
		wg.Queue(jobs[index].parse)
	}
	wg.RunAndWait()
	files := make(map[ast.SourceFileParseOptions]*overlayParse, len(jobs))
	for index := range jobs {
		files[jobs[index].opts] = &jobs[index]
	}
	return &preparsedOverlays{files: files}
}

// parse is the parse the overlay host runs for the request. A panic is kept and
// raised again by take on the requesting goroutine, so a parse that no request
// takes cannot fail the build.
func (job *overlayParse) parse() {
	defer func() {
		if recovered := recover(); recovered != nil {
			job.panicked, job.recovered = true, recovered
		}
	}()
	job.file = parser.ParseSourceFile(job.opts, job.text, core.GetScriptKindFromFileName(job.opts.FileName))
}

func (p *preparsedOverlays) take(opts ast.SourceFileParseOptions, text string) *ast.SourceFile {
	if p == nil {
		return nil
	}
	p.mu.Lock()
	parsed := p.files[opts]
	delete(p.files, opts)
	p.mu.Unlock()
	if parsed == nil || parsed.text != text {
		return nil
	}
	if parsed.panicked {
		panic(parsed.recovered)
	}
	return parsed.file
}

func (p *preparsedOverlays) release() {
	if p == nil {
		return
	}
	p.mu.Lock()
	p.files = nil
	p.mu.Unlock()
}

type overlayAliasEntry struct {
	path, text string
	physical   string
	resolved   bool
	panicked   bool
	recovered  any
}

// parallelOverlayAliases returns overlayAliases(overlays, "", caseSensitive)
// with the realpath lookups run concurrently. One range over overlays fixes the
// order the serial loop visits; each lookup fills only its own entry, and
// mergeOverlayAliases then inserts in that order, so a colliding alias keeps
// the text the serial loop would leave in it.
func parallelOverlayAliases(overlays map[string]string, caseSensitive bool) map[string]string {
	if len(overlays) <= 1 {
		return overlayAliases(overlays, "", caseSensitive)
	}
	entries := make([]overlayAliasEntry, 0, len(overlays))
	for path, text := range overlays {
		entries = append(entries, overlayAliasEntry{path: path, text: text})
	}
	configDir := filepath.Dir(filepath.FromSlash(""))
	canonicalConfigDir, configErr := fsutil.EvalSymlinks(configDir)
	// Workers stride over the entries, so no more realpath calls block at once
	// than there are Ps.
	workers := min(runtime.GOMAXPROCS(0), len(entries))
	wg := core.NewWorkGroup(false)
	for worker := range workers {
		wg.Queue(func() {
			for index := worker; index < len(entries); index += workers {
				entries[index].resolve(caseSensitive)
			}
		})
	}
	wg.RunAndWait()
	return mergeOverlayAliases(entries, configDir, canonicalConfigDir, configErr, caseSensitive)
}

func (entry *overlayAliasEntry) resolve(caseSensitive bool) {
	defer func() {
		if recovered := recover(); recovered != nil {
			entry.panicked, entry.recovered = true, recovered
		}
	}()
	entry.physical, entry.resolved = canonicalExistingOverlayPath(entry.path, caseSensitive)
}

// mergeOverlayAliases is the insertion loop of overlayAliases over resolved
// entries, in entry order. A lookup panic is raised again at its entry.
func mergeOverlayAliases(entries []overlayAliasEntry, configDir, canonicalConfigDir string, configErr error, caseSensitive bool) map[string]string {
	aliases := make(map[string]string, len(entries)*2)
	for _, entry := range entries {
		aliases[normalizeOverlayPath(entry.path, caseSensitive)] = entry.text
		if entry.panicked {
			panic(entry.recovered)
		}
		if !entry.resolved {
			continue
		}
		aliases[entry.physical] = entry.text
		if configErr != nil {
			continue
		}
		relative, err := filepath.Rel(canonicalConfigDir, filepath.FromSlash(entry.physical))
		if err == nil && relative != ".." && !strings.HasPrefix(relative, ".."+string(filepath.Separator)) {
			aliases[normalizeOverlayPath(relative, caseSensitive)] = entry.text
			aliases[normalizeOverlayPath(filepath.Join(configDir, relative), caseSensitive)] = entry.text
		}
	}
	return aliases
}

// composeNativeOverlayTraces copies traces and adds each overlay's trace
// composed with the incoming trace for its file. A trace map does not change
// once built, and composition only reads both maps and allocates its result,
// so overlays compose in the Program's work group; results then go in in
// overlay order, so a repeated file keeps its last overlay's trace.
func composeNativeOverlayTraces(program *compiler.Program, traces diagnosticTraces, overlays []nativeSourceOverlay) diagnosticTraces {
	keys := make([]string, len(overlays))
	results := make([]*sourceTraceMap, len(overlays))
	wg := core.NewWorkGroup(program.SingleThreaded() || len(overlays) <= 1)
	for index, overlay := range overlays {
		key := normalizeSourceFilePath(overlay.fileName)
		keys[index] = key
		inner := traces[key]
		wg.Queue(func() {
			results[index] = composeSourceTraceMaps(overlay.trace, inner)
		})
	}
	wg.RunAndWait()
	composed := make(diagnosticTraces, len(traces)+len(overlays))
	for key, trace := range traces {
		composed[key] = trace
	}
	for index, key := range keys {
		composed[key] = results[index]
	}
	return composed
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
	raw := generator.RawSourceMap()
	trace, err := newSourceTraceMapFromMappings(raw.Version, raw.Mappings, metadata.Trace().OriginalFileName(), metadata.Trace().OriginalText())
	if err != nil {
		return "", nil, err
	}
	return textWriter.String(), trace, nil
}
