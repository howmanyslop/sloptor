package compile

import (
	"sync"
	"sync/atomic"

	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tspath"
	"rotor/tsgo/vfs"
	"rotor/tsgo/vfs/cachedvfs"
	"rotor/tsgo/vfs/osvfs"
)

// solutionCompileCache is drain-scoped: one per SolutionCoordinator.Drain.
// It shares OS metadata (cachedvfs) and parsed .d.ts/.json SourceFiles across
// referenced projects. Ordinary .ts/.tsx files stay project-local.
type solutionCompileCache struct {
	fs       vfs.FS
	mu       sync.Mutex
	sources  map[ast.SourceFileParseOptions]*ast.SourceFile
	inflight map[ast.SourceFileParseOptions]chan struct{}
	hits     atomic.Int64
	misses   atomic.Int64
}

func newSolutionCompileCache() *solutionCompileCache {
	return &solutionCompileCache{
		fs:       cachedvfs.From(osvfs.FS()),
		sources:  map[ast.SourceFileParseOptions]*ast.SourceFile{},
		inflight: map[ast.SourceFileParseOptions]chan struct{}{},
	}
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
	return solutionCacheFSWithConfigRead(cache, configPath, overlays, nil)
}

func solutionCacheFSWithConfigRead(cache *solutionCompileCache, configPath string, overlays map[string]string, onConfigRead func(string, string)) vfs.FS {
	base := osvfs.FS()
	if cache != nil {
		base = cache.fs
	}
	if len(overlays) > 0 {
		return newOverlayFSWithConfigRead(base, configPath, overlays, onConfigRead)
	}
	fs := SanitizeFSWithConfigRead(bundled.WrapFS(base), configPath, onConfigRead)
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
