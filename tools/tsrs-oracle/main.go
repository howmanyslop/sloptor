// Command tsrs-oracle dumps what rotor's checker answers for every expression
// and declaration node of a project, one canonical line per node.
//
// It is the Go side of the rotor_tsrs differential: crates/rotor_tsrs writes
// the same format from tsrs, and the two dumps must agree line for line. The
// format is documented in crates/rotor_tsrs/src/dump.rs; keep the two in step.
//
// Usage: go run ./tools/tsrs-oracle -project <tsconfig> [-out <file>]
package main

import (
	"bufio"
	"context"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"rotor/internal/compile"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/checker"
	"rotor/tsgo/compiler"
	"rotor/tsgo/jsnum"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs"
	"rotor/tsgo/vfs/cachedvfs"
	"rotor/tsgo/vfs/osvfs"
	"rotor/tsgo/vfs/wrapvfs"
)

// The synthetic declarations internal/compile adds to every program
// (envdecl.go and macrodecl.go). They are declaration files, so the walk never
// visits them; only the global names they declare matter.
const (
	envDeclFileName   = "__rotor_env.d.ts"
	macroDeclFileName = "__rotor_macros.d.ts"
	envDeclText       = `// rotor compiler extension (synthetic, in-memory): the $env compile-time
// environment macro. This file is injected by the rotor compiler and does
// not exist on disk.
declare const $env: {
	readonly [envName: string]: string | undefined;
} & ((name: string, fallback: string) => string) &
	((name: string) => string | undefined);
`
	macroDeclText = `// rotor compiler extension (synthetic, in-memory): $nameof, $keys, and $file.
// This file is injected by the rotor
// compiler and does not exist on disk.
declare function $nameof(item: unknown): string;
declare function $keys<T>(): string[];
declare function $file(path: string): any;
`
)

func injectFile(inner vfs.FS, path, text string) vfs.FS {
	matches := func(candidate string) bool {
		if inner.UseCaseSensitiveFileNames() {
			return candidate == path
		}
		return strings.EqualFold(candidate, path)
	}
	return wrapvfs.Wrap(inner, wrapvfs.Replacements{
		FileExists: func(candidate string) bool {
			return matches(candidate) || inner.FileExists(candidate)
		},
		ReadFile: func(candidate string) (string, bool) {
			if matches(candidate) {
				return text, true
			}
			return inner.ReadFile(candidate)
		},
	})
}

// declaresOnDisk mirrors projectDeclaresEnvOnDisk and
// projectDeclaresMacrosOnDisk.
func declaresOnDisk(fs vfs.FS, fileNames []string, companion, marker string) bool {
	for _, name := range fileNames {
		base := name[strings.LastIndexByte(name, '/')+1:]
		if !strings.EqualFold(base, companion) && !strings.EqualFold(base, compile.RotorTypesFileName) {
			continue
		}
		if text, ok := fs.ReadFile(name); ok && strings.Contains(text, marker) {
			return true
		}
	}
	return false
}

// loadProgram builds the program the way newProjectProgramWithOptions does
// for a build without overlays, on one checker so the query order is fixed.
func loadProgram(configPath string) (*compiler.Program, error) {
	dir := tspath.GetDirectoryPath(configPath)
	fs := vfs.FS(cachedvfs.From(compile.SanitizeFSWithConfigPath(bundled.WrapFS(osvfs.FS()), configPath)))
	envDecl := dir + "/" + envDeclFileName
	macroDecl := dir + "/" + macroDeclFileName
	fs = injectFile(fs, envDecl, envDeclText)
	fs = injectFile(fs, macroDecl, macroDeclText)

	host := compiler.NewCompilerHost(dir, fs, bundled.LibPath(), nil, nil)
	parsed, diagnostics := tsoptions.GetParsedCommandLineOfConfigFile(configPath, nil, nil, host, nil)
	if len(diagnostics) > 0 || parsed == nil || parsed.ParsedConfig == nil {
		return nil, fmt.Errorf("%s has %d config diagnostics", configPath, len(diagnostics))
	}
	compile.ApplyAutomaticTypes(parsed.CompilerOptions())
	one := 1
	compile.ApplyCheckerOverride(parsed.CompilerOptions(), &one)
	if !declaresOnDisk(fs, parsed.ParsedConfig.FileNames, compile.EnvDeclFileName, "declare const $env") {
		parsed.ParsedConfig.FileNames = append(parsed.ParsedConfig.FileNames, envDecl)
	}
	if !declaresOnDisk(fs, parsed.ParsedConfig.FileNames, compile.MacroDeclFileName, "declare function $nameof") {
		parsed.ParsedConfig.FileNames = append(parsed.ParsedConfig.FileNames, macroDecl)
	}
	return compiler.NewProgram(compiler.ProgramOptions{Host: host, Config: parsed}), nil
}

