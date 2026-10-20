package rojo

import (
	"os"
	"path/filepath"
	"sort"
)

// ProjectFiles is a build-local snapshot of project contents. Package resolver
// discovery and declaration-to-runtime remapping must read the same contents.
// Construct a new snapshot for each build, including incremental rebuilds.
type ProjectFiles struct {
	configs map[string]jsonValue
}

func NewProjectFiles() *ProjectFiles {
	return &ProjectFiles{configs: make(map[string]jsonValue)}
}

func (f *ProjectFiles) read(path string) jsonValue {
	path = filepath.Clean(realpathOr(path))
	if config, ok := f.configs[path]; ok {
		return config
	}
	config := jsonValue{kind: jsonInvalid}
	if contents, err := os.ReadFile(path); err == nil {
		config = parseJSON(contents)
	}
	f.configs[path] = config
	return config
}

// RootPath returns a validated project's string tree.$path. The optional
// object form deliberately does not participate in runtime-path remapping.
func (f *ProjectFiles) RootPath(path string) (string, bool) {
	config := f.read(path)
	if validateConfig(config) != "" {
		return "", false
	}
	tree, _ := config.member("tree")
	rootPath, _ := tree.member("$path")
	return rootPath.str, rootPath.kind == jsonString
}

// Paths includes absent candidates so watch mode can observe their creation.
func (f *ProjectFiles) Paths() []string {
	paths := make([]string, 0, len(f.configs))
	for path := range f.configs {
		paths = append(paths, path)
	}
	sort.Strings(paths)
	return paths
}
