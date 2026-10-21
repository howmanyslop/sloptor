package compile

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"

	"rotor/internal/jecs"
	"rotor/internal/logservice"
	"rotor/tsgo/ast"
	"rotor/tsgo/checker"
	"rotor/tsgo/compiler"
)

const (
	nativeJecsTransform = "rbxts-transformer-jecs"
	nativeJecsEnv       = "ROTOR_NATIVE_JECS"
)

// nativeJecsSourceHashes are the installed transformers internal/jecs is a
// port of, hashed by hashJecsTransformerSources. Any other build of the
// package, including a re-patched one, keeps running in the sidecar.
var nativeJecsSourceHashes = map[string]bool{
	// rbxts-transformer-jecs 0.0.11 with patches/rbxts-transformer-jecs@0.0.11.patch
	"79f5ae9346aaa20019f1147dcd103ec16fc6df3a1a622f774780efbf4293a3fd": true,
}

var jecsSourceHashCache sync.Map // package directory -> hash or ""

// nativeJecsStage splits a transformer stage into a leading native jecs pass
// and the plugins left for the sidecar. It declines (nil config) whenever
// running jecs first could change what the remaining plugins see: jecs must
// be the stage's first `before` transformer, configured only with options the
// port understands, and installed as a build the port matches.
func nativeJecsStage(dir string, plugins []transformerPluginConfig) (*jecs.Config, []transformerPluginConfig) {
	if os.Getenv(nativeJecsEnv) == "0" {
		return nil, plugins
	}
	for index, plugin := range plugins {
		var fields map[string]json.RawMessage
		if err := json.Unmarshal(plugin.marshalJSON(), &fields); err != nil {
			return nil, plugins
		}
		if isTruthyJSON(fields["after"]) || isTruthyJSON(fields["afterDeclarations"]) {
			continue
		}
		if plugin.Transform != nativeJecsTransform {
			return nil, plugins
		}
		config := jecs.Config{}
		for key, value := range fields {
			switch key {
			case "transform", "after", "afterDeclarations":
			case "silent":
				if err := json.Unmarshal(value, &config.Silent); err != nil {
					return nil, plugins
				}
			case "jecsPackage":
				if err := json.Unmarshal(value, &config.JecsPackage); err != nil {
					return nil, plugins
				}
			default:
				return nil, plugins
			}
		}
		if !nativeJecsSourceHashes[installedJecsSourceHash(dir)] {
			return nil, plugins
		}
		rest := slices.Delete(slices.Clone(plugins), index, index+1)
		return &config, rest
	}
	return nil, plugins
}

// usesSubprocessSidecar reports whether host ends in the CLI's Node worker.
// API callback hosts run plugins in the caller's own process, which keeps
// every plugin there.
func usesSubprocessSidecar(host transformerHost) bool {
	for {
		switch wrapped := host.(type) {
		case sidecarTransformerHost:
			return true
		case prefetchingTransformerHost:
			host = wrapped.transformerHost
		default:
			return false
		}
	}
}

func isTruthyJSON(raw json.RawMessage) bool {
	if len(raw) == 0 {
		return false
	}
	switch strings.TrimSpace(string(raw)) {
	case "false", "null", "0", `""`:
		return false
	}
	return true
}

// installedJecsSourceHash hashes the transformer the sidecar would load for a
// project in dir, found the way Node resolves it: the nearest node_modules.
func installedJecsSourceHash(dir string) string {
	for current := filepath.Clean(filepath.FromSlash(dir)); ; current = filepath.Dir(current) {
		packageDir := filepath.Join(current, "node_modules", nativeJecsTransform)
		if info, err := os.Stat(packageDir); err == nil && info.IsDir() {
			if cached, ok := jecsSourceHashCache.Load(packageDir); ok {
				return cached.(string)
			}
			hash, err := hashJecsTransformerSources(filepath.Join(packageDir, "out", "src"))
			if err != nil {
				hash = ""
			}
			jecsSourceHashCache.Store(packageDir, hash)
			return hash
		}
		if filepath.Dir(current) == current {
			return ""
		}
	}
}

