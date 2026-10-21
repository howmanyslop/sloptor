package rojo

import (
	"os"
	"path/filepath"
	"runtime"
	"sync"
)

// directoryListings holds the directory reads one resolver walk will make,
// fetched ahead of it in parallel. The walk itself stays sequential, since
// its partition and path order is the output, but a package build walks every
// installed package under its type roots and nearly all of that time is
// ReadDir and realpath syscalls that do not depend on one another.
type directoryListings struct {
	entries   sync.Map // physical directory -> listing
	realpaths sync.Map // logical link path -> physical path
}

type listing struct {
	entries []os.DirEntry
	err     error
}

// prefetchDirectoryListings reads the tree under root the way searchDirectory
// will: every directory down to the first one holding a default.project.json,
// following links to directories.
func prefetchDirectoryListings(root string) *directoryListings {
	listings := &directoryListings{}
	limit := make(chan struct{}, max(4, runtime.GOMAXPROCS(0)*2))
	var visited sync.Map
	var wg sync.WaitGroup
	var visit func(physical string)
	visit = func(physical string) {
		if _, seen := visited.LoadOrStore(physical, true); seen {
			return
		}
		wg.Go(func() {
			limit <- struct{}{}
			entries, err := os.ReadDir(physical)
			listings.entries.Store(physical, listing{entries: entries, err: err})
			var links []os.DirEntry
			if err == nil {
				for _, entry := range entries {
					if entry.Name() == rojoDefaultName {
						<-limit
						return
					}
				}
				for _, entry := range entries {
					switch entry.Type() {
					case os.ModeDir:
						visit(filepath.Join(physical, entry.Name()))
					case 0:
					default:
						links = append(links, entry)
					}
				}
			}
			<-limit
			for _, entry := range links {
				child := filepath.Join(physical, entry.Name())
				info, err := os.Stat(child)
				if err != nil || !info.IsDir() {
					continue
				}
				real := realpathOr(child)
				listings.realpaths.Store(child, real)
				visit(real)
			}
		})
	}
	visit(realpathOr(root))
	wg.Wait()
	return listings
}

func (l *directoryListings) readDir(physical string) ([]os.DirEntry, error) {
	if l != nil {
		if cached, ok := l.entries.Load(physical); ok {
			found := cached.(listing)
			return found.entries, found.err
		}
	}
	return os.ReadDir(physical)
}

// realpath answers for a link the prefetch followed; the prefetch only saw
// links under physical directories, so the logical path has to match one.
func (l *directoryListings) realpath(path string) string {
	if l != nil {
		if real, ok := l.realpaths.Load(path); ok {
			return real.(string)
		}
	}
	return realpathOr(path)
}
