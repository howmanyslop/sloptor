package compile

import (
	"path"
	"sort"
	"strings"
	"sync"

	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	"rotor/tsgo/module"
	"rotor/tsgo/parser"
	"rotor/tsgo/scanner"
	"rotor/tsgo/tspath"
)

// declarationPathRewriter is the native port of the sidecar's `transformPaths`
// (formerly tools/sidecar/lib/declarations.js): every module specifier a
// `.d.ts` carries is rewritten from its `paths`/`baseUrl` spelling to a path
// relative to the emitting file, because the Luau runtime resolves neither.
// Upstream rbxtsc runs this as an `afterDeclarations` transformer; tsgo has no
// custom-transformer hook, so the rewrite is applied to the emitted text,
// driven by a reparse of that text (never a regex).
//
// Three divergences from the JS version, all deliberate:
//
//   - An unresolved specifier falls back to a real resolver pass
//     (resolveFresh). The JS transformer resolved specifiers through a
//     module-resolution cache that only ever saw what the program had already
//     resolved for that file, so `import("alias/x")` types synthesized by the
//     declaration emitter for INFERRED types kept their alias spelling and
//     shipped unresolvable. This is a fix, not a difference of opinion.
//
//   - The visitor recurses into NESTED `import()` types. The JS returned an
//     updated ImportTypeNode without descending into its type arguments, so
//     `import("@alias/a").T<import("@alias/b").U>` left the inner specifier
//     alone. Here every specifier in the tree is visited, for the same reason
//     as above: an alias spelling the runtime cannot resolve is a bug wherever
//     it sits.
//
//   - rootDirs is applied unconditionally. This is INHERITED from rbxtsc, not
//     a faithful port of tsc: upstream `transformPaths` reads
//     `compilerOptions.rootDirs` directly, while tsc gates the equivalent
//     collapse on a `useRootDirs` flag that declaration emit leaves off. It is
//     kept because it is what the fork ships and what existing projects
//     published their types against.
type declarationPathRewriter struct {
	program            *compiler.Program
	options            *core.CompilerOptions
	implicitExtensions []string
	rootDirs           []string
	compare            tspath.ComparePathsOptions
	currentDirectory   string
	useCaseSensitive   bool
	resolverMu         sync.Mutex
	resolver           *module.Resolver
}

// newDeclarationPathRewriter returns nil for a project whose declarations can
// carry no alias spelling at all (no `baseUrl`, no `paths`), which is the same
// early-out the JS transformer took and keeps the emit path free of a reparse
// per file.
func newDeclarationPathRewriter(program *compiler.Program) *declarationPathRewriter {
	options := program.Options()
	if options.BaseUrl == "" && (options.Paths == nil || options.Paths.Size() == 0) {
		return nil
	}
	extensions := []string{tspath.ExtensionTs, tspath.ExtensionDts}
	allowsJS := options.AllowJs.IsTrue()
	allowsJSX := options.Jsx != core.JsxEmitNone
	if allowsJS {
		extensions = append(extensions, tspath.ExtensionJs)
	}
	if allowsJSX {
		extensions = append(extensions, tspath.ExtensionTsx)
	}
	if allowsJS && allowsJSX {
		extensions = append(extensions, tspath.ExtensionJsx)
	}
	if options.ResolveJsonModule.IsTrue() {
		extensions = append(extensions, tspath.ExtensionJson)
	}
	useCaseSensitive := program.Host().FS().UseCaseSensitiveFileNames()
	currentDirectory := program.Host().GetCurrentDirectory()
	var rootDirs []string
	for _, dir := range options.RootDirs {
		// Upstream filters rootDirs with path.isAbsolute; a relative entry is
		// dropped rather than resolved.
		if tspath.IsRootedDiskPath(tspath.NormalizeSlashes(dir)) {
			rootDirs = append(rootDirs, tspath.NormalizePath(dir))
		}
	}
	return &declarationPathRewriter{
		program:            program,
		options:            options,
		implicitExtensions: extensions,
		rootDirs:           rootDirs,
		compare:            tspath.ComparePathsOptions{UseCaseSensitiveFileNames: useCaseSensitive},
		currentDirectory:   currentDirectory,
		useCaseSensitive:   useCaseSensitive,
	}
}

// textEdit is a half-open byte range in the emitted declaration text and the
// text that replaces it. The `.d.ts.map` fixup consumes these too, so they
// carry the sizes it needs rather than being applied and forgotten.
type textEdit struct {
	start int
	end   int
	text  string
}

