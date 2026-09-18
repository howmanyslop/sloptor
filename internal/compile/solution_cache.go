package compile

import (
	"path/filepath"
	"sync"
	"sync/atomic"

	"rotor/internal/rojo"
	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/packagejson"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs"
	"rotor/tsgo/vfs/cachedvfs"
	"rotor/tsgo/vfs/osvfs"
)

// solutionCompileCache is drain-scoped: one per SolutionCoordinator.Drain.
// It shares OS metadata (cachedvfs), parsed .d.ts/.json SourceFiles, Rojo
// resolvers, and package.json metadata across referenced projects. Ordinary
// .ts/.tsx files and module-resolution results stay project-local.
type solutionCompileCache struct {
	fs                 vfs.FS
	packageJSON        *packagejson.InfoCache
	mu                 sync.Mutex
	sources            map[ast.SourceFileParseOptions]*ast.SourceFile
	inflight           map[ast.SourceFileParseOptions]chan struct{}
	rojoCaches         map[string]*rojo.RojoResolverCache
	rojoSnapshots      map[string]*rojo.ResolverSnapshot
	rojoInflight       map[string]chan struct{}
	syntheticResolvers map[string]*rojo.RojoResolver
	hits               atomic.Int64
	misses             atomic.Int64
}

func newSolutionCompileCache() *solutionCompileCache {
	return &solutionCompileCache{
		fs:                 cachedvfs.From(osvfs.FS()),
		packageJSON:        packagejson.NewInfoCache("/", osvfs.FS().UseCaseSensitiveFileNames()),
		sources:            map[ast.SourceFileParseOptions]*ast.SourceFile{},
		inflight:           map[ast.SourceFileParseOptions]chan struct{}{},
		rojoCaches:         map[string]*rojo.RojoResolverCache{},
		rojoSnapshots:      map[string]*rojo.ResolverSnapshot{},
		rojoInflight:       map[string]chan struct{}{},
		syntheticResolvers: map[string]*rojo.RojoResolver{},
	}
}

func (c *solutionCompileCache) rojoResolver(configPath, cacheDir, compilerVersion string, deferPersist bool) (*rojo.RojoResolverCache, *rojo.RojoResolver) {
	configKey := filepath.Clean(configPath)
	ownerKey := configKey + "\x00" + filepath.Clean(cacheDir)
	for {
		c.mu.Lock()
		cache := c.rojoCaches[ownerKey]
		if cache == nil {
			cache = rojo.NewRojoResolverCacheWithDeferredPersist(cacheDir, compilerVersion, deferPersist)
			c.rojoCaches[ownerKey] = cache
		}
		snapshot := c.rojoSnapshots[configKey]
		if snapshot != nil {
			c.mu.Unlock()
			if resolver, ok := cache.AdoptSnapshot(configPath, snapshot); ok {
				return cache, resolver
			}
			c.mu.Lock()
			if c.rojoSnapshots[configKey] == snapshot {
				delete(c.rojoSnapshots, configKey)
			}
			c.mu.Unlock()
			continue
		}
		if wait := c.rojoInflight[configKey]; wait != nil {
			c.mu.Unlock()
			<-wait
			continue
		}
		wait := make(chan struct{})
		c.rojoInflight[configKey] = wait
		c.mu.Unlock()

		snapshot, resolver := cache.LoadSnapshot(configPath)

		c.mu.Lock()
		if snapshot != nil {
			c.rojoSnapshots[configKey] = snapshot
		}
		delete(c.rojoInflight, configKey)
		close(wait)
		c.mu.Unlock()
		return cache, resolver
	}
}

func (c *solutionCompileCache) syntheticRojoResolver(basePath string) *rojo.RojoResolver {
	key := filepath.Clean(basePath)
	c.mu.Lock()
	defer c.mu.Unlock()
	if resolver, ok := c.syntheticResolvers[key]; ok {
		return resolver
	}
	resolver := rojo.Synthetic(basePath)
	c.syntheticResolvers[key] = resolver
	return resolver
}

func (c *solutionCompileCache) wrap(host compiler.CompilerHost, skip map[string]struct{}) compiler.CompilerHost {
	if c == nil {
		return host
	}
	return &cachingCompilerHost{CompilerHost: host, cache: c, skip: skip}
}

func (c *solutionCompileCache) getSourceFile(opts ast.SourceFileParseOptions, parse func(ast.SourceFileParseOptions) *ast.SourceFile) *ast.SourceFile {
	c.mu.Lock()
	if file, ok := c.sources[opts]; ok {
		c.mu.Unlock()
		c.hits.Add(1)
		return file
	}
	if wait, ok := c.inflight[opts]; ok {
		c.mu.Unlock()
		<-wait
		c.mu.Lock()
		file := c.sources[opts]
		c.mu.Unlock()
		c.hits.Add(1)
		return file
	}
	wait := make(chan struct{})
	c.inflight[opts] = wait
	c.mu.Unlock()

	file := parse(opts)

	c.mu.Lock()
	if file != nil {
		c.sources[opts] = file
	}
	delete(c.inflight, opts)
	close(wait)
	c.mu.Unlock()
	c.misses.Add(1)
	return file
}

type cachingCompilerHost struct {
	compiler.CompilerHost
	cache *solutionCompileCache
	skip  map[string]struct{}
}

func (h *cachingCompilerHost) GetSourceFile(opts ast.SourceFileParseOptions) *ast.SourceFile {
	if h.skipFile(opts.FileName) || !cacheableParseName(opts.FileName) {
		return h.CompilerHost.GetSourceFile(opts)
	}
	return h.cache.getSourceFile(opts, h.CompilerHost.GetSourceFile)
}

func (h *cachingCompilerHost) skipFile(fileName string) bool {
	if len(h.skip) == 0 {
		return false
	}
	_, ok := h.skip[normalizeSourceFilePath(fileName)]
	return ok
}

func cacheableParseName(fileName string) bool {
	return tspath.IsDeclarationFileName(fileName) || tspath.FileExtensionIs(fileName, tspath.ExtensionJson)
}

func solutionCacheFS(cache *solutionCompileCache, configPath string, overlays map[string]string) vfs.FS {
	base := osvfs.FS()
	if cache != nil {
		base = cache.fs
	}
	if len(overlays) > 0 {
		return newOverlayFS(base, configPath, overlays)
	}
	fs := SanitizeFSWithConfigPath(bundled.WrapFS(base), configPath)
	if cache == nil {
		return cachedvfs.From(fs)
	}
	return fs
}

func syntheticDeclSkip(configPath string) map[string]struct{} {
	return map[string]struct{}{
		normalizeSourceFilePath(envDeclPath(configPath)):   {},
		normalizeSourceFilePath(assetDeclPath(configPath)): {},
		normalizeSourceFilePath(macroDeclPath(configPath)): {},
	}
}
