package compile

import (
	"os"
	"path/filepath"
	"sort"
)

// PackageWatchInputs are layout inputs, never passthrough assets or solution
// configuration. Directories need nonrecursive watches for discovery changes.
type PackageWatchInputs struct {
	Files       []string
	Directories []string
	fingerprint string
}

func (p *packageLayout) watchInputs(dir string, typeRoots []string) *PackageWatchInputs {
	if p == nil {
		return nil
	}
	inputs := &PackageWatchInputs{}
	inputs.fingerprint, _ = p.fingerprint(dir)
	directories := map[string]struct{}{}
	files := map[string]struct{}{}
	addDirectory := func(path string) {
		for {
			if info, err := os.Stat(path); err == nil && info.IsDir() {
				directories[path] = struct{}{}
				return
			}
			parent := filepath.Dir(path)
			if parent == path {
				return
			}
			path = parent
		}
	}
	checkedAncestors := map[string]struct{}{}
	watchAliases := func(path string) {
		for {
			if _, checked := checkedAncestors[path]; checked {
				return
			}
			checkedAncestors[path] = struct{}{}
			parent := filepath.Dir(path)
			if parent == path {
				return
			}
			// Junctions are ModeIrregular on Windows. Watch the entry's
			// parent: subscriptions inside its old target survive retargets.
			if info, err := os.Lstat(path); err == nil && info.Mode()&(os.ModeSymlink|os.ModeIrregular) != 0 {
				addDirectory(parent)
			}
			path = parent
		}
	}
	addFile := func(path string) {
		parent := filepath.Dir(path)
		watchAliases(path)
		addDirectory(parent)
		if info, err := os.Stat(parent); err == nil && info.IsDir() {
			files[path] = struct{}{}
		}
	}
	for _, root := range typeRoots {
		root = filepath.Clean(filepath.FromSlash(root))
		watchAliases(root)
		addDirectory(root)
		addDirectory(filepath.Dir(root))
	}
	for _, resolver := range p.resolvers {
		state := resolver.GetState()
		// parseConfig records explicit references before checking existence.
		// Keep missing named configs subscribed so recreation is observed.
		for _, path := range state.WalkedConfigs {
			addFile(path)
		}
		for _, directory := range state.WalkedDirectories {
			watchAliases(directory)
			addDirectory(directory)
			addDirectory(filepath.Dir(directory))
			addFile(filepath.Join(directory, "default.project.json"))
		}
		for _, partition := range state.Partitions {
			addFile(filepath.Join(partition.FsPath, "default.project.json"))
		}
	}
	for _, path := range append(p.files.Paths(), p.manifests...) {
		addFile(path)
	}
	for path := range files {
		inputs.Files = append(inputs.Files, path)
	}
	for path := range directories {
		inputs.Directories = append(inputs.Directories, path)
	}
	sort.Strings(inputs.Files)
	sort.Strings(inputs.Directories)
	return inputs
}

// RefreshPackageWatchInputs rechecks discovery after a package-input event.
// Even an unchanged mapping can need new subscriptions (a new directory or a
// retargeted junction). Only semantic changes require a compiler build.
func (c *SolutionCoordinator) RefreshPackageWatchInputs(projectPath string) bool {
	previous := c.watchSets[projectPath].PackageInputs
	for _, project := range c.graph.Projects {
		if project.ConfigPath != projectPath {
			continue
		}
		set := c.projectWatchSet(project, map[string]rojoWatchState{})
		c.watchSets[projectPath] = set
		if previous == nil || set.PackageInputs == nil {
			return previous != set.PackageInputs
		}
		return previous.fingerprint != set.PackageInputs.fingerprint
	}
	return false
}
