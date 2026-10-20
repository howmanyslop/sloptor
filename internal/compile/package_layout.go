package compile

import (
	"crypto/sha256"
	"encoding/hex"
	"path/filepath"
	"strings"

	"rotor/internal/rojo"
	"rotor/internal/transformer"
	"rotor/tsgo/core"
	tsjson "rotor/tsgo/json"
	"rotor/tsgo/vfs/osvfs"
)

// packageLayout is one build's package import inputs, shared by both salt
// computations and emission. It must not survive into the next build.
type packageLayout struct {
	resolvers []*rojo.RojoResolver
	mapping   transformer.NodeModulesPathMapping
	files     *rojo.ProjectFiles
	manifests []string
}

func isPackageBuild(dir string, opts ProjectOptions) (bool, error) {
	if opts.Type != "" {
		return opts.Type == transformer.ProjectTypePackage, nil
	}
	_, isPackage, err := projectIsPackage(dir)
	return isPackage, err
}

func preparePackageLayout(dir string, options *core.CompilerOptions, opts ProjectOptions) (*packageLayout, error) {
	if opts.EmitDeclarationOnly {
		return nil, nil
	}
	isPackage, err := isPackageBuild(dir, opts)
	if err != nil || !isPackage {
		return nil, err
	}
	layout := &packageLayout{files: rojo.NewProjectFiles()}
	for _, typeRoot := range options.TypeRoots {
		layout.resolvers = append(layout.resolvers, rojo.PackageImports(filepath.FromSlash(typeRoot), layout.files))
	}
	layout.mapping, layout.manifests = buildNodeModulesPathMapping(options.TypeRoots, osvfs.FS().UseCaseSensitiveFileNames(), options.CustomConditions,
		func(pkgDir, runtimePath string) string {
			if rootPath, ok := layout.files.RootPath(filepath.Join(pkgDir, "default.project.json")); ok {
				return remapRuntimePath(pkgDir, runtimePath, rootPath)
			}
			return runtimePath
		})
	return layout, nil
}

func (p *packageLayout) fingerprint(dir string) (string, error) {
	if p == nil {
		return "", nil
	}
	relative := func(path string) string {
		if rel, err := filepath.Rel(filepath.FromSlash(dir), filepath.FromSlash(path)); err == nil {
			// A wildcard's prefix can end in a separator; preserve it so
			// out/* and out* cannot share an incremental fingerprint.
			if strings.HasSuffix(path, "/") || strings.HasSuffix(path, string(filepath.Separator)) {
				rel += string(filepath.Separator)
			}
			return filepath.ToSlash(rel)
		}
		return filepath.ToSlash(path)
	}
	type resolverMapping struct {
		Partitions []rojo.PartitionInfo         `json:"partitions"`
		FilePaths  []rojo.ResolverFilePathState `json:"filePaths"`
	}
	resolvers := make([]resolverMapping, 0, len(p.resolvers))
	for _, resolver := range p.resolvers {
		state := resolver.GetState()
		for i := range state.Partitions {
			state.Partitions[i].FsPath = relative(state.Partitions[i].FsPath)
		}
		for i := range state.FilePaths {
			state.FilePaths[i].Path = relative(state.FilePaths[i].Path)
		}
		resolvers = append(resolvers, resolverMapping{state.Partitions, state.FilePaths})
	}
	mapping := transformer.NodeModulesPathMapping{Exact: make(map[string]string)}
	for key, value := range p.mapping.Exact {
		mapping.Exact[relative(key)] = relative(value)
	}
	for _, pattern := range p.mapping.Patterns {
		pattern.TypesPrefix = relative(pattern.TypesPrefix)
		pattern.Runtime = relative(pattern.Runtime)
		mapping.Patterns = append(mapping.Patterns, pattern)
	}
	payload, err := tsjson.Marshal(struct {
		Policy    string                             `json:"policy"`
		Resolvers []resolverMapping                  `json:"resolvers"`
		Mapping   transformer.NodeModulesPathMapping `json:"mapping"`
	}{"package-default-projects-v1", resolvers, mapping}, tsjson.Deterministic(true))
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(payload)
	return hex.EncodeToString(sum[:]), nil
}