// nonOverlappingTextEdits sorts edits by position and drops any that starts
// inside the previous one. The text splice and the `.d.ts.map` column fixup
// consume the SAME slice, so the filtering happens once, up front: dropping an
// edit in only one of them would leave the two disagreeing about where the
// text ended up.
func nonOverlappingTextEdits(edits []textEdit) []textEdit {
	if len(edits) < 2 {
		return edits
	}
	sort.SliceStable(edits, func(i, j int) bool { return edits[i].start < edits[j].start })
	kept := edits[:1]
	for _, edit := range edits[1:] {
		if edit.start < kept[len(kept)-1].end {
			continue
		}
		kept = append(kept, edit)
	}
	return kept
}

// applyTextEdits splices edits into text. They must already be sorted and
// non-overlapping, which is what nonOverlappingTextEdits returns.
func applyTextEdits(text string, edits []textEdit) string {
	if len(edits) == 0 {
		return text
	}
	var builder strings.Builder
	builder.Grow(len(text))
	cursor := 0
	for _, edit := range edits {
		if edit.start < cursor {
			continue
		}
		builder.WriteString(text[cursor:edit.start])
		builder.WriteString(edit.text)
		cursor = edit.end
	}
	builder.WriteString(text[cursor:])
	return builder.String()
}

// specifierEdits returns the replacements every rewritable module specifier in
// declText needs, sorted by position. originalFile is the .ts/.tsx the
// declaration was emitted from: specifiers resolve relative to that file, not
// to the .d.ts output path (which is where upstream resolves them too, because
// the declaration transformer runs on the input source file node).
func (r *declarationPathRewriter) specifierEdits(originalFile *ast.SourceFile, declFileName string, declText string) []textEdit {
	if r == nil || !strings.ContainsAny(declText, "\"'") {
		return nil
	}
	parsed := parser.ParseSourceFile(
		ast.SourceFileParseOptions{
			FileName: declFileName,
			Path:     tspath.ToPath(declFileName, r.currentDirectory, r.useCaseSensitive),
		},
		declText,
		core.ScriptKindTS,
	)
	if parsed == nil {
		return nil
	}
	resolutions := r.program.GetResolvedModules()[originalFile.Path()]
	// The declaration text is a reparse, so its nodes carry no resolution
	// metadata; the mode a specifier resolved under is the ORIGINAL file's
	// default. Under `module: preserve` / `moduleResolution: bundler` one name
	// can sit in the cache twice under different modes with different resolved
	// files, and Go map iteration order is random, so picking by name alone
	// would emit nondeterministic text and churn the incremental hash on every
	// build.
	mode := r.program.GetDefaultResolutionModeForFile(originalFile)
	fileName := tspath.NormalizePath(originalFile.FileName())
	fileDirectory := tspath.GetDirectoryPath(fileName)

	var edits []textEdit
	record := func(literal *ast.Node) {
		if literal == nil || !ast.IsStringLiteral(literal) {
			return
		}
		replacement, ok := r.resolveSpecifier(resolutions, mode, fileName, fileDirectory, literal.Text())
		if !ok {
			return
		}
		start := scanner.SkipTrivia(declText, literal.Pos())
		if start >= literal.End() || declText[start] != '"' && declText[start] != '\'' {
			return
		}
		edits = append(edits, textEdit{start: start, end: literal.End(), text: quoteSpecifier(replacement)})
	}
	var visit func(node *ast.Node) bool
	visit = func(node *ast.Node) bool {
		switch node.Kind {
		case ast.KindImportDeclaration:
			record(node.AsImportDeclaration().ModuleSpecifier)
		case ast.KindExportDeclaration:
			record(node.AsExportDeclaration().ModuleSpecifier)
		case ast.KindExternalModuleReference:
			record(node.AsExternalModuleReference().Expression)
		case ast.KindImportType:
			argument := node.AsImportTypeNode().Argument
			if argument != nil && argument.Kind == ast.KindLiteralType {
				record(argument.AsLiteralTypeNode().Literal)
			}
		case ast.KindCallExpression:
			call := node.AsCallExpression()
			callee := call.Expression
			if callee != nil && call.Arguments != nil && len(call.Arguments.Nodes) == 1 {
				isRequire := ast.IsIdentifier(callee) && callee.Text() == "require"
				isImport := callee.Kind == ast.KindImportKeyword
				if isRequire || isImport {
					record(call.Arguments.Nodes[0])
				}
			}
		}
		node.ForEachChild(visit)
		return false
	}
	parsed.AsNode().ForEachChild(visit)
	return edits
}

