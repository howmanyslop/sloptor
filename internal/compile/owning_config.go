package compile

import (
	"errors"
	"fmt"
	"path/filepath"
	"runtime"
	"strings"

	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/vfs/osvfs"
)

// ErrNoOwningConfig means a solution config references no project that
// includes the file.
var ErrNoOwningConfig = errors.New("compile: no referenced project includes this file")

// OwningConfig resolves which tsconfig compiles fileName, starting from the
// nearest one. A config that is not a solution coordinator owns the file
// itself, matching `sloptor diagnostics --project <it>`. A coordinator (no
// files of its own, only references) owns nothing, so its references are
// searched, depth-first and in order, for the first project whose file set
// includes fileName.
func OwningConfig(tsConfigPath, fileName string) (string, error) {
	return owningConfig(tsConfigPath, fileName, map[string]struct{}{})
}

func owningConfig(tsConfigPath, fileName string, visited map[string]struct{}) (string, error) {
	key := configKey(tsConfigPath)
	if _, seen := visited[key]; seen {
		return "", ErrNoOwningConfig
	}
	visited[key] = struct{}{}

	references, coordinator, _, err := readProjectReferencePaths(tsConfigPath)
	if err != nil {
		return "", err
	}
	if !coordinator {
		return tsConfigPath, nil
	}
	for _, reference := range references {
		if !dirContainsFile(filepath.Dir(reference), fileName) {
			// A project's files live under its own directory, and parsing a
			// config walks its whole tree, so skip the ones that cannot hold
			// this file.
			continue
		}
		_, referenceCoordinator, _, err := readProjectReferencePaths(reference)
		if err != nil {
			continue
		}
		if referenceCoordinator {
			if owner, err := owningConfig(reference, fileName, visited); err == nil {
				return owner, nil
			}
			continue
		}
		included, err := configIncludesFile(reference, fileName)
		if err != nil || !included {
			continue
		}
		return reference, nil
	}
	return "", fmt.Errorf("%w: %s", ErrNoOwningConfig, fileName)
}

func configIncludesFile(configPath, fileName string) (bool, error) {
	host := compiler.NewCompilerHost(filepath.Dir(configPath), osvfs.FS(), bundled.LibPath(), nil, nil)
	parsed, diagnostics := tsoptions.GetParsedCommandLineOfConfigFile(filepath.ToSlash(configPath), nil, nil, host, nil)
	if parsed == nil || parsed.ParsedConfig == nil {
		if len(diagnostics) > 0 {
			return false, errors.New("compile: tsconfig has errors")
		}
		return false, errors.New("compile: tsconfig did not parse")
	}
	want := configKey(fileName)
	for _, name := range parsed.ParsedConfig.FileNames {
		if configKey(normalizeSourceFilePath(name)) == want {
			return true, nil
		}
	}
	return false, nil
}

func configKey(path string) string {
	path = filepath.Clean(filepath.FromSlash(path))
	if runtime.GOOS == "windows" {
		return strings.ToLower(path)
	}
	return path
}

func dirContainsFile(dir, path string) bool {
	rel, err := filepath.Rel(configKey(dir), configKey(path))
	return err == nil && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator))
}