// hashJecsTransformerSources is sha256 over every .js file under root in path
// order, each framed as "<relative path>\0<contents>\0".
func hashJecsTransformerSources(root string) (string, error) {
	var files []string
	err := filepath.WalkDir(root, func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if !entry.IsDir() && strings.HasSuffix(path, ".js") {
			relative, err := filepath.Rel(root, path)
			if err != nil {
				return err
			}
			files = append(files, filepath.ToSlash(relative))
		}
		return nil
	})
	if err != nil {
		return "", err
	}
	slices.Sort(files)
	hash := sha256.New()
	for _, file := range files {
		contents, err := os.ReadFile(filepath.Join(root, filepath.FromSlash(file)))
		if err != nil {
			return "", err
		}
		hash.Write([]byte(file))
		hash.Write([]byte{0})
		hash.Write(contents)
		hash.Write([]byte{0})
	}
	return hex.EncodeToString(hash.Sum(nil)), nil
}

// applyNativeJecsTransform runs jecs over sourceFiles and overlays the files
// it changed back into the program, the way the native Flamework stage does.
// The files that pass the transformer's text prefilter are split across the
// program's checkers: each file's transform reads only its own syntax and the
// checker's answers, so every checker can take a share with its own state.
func applyNativeJecsTransform(ctx context.Context, program *compiler.Program, sourceFiles []*ast.SourceFile, traces diagnosticTraces, config jecs.Config) (*preparedTransformerProgram, error) {
	configPath := program.Options().ConfigFilePath
	stop := logStageNamed(configPath, "native jecs transform")
	defer stop()
	var candidates []int
	for index, sourceFile := range sourceFiles {
		if !sourceFile.IsDeclarationFile && jecs.NeedsTransform(sourceFile.Text()) {
			candidates = append(candidates, index)
		}
	}
	overlays := make([]*nativeSourceOverlay, len(sourceFiles))
	transformGroup := func(c *checker.Checker, group []int) error {
		files := make([]*ast.SourceFile, len(group))
		for position, index := range group {
			files[position] = sourceFiles[index]
		}
		result, err := jecs.Transform(jecs.Input{Program: program, Checker: c, Files: files, Config: config, Warn: logservice.Warn})
		if err != nil {
			return fmt.Errorf("compile: native jecs transform: %w", err)
		}
		for position, changed := range result.Changed {
			if !changed {
				continue
			}
			original := files[position]
			text, trace, err := printNativeSource(result.Files[position], original, result.EmitContext, original.FileName(), original.Text())
			if err != nil {
				return err
			}
			overlays[group[position]] = &nativeSourceOverlay{fileName: original.FileName(), text: text, trace: trace}
		}
		return nil
	}

	var groups [][]int
	var errs []error
	if len(candidates) > 1 {
		var mu sync.Mutex
		program.ForEachCheckerParallel(func(index int, _ *checker.Checker) {
			mu.Lock()
			for len(groups) <= index {
				groups = append(groups, nil)
				errs = append(errs, nil)
			}
			mu.Unlock()
		})
	}
	if len(groups) > 1 {
		for position, index := range candidates {
			groups[position%len(groups)] = append(groups[position%len(groups)], index)
		}
		program.ForEachCheckerParallel(func(index int, c *checker.Checker) {
			if len(groups[index]) > 0 {
				errs[index] = transformGroup(c, groups[index])
			}
		})
	} else if len(candidates) > 0 {
		c, release := program.GetTypeChecker(ctx)
		errs = append(errs, transformGroup(c, candidates))
		release()
	}
	for _, err := range errs {
		if err != nil {
			return nil, err
		}
	}

	var changed []nativeSourceOverlay
	for _, overlay := range overlays {
		if overlay != nil {
			changed = append(changed, *overlay)
		}
	}
	transformedProgram, remapped, composed, err := updateNativeFlameworkProgram(nativeProgramUpdate{
		program: program, sourceFiles: sourceFiles, traces: traces, overlays: changed,
	})
	if err != nil {
		return nil, err
	}
	if transformedProgram == nil {
		return nil, errors.New("compile: native jecs program update returned nil")
	}
	return &preparedTransformerProgram{program: transformedProgram, sourceFiles: remapped, sourceTraces: composed}, nil
}
