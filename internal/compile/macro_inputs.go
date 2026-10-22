package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"slices"
	"strings"

	"rotor/internal/dotenv"
	"rotor/internal/transformer"
	"rotor/tsgo/ast"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	"rotor/tsgo/parser"
	"rotor/tsgo/tspath"
)

// macroInputs records conservative dependencies before type checking. Local
// bindings with the same names may add an unnecessary input, but never hide a
// real macro input. Values enter only the hashed incremental salt.
type macroInputs struct {
	files  map[string]struct{}
	values map[string]string
}

func collectMacroInputs(dir string, sources []*ast.SourceFile) macroInputs {
	inputs := macroInputs{files: map[string]struct{}{}, values: map[string]string{}}
	env := dotenv.Load(dir)
	resolver := newFileResolver(dir)
	recordEnv := func(key string) {
		value, ok := env.Lookup(key)
		if ok {
			inputs.values["env:"+key] = "set:" + value
		} else {
			inputs.values["env:"+key] = "unset"
		}
	}
	for _, source := range sources {
		if source.IsDeclarationFile || (!strings.Contains(source.Text(), "$env") && !strings.Contains(source.Text(), "$file")) {
			continue
		}
		usesEnv := false
		var visit func(*ast.Node) bool
		visit = func(node *ast.Node) bool {
			switch node.Kind {
			case ast.KindCallExpression:
				callee := node.Expression()
				args := node.Arguments()
				if callee != nil {
					callee = transformer.SkipDownwards(callee)
				}
				if callee != nil && ast.IsIdentifier(callee) && len(args) > 0 {
					argument := transformer.SkipDownwards(args[0])
					if !ast.IsStringLiteralLike(argument) {
						break
					}
					switch callee.Text() {
					case "$env":
						usesEnv = true
						recordEnv(argument.Text())
					case "$file":
						path, err := resolver.resolvePath(source.FileName(), argument.Text())
						if err == nil {
							inputs.files[path] = struct{}{}
							data, err := resolver.Read(source.FileName(), argument.Text())
							value := "unreadable"
							if err == nil {
								sum := sha256.Sum256(data)
								value = hex.EncodeToString(sum[:])
							}
							inputs.values["file:"+relativeIncrementalPath(dir, path)] = value
						}
					}
				}
			case ast.KindPropertyAccessExpression, ast.KindElementAccessExpression:
				base := transformer.SkipDownwards(node.Expression())
				if base != nil && ast.IsIdentifier(base) && base.Text() == "$env" {
					usesEnv = true
					if node.Kind == ast.KindPropertyAccessExpression {
						recordEnv(node.Name().Text())
					} else if arg := node.AsElementAccessExpression().ArgumentExpression; arg != nil {
						arg = transformer.SkipDownwards(arg)
						if ast.IsStringLiteralLike(arg) {
							recordEnv(arg.Text())
						}
					}
				}
			}
			node.ForEachChild(visit)
			return false
		}
		source.AsNode().ForEachChild(visit)
		if usesEnv {
			for _, path := range env.Files() {
				inputs.files[path] = struct{}{}
			}
		}
	}
	return inputs
}

func macroInputValues(dir string, program *compiler.Program) map[string]string {
	if configPath := program.Options().ConfigFilePath; configPath != "" {
		dir = filepath.Dir(filepath.FromSlash(configPath))
	}
	return collectMacroInputs(dir, projectSourceFiles(program)).values
}

// MacroWatchFiles includes missing inputs, so creating an environment or data
// file recovers a failed build. It does not instantiate a checker or plugins.
func MacroWatchFiles(configPath string) []string {
	config, _, err := parseProjectConfig(configPath)
	if err != nil {
		return nil
	}
	var sources []*ast.SourceFile
	for _, name := range config.FileNames() {
		if tspath.IsDeclarationFileName(name) {
			continue
		}
		data, err := os.ReadFile(filepath.FromSlash(name))
		if err != nil || (!strings.Contains(string(data), "$env") && !strings.Contains(string(data), "$file")) {
			continue
		}
		sources = append(sources, parser.ParseSourceFile(ast.SourceFileParseOptions{FileName: name, Path: tspath.ToPath(name, config.GetCurrentDirectory(), config.UseCaseSensitiveFileNames())}, string(data), core.GetScriptKindFromFileName(name)))
	}
	return MacroInputFiles(config.GetCurrentDirectory(), sources)
}

// MacroInputFiles returns watch inputs from sources already loaded by a caller.
func MacroInputFiles(dir string, sources []*ast.SourceFile) []string {
	inputs := collectMacroInputs(dir, sources)
	paths := make([]string, 0, len(inputs.files))
	for path := range inputs.files {
		paths = append(paths, path)
	}
	slices.Sort(paths)
	return paths
}
