package rojo

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
)

const resolverCacheFormatVersion = 4

// resolverCacheManifestEntry is one path the resolver read. Missing marks a
// partition path that did not exist; creating it changes the resolver (a new
// directory is walked), so the entry stays valid only while it is absent.
// Listing is a walked directory's entries that the resolver reads, so a new
// mtime from writing other files (such as build outputs) keeps the entry.
// Sha256 is a project file's contents: package managers extract tarballs with
// fixed mtimes, so an upgraded package's project file can keep its mtime.
type resolverCacheManifestEntry struct {
	Path          string `json:"path"`
	MtimeUnixNano int64  `json:"mtimeUnixNano"`
	Missing       bool   `json:"missing,omitempty"`
	Listing       string `json:"listing,omitempty"`
	Sha256        string `json:"sha256,omitempty"`
}

type resolverCacheFile struct {
	Version         int                          `json:"version"`
	CompilerVersion string                       `json:"compilerVersion"`
	Key             string                       `json:"key"`
	State           ResolverState                `json:"state"`
	MtimeManifest   []resolverCacheManifestEntry `json:"mtimeManifest"`
}

type resolverCacheEntry struct {
	Resolver *RojoResolver
	Manifest []resolverCacheManifestEntry
}

// RojoResolverCache keeps parsed resolver states in memory and on disk.
type RojoResolverCache struct {
	cacheDir        string
	compilerVersion string
	deferPersist    bool

	mu      sync.Mutex
	l1      map[string]resolverCacheEntry
	pending map[string]resolverCacheEntry
}

// NewRojoResolverCache creates a cache whose disk entries are stored in cacheDir.
func NewRojoResolverCache(cacheDir, compilerVersion string) *RojoResolverCache {
	return NewRojoResolverCacheWithDeferredPersist(cacheDir, compilerVersion, false)
}

// NewMemoryRojoResolverCache creates a cache that never reads or writes disk.
func NewMemoryRojoResolverCache() *RojoResolverCache {
	return NewRojoResolverCacheWithDeferredPersist("", "", false)
}

func NewRojoResolverCacheWithDeferredPersist(cacheDir, compilerVersion string, deferPersist bool) *RojoResolverCache {
	return &RojoResolverCache{
		cacheDir:        cacheDir,
		compilerVersion: compilerVersion,
		deferPersist:    deferPersist,
		l1:              make(map[string]resolverCacheEntry),
		pending:         map[string]resolverCacheEntry{},
	}
}

// Load returns a resolver for rojoConfigFilePath, preferring valid L1 then L2 state.
func (c *RojoResolverCache) Load(rojoConfigFilePath string) *RojoResolver {
	key := cacheKey(rojoConfigFilePath)
	c.mu.Lock()
	defer c.mu.Unlock()

	if entry, ok := c.l1[key]; ok && manifestStillValid(entry.Manifest) {
		return entry.Resolver
	}
	delete(c.l1, key)

	if resolver, manifest, ok := c.loadDisk(key); ok {
		c.l1[key] = resolverCacheEntry{Resolver: resolver, Manifest: manifest}
		return resolver
	}

	resolver := FromPath(key)
	manifest, ok := resolverMtimeManifest(resolver.GetState())
	if !ok {
		return resolver
	}
	c.l1[key] = resolverCacheEntry{Resolver: resolver, Manifest: manifest}
	entry := c.l1[key]
	if c.deferPersist {
		c.pending[key] = entry
	} else {
		c.writeDisk(key, resolver.GetState(), manifest)
	}
	return resolver
}

func (c *RojoResolverCache) Persist() {
	c.mu.Lock()
	defer c.mu.Unlock()
	for key, entry := range c.pending {
		c.writeDisk(key, entry.Resolver.GetState(), entry.Manifest)
	}
	c.pending = map[string]resolverCacheEntry{}
}

func (c *RojoResolverCache) loadDisk(key string) (*RojoResolver, []resolverCacheManifestEntry, bool) {
	if c.cacheDir == "" {
		return nil, nil, false
	}
	path := c.cachePath(key)
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, nil, false
	}

	var file resolverCacheFile
	if err := json.Unmarshal(data, &file); err != nil {
		if !c.deferPersist {
			_ = os.Remove(path)
		}
		return nil, nil, false
	}
	if file.Version != resolverCacheFormatVersion || file.CompilerVersion != c.compilerVersion || file.Key != key ||
		!stateMatchesManifest(file.State, file.MtimeManifest) {
		if !c.deferPersist {
			_ = os.Remove(path)
		}
		return nil, nil, false
	}
	if !manifestStillValid(file.MtimeManifest) {
		return nil, nil, false
	}
	resolver, err := FromState(file.State)
	if err != nil {
		if !c.deferPersist {
			_ = os.Remove(path)
		}
		return nil, nil, false
	}
	return resolver, slices.Clone(file.MtimeManifest), true
}