// resolveFresh answers specifiers the program never resolved: the declaration
// emitter synthesizes import types for inferred types, and those specifiers
// need not appear in the source file's own import list. The JS transformer
// reached ts.resolveModuleName for these; without it an `import("alias/x")`
// ships unresolvable.
func (r *declarationPathRewriter) resolveFresh(containingFile string, specifier string, mode core.ResolutionMode) *module.ResolvedModule {
	r.resolverMu.Lock()
	defer r.resolverMu.Unlock()
	if r.resolver == nil {
		r.resolver = module.NewResolver(r.program.Host(), r.options, "", "")
	}
	resolved, _ := r.resolver.ResolveModuleName(specifier, containingFile, mode, nil)
	return resolved
}

// quoteSpecifier wraps a resolved path in a double-quoted TypeScript string
// literal. A path may legitimately contain a quote or a backslash on a POSIX
// filesystem, and splicing one in raw would produce a `.d.ts` that no longer
// parses.
func quoteSpecifier(specifier string) string {
	if !strings.ContainsAny(specifier, "\"\\") {
		return "\"" + specifier + "\""
	}
	var builder strings.Builder
	builder.Grow(len(specifier) + 4)
	builder.WriteByte('"')
	for index := 0; index < len(specifier); index++ {
		if specifier[index] == '"' || specifier[index] == '\\' {
			builder.WriteByte('\\')
		}
		builder.WriteByte(specifier[index])
	}
	builder.WriteByte('"')
	return builder.String()
}

// resolveSpecifier returns the relative spelling for specifier, or ok=false
// when it must be left alone: an external-library import (a `@rbxts/*`
// package), or anything that does not resolve at all — which is also how a URL
// specifier survives untouched, since no lookup can ever answer it.
func (r *declarationPathRewriter) resolveSpecifier(resolutions module.ModeAwareCache[*module.ResolvedModule], mode core.ResolutionMode, containingFile string, fileDirectory string, specifier string) (string, bool) {
	resolved := resolutions[module.ModeAwareCacheKey{Name: specifier, Mode: mode}]
	if !resolved.IsResolved() && mode != core.ResolutionModeNone {
		resolved = resolutions[module.ModeAwareCacheKey{Name: specifier, Mode: core.ResolutionModeNone}]
	}
	if !resolved.IsResolved() {
		resolved = r.resolveFresh(containingFile, specifier, mode)
	}
	if !resolved.IsResolved() || resolved.IsExternalLibraryImport {
		return "", false
	}

	resolvedFileName := tspath.NormalizePath(resolved.ResolvedFileName)
	directory := fileDirectory
	moduleDirectory := tspath.GetDirectoryPath(resolvedFileName)
	if len(r.rootDirs) > 0 {
		fileRoot := r.longestContainingRoot(containingFile)
		moduleRoot := r.longestContainingRoot(resolvedFileName)
		if fileRoot != "" && moduleRoot != "" {
			directory = r.relativeFrom(fileRoot, directory)
			moduleDirectory = r.relativeFrom(moduleRoot, moduleDirectory)
		}
	}
	resolvedPath := path.Join(r.relativeFrom(directory, moduleDirectory), tspath.GetBaseFileName(resolvedFileName))
	for _, extension := range r.implicitExtensions {
		if extension == resolved.Extension {
			resolvedPath = strings.TrimSuffix(resolvedPath, extension)
			break
		}
	}
	if resolvedPath == "" {
		return "", false
	}
	if !strings.HasPrefix(resolvedPath, ".") {
		resolvedPath = "./" + resolvedPath
	}
	return resolvedPath, true
}

// relativeFrom is `path.relative` with the platform's case rule (Node's
// win32 implementation compares case-insensitively; Go's filepath.Rel does
// not, which would emit a bogus `../` chain for a differently-cased drive or
// directory component on Windows).
func (r *declarationPathRewriter) relativeFrom(from string, to string) string {
	if (tspath.GetRootLength(from) > 0) != (tspath.GetRootLength(to) > 0) {
		return to
	}
	return tspath.GetRelativePathFromDirectory(from, to, r.compare)
}

// longestContainingRoot mirrors upstream's rootDirs scan: the longest
// `rootDirs` entry that contains candidate, or "" when none does.
func (r *declarationPathRewriter) longestContainingRoot(candidate string) string {
	best := ""
	for _, root := range r.rootDirs {
		if !tspath.ContainsPath(root, candidate, r.compare) {
			continue
		}
		if len(root) > len(best) {
			best = root
		}
	}
	return best
}