// projectSourceFiles mirrors internal/compile's projectSourceFiles.
func projectSourceFiles(program *compiler.Program) []*ast.SourceFile {
	var files []*ast.SourceFile
	for _, file := range program.SourceFiles() {
		name := file.FileName()
		if file.IsDeclarationFile ||
			program.IsSourceFileFromExternalLibrary(file) ||
			program.IsSourceFromProjectReference(file.Path()) ||
			(!strings.HasSuffix(name, ".ts") && !strings.HasSuffix(name, ".tsx")) {
			continue
		}
		files = append(files, file)
	}
	return files
}

func walk(node *ast.Node, visit func(*ast.Node)) {
	visit(node)
	node.ForEachChild(func(child *ast.Node) bool {
		walk(child, visit)
		return false
	})
}

func dumped(node *ast.Node) bool {
	return ast.IsExpression(node) || ast.IsDeclaration(node)
}

// escape keeps each record on one line and its fields tab-separated.
func escape(text string) string {
	return strings.NewReplacer(`\`, `\\`, "\n", `\n`, "\r", `\r`, "\t", `\t`).Replace(text)
}

// symbolField prints a symbol as name,flags,declarations. Internal names
// (InternalSymbolNamePrefix, "\xFE" in tsgo and "\x7F" in tsrs) print as
// "#" and the rest of the name.
func symbolField(symbol *ast.Symbol) string {
	if symbol == nil {
		return "-"
	}
	name := symbol.Name
	if rest, internal := strings.CutPrefix(name, ast.InternalSymbolNamePrefix); internal {
		name = "#" + rest
	}
	return fmt.Sprintf("%s,%x,%d", escape(name), uint32(symbol.Flags), len(symbol.Declarations))
}

// objectFlags keeps the object flags that describe a type, in tsgo's layout:
// the kind and propagated flags, the object-only flags and FromTypeNode, the
// union's ContainsIntersections and the intersection's
// IsConstrainedTypeVariable. Memo bits (the Computed flags, MembersResolved,
// the identical-base-type cache) record which queries already ran, not what
// the type is, so they are left out.
func objectFlags(t *checker.Type) uint32 {
	flags := t.ObjectFlags()
	out := uint32(flags) & (1<<19 - 1)
	switch {
	case t.Flags()&checker.TypeFlagsObject != 0:
		out |= uint32(flags & (checker.ObjectFlagsContainsSpread | checker.ObjectFlagsObjectRestType |
			checker.ObjectFlagsInstantiationExpressionType | checker.ObjectFlagsSingleSignatureType |
			checker.ObjectFlagsIsClassInstanceClone | checker.ObjectFlagsFromTypeNode))
	case t.Flags()&checker.TypeFlagsUnion != 0:
		out |= uint32(flags & checker.ObjectFlagsContainsIntersections)
	case t.Flags()&checker.TypeFlagsIntersection != 0:
		out |= uint32(flags & checker.ObjectFlagsIsConstrainedTypeVariable)
	}
	return out
}

// guarded runs a query that typescript-go may panic on (getTypeAtLocation of
// some declarations whose symbol never bound) and prints "panic" instead.
func guarded(query func() string) (out string) {
	defer func() {
		if recover() != nil {
			out = "panic"
		}
	}()
	return query()
}

func queries(c *checker.Checker, node *ast.Node) string {
	return guarded(func() string { return unguardedQueries(c, node) })
}

func unguardedQueries(c *checker.Checker, node *ast.Node) string {
	t := c.GetTypeAtLocation(node)
	apparent := c.GetApparentType(t)
	return fmt.Sprintf("t=%x\to=%x\ts=%s\tsym=%s\ttsym=%s\tap=%d\tcs=%d\tks=%d",
		uint32(t.Flags()),
		objectFlags(t),
		escape(c.TypeToString(t)),
		symbolField(c.GetSymbolAtLocation(node)),
		symbolField(t.Symbol()),
		len(c.GetPropertiesOfType(apparent)),
		len(c.GetCallSignatures(t)),
		len(c.GetSignaturesOfType(t, checker.SignatureKindConstruct)),
	)
}

// typeChecks is the canonical predicate order; bit i of the d= and p= masks
// is typeChecks[i] under IsDefinitelyType and IsPossiblyType.
func typeChecks(s *transformer.State) []transformer.TypeCheck {
	return []transformer.TypeCheck{
		transformer.IsAnyType(s),
		transformer.IsBooleanType,
		transformer.IsBooleanLiteralType(s, false),
		transformer.IsBooleanLiteralType(s, true),
		transformer.IsNumberType,
		transformer.IsNumberLiteralType(jsnum.Number(0)),
		transformer.IsNaNType,
		transformer.IsStringType,
		transformer.IsObjectType,
		transformer.IsNonPrimitiveType,
		transformer.IsUndefinedType,
		transformer.IsEmptyStringType,
		transformer.IsArrayType(s),
		transformer.IsLuaTupleType(s),
		transformer.IsSetType(s),
		transformer.IsMapType(s),
		transformer.IsGeneratorType(s),
		transformer.IsIterableFunctionType(s),
		transformer.IsIterableType(s),
		transformer.IsIterableFunctionLuaTupleType(s),
		transformer.IsRobloxType(s),
	}
}

func bit(mask *uint32, index int, value bool) {
	if value {
		*mask |= 1 << index
	}
}

// firstConstructSymbol mirrors transformer.getFirstConstructSymbol, starting
// from the type instead of an expression.
func firstConstructSymbol(t *checker.Type) *ast.Symbol {
	symbol := t.Symbol()
	if symbol == nil {
		return nil
	}
	for _, declaration := range symbol.Declarations {
		if ast.IsInterfaceDeclaration(declaration) {
			for _, member := range declaration.AsInterfaceDeclaration().Members.Nodes {
				if ast.IsConstructSignatureDeclaration(member) {
					return member.Symbol()
				}
			}
		}
	}
	return nil
}

func predicates(s *transformer.State, node *ast.Node) string {
	return guarded(func() string { return unguardedPredicates(s, node) })
}

func unguardedPredicates(s *transformer.State, node *ast.Node) string {
	t := s.Checker.GetTypeAtLocation(node)
	var definitely, possibly, extra uint32
	for index, check := range typeChecks(s) {
		bit(&definitely, index, transformer.IsDefinitelyType(s, t, check))
		bit(&possibly, index, transformer.IsPossiblyType(s, t, check))
	}
	bit(&extra, 0, transformer.IsDefinitelyType(s, t, transformer.IsObjectType, transformer.IsNonPrimitiveType))
	bit(&extra, 1, transformer.IsDefinitelyType(s, t, transformer.IsStringType, transformer.IsNumberType))
	bit(&extra, 2, transformer.IsSharedTableType(s, t))
	leaves := 0
	transformer.WalkTypes(s, t, func(*checker.Type) { leaves++ })
	return fmt.Sprintf("d=%x\tp=%x\tx=%x\tw=%d\tfds=%s\tfcs=%s",
		definitely, possibly, extra, leaves,
		symbolField(transformer.GetFirstDefinedSymbol(s, t)),
		symbolField(firstConstructSymbol(t)),
	)
}

type record struct {
	key     string
	queries string
}

func run(configPath string, out io.Writer) error {
	program, err := loadProgram(configPath)
	if err != nil {
		return err
	}
	ctx := context.Background()
	// rotor checks every file before it transforms any (the precheck pass), so
	// the walk sees a fully checked program.
	program.GetSemanticDiagnostics(ctx, nil)

	dir := tspath.GetDirectoryPath(configPath)
	files := projectSourceFiles(program)
	perFile := make([][]record, len(files))
	for index, file := range files {
		c, done := program.GetTypeCheckerForFile(ctx, file)
		relative := tspath.GetRelativePathFromDirectory(dir, file.FileName(), tspath.ComparePathsOptions{})
		walk(file.AsNode(), func(node *ast.Node) {
			if !dumped(node) {
				return
			}
			key := relative + ":" + strconv.Itoa(node.Pos()) + ":" + strconv.Itoa(node.End()) + ":" +
				strings.TrimPrefix(node.Kind.String(), "Kind")
			perFile[index] = append(perFile[index], record{key, queries(c, node)})
		})
		done()
	}

	// The predicates run in a second pass, after every type the first pass
	// prints exists: the MacroManager behind State resolves its symbols in Go
	// map order, which must not shift the type ids typeToString orders by.
	// Module symbols and import types name files by absolute path; the dump
	// names them relative to the project so it is the same on every machine.
	portable := strings.NewReplacer(dir, "<project>")
	writer := bufio.NewWriter(out)
	multi := transformer.NewMultiState()
	for index, file := range files {
		c, done := program.GetTypeCheckerForFile(ctx, file)
		s := transformer.NewState(program, c, file, transformer.NewDiagService(), multi)
		position := 0
		walk(file.AsNode(), func(node *ast.Node) {
			if !dumped(node) {
				return
			}
			entry := perFile[index][position]
			position++
			fmt.Fprintf(writer, "%s\t%s\t%s\n", entry.key, portable.Replace(entry.queries), portable.Replace(predicates(s, node)))
		})
		done()
	}
	return writer.Flush()
}

// write runs the dump into path, or stdout when path is empty, and reports a
// failed close: the dump is only whole once the file is.
func write(configPath, path string) (err error) {
	if path == "" {
		return run(configPath, os.Stdout)
	}
	file, err := os.Create(path)
	if err != nil {
		return err
	}
	defer func() {
		if closeErr := file.Close(); err == nil {
			err = closeErr
		}
	}()
	return run(configPath, file)
}

func main() {
	project := flag.String("project", "", "tsconfig to load")
	output := flag.String("out", "", "file to write (default: stdout)")
	flag.Parse()
	if *project == "" {
		fmt.Fprintln(os.Stderr, "usage: tsrs-oracle -project <tsconfig> [-out <file>]")
		os.Exit(2)
	}
	configPath, err := filepath.Abs(*project)
	if err == nil {
		err = write(filepath.ToSlash(configPath), *output)
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