func (c *RojoResolverCache) writeDisk(key string, state ResolverState, manifest []resolverCacheManifestEntry) {
	if c.cacheDir == "" {
		return
	}
	data, err := json.MarshalIndent(resolverCacheFile{
		Version:         resolverCacheFormatVersion,
		CompilerVersion: c.compilerVersion,
		Key:             key,
		State:           state,
		MtimeManifest:   manifest,
	}, "", "\t")
	if err != nil {
		return
	}
	data = append(data, '\n')
	_ = writeResolverCacheFileAtomic(c.cachePath(key), data)
}

func (c *RojoResolverCache) cachePath(key string) string {
	sum := sha256.Sum256([]byte(key))
	return filepath.Join(c.cacheDir, hex.EncodeToString(sum[:])+".rojocache.json")
}

func cacheKey(path string) string {
	abs, err := filepath.Abs(path)
	if err != nil {
		return filepath.Clean(path)
	}
	return filepath.Clean(abs)
}

func resolverMtimeManifest(state ResolverState) ([]resolverCacheManifestEntry, bool) {
	paths := append(slices.Clone(state.WalkedConfigs), state.WalkedDirectories...)
	paths = sortedStrings(paths)
	if len(paths) == 0 {
		return nil, false
	}
	manifest := make([]resolverCacheManifestEntry, 0, len(paths))
	var ok bool
	for _, path := range paths {
		info, err := os.Stat(path)
		if err != nil {
			return nil, false
		}
		entry := resolverCacheManifestEntry{
			Path:          path,
			MtimeUnixNano: info.ModTime().UnixNano(),
		}
		if info.IsDir() {
			if entry.Listing, ok = resolverDirectoryListing(path); !ok {
				return nil, false
			}
		} else if entry.Sha256, ok = resolverFileHash(path); !ok {
			return nil, false
		}
		manifest = append(manifest, entry)
	}
	for _, partition := range state.Partitions {
		if _, err := os.Stat(partition.FsPath); os.IsNotExist(err) {
			manifest = append(manifest, resolverCacheManifestEntry{Path: partition.FsPath, Missing: true})
		}
	}
	return manifest, true
}

// manifestStillValid reports whether every entry still holds. A directory
// whose mtime changed but whose listing did not gets its new mtime recorded.
func manifestStillValid(manifest []resolverCacheManifestEntry) bool {
	if len(manifest) == 0 {
		return false
	}
	for index := range manifest {
		entry := &manifest[index]
		info, err := os.Stat(entry.Path)
		if entry.Missing {
			if !os.IsNotExist(err) {
				return false
			}
			continue
		}
		if err != nil {
			return false
		}
		if mtime := info.ModTime().UnixNano(); mtime != entry.MtimeUnixNano {
			if entry.Listing == "" || !info.IsDir() {
				return false
			}
			if listing, ok := resolverDirectoryListing(entry.Path); !ok || listing != entry.Listing {
				return false
			}
			entry.MtimeUnixNano = mtime
		}
		if !info.IsDir() {
			if hash, ok := resolverFileHash(entry.Path); !ok || hash != entry.Sha256 {
				return false
			}
		}
	}
	return true
}

func resolverFileHash(path string) (string, bool) {
	data, err := os.ReadFile(path)
	if err != nil {
		return "", false
	}
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:]), true
}

// resolverDirectoryListing lists what parsing reads from a directory: its
// subdirectories and its Rojo project files.
func resolverDirectoryListing(dir string) (string, bool) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return "", false
	}
	var listing strings.Builder
	listing.WriteString("v")
	for _, entry := range entries {
		isDir, isRegular := entryKind(dir, entry)
		switch {
		case isDir:
			listing.WriteString("/d:" + entry.Name())
		case isRegular && rojoFileRegex.MatchString(entry.Name()):
			listing.WriteString("/f:" + entry.Name())
		}
	}
	return listing.String(), true
}

func stateMatchesManifest(state ResolverState, manifest []resolverCacheManifestEntry) bool {
	expected, ok := resolverMtimeManifest(state)
	return ok && slices.Equal(expected, manifest)
}

func writeResolverCacheFileAtomic(path string, data []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(filepath.Dir(path), ".rojo-cache-*.tmp")
	if err != nil {
		return err
	}
	tmpName := tmp.Name()
	_, writeErr := tmp.Write(data)
	closeErr := tmp.Close()
	if writeErr != nil || closeErr != nil {
		_ = os.Remove(tmpName)
		if writeErr != nil {
			return writeErr
		}
		return closeErr
	}
	if err := os.Rename(tmpName, path); err != nil {
		_ = os.Remove(tmpName)
		return err
	}
	return nil
}
