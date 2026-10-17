package compile

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

func TestSolutionWatchArtifactFilterCanonicalizesUniquePathsOnce(t *testing.T) {
	root := t.TempDir()
	out := filepath.Join(root, "out")
	alias := filepath.Join(root, "alias")
	assets := filepath.Join(root, "cafe\u0301")
	spelledOut := out + string(filepath.Separator) + "."
	directories := []string{out, alias, assets, assets}
	sets := []SolutionWatchSet{
		{ArtifactDirectories: []string{out, alias}, RootDirs: []string{out}, RojoDirectories: directories},
		{ArtifactDirectories: []string{spelledOut}, RojoDirectories: directories},
	}
	calls := map[string]int{}
	filterSolutionWatchArtifacts(sets, func(path string) string {
		path = filepath.Clean(filepath.FromSlash(path))
		calls[path]++
		if path == alias {
			return out
		}
		return path
	})
	if want := map[string]int{out: 1, alias: 1, assets: 1}; !reflect.DeepEqual(calls, want) {
		t.Errorf("canonical resolutions = %v, want %v", calls, want)
	}
	for _, set := range sets {
		if !reflect.DeepEqual(set.RojoDirectories, []string{assets, assets}) {
			t.Errorf("filtered directories = %v, want duplicate intentional assets preserved", set.RojoDirectories)
		}
	}
	if !reflect.DeepEqual(sets[0].RootDirs, []string{out}) {
		t.Errorf("explicit source root inside artifact was changed: %v", sets[0].RootDirs)
	}
	if !reflect.DeepEqual(directories, []string{out, alias, assets, assets}) {
		t.Errorf("cached directory slice was mutated: %v", directories)
	}
}

func TestWatchSetsRechecksCachedSymlinkOwnership(t *testing.T) {
	root := t.TempDir()
	out := filepath.Join(root, "out")
	external := filepath.Join(root, "external")
	for _, path := range []string{out, external} {
		if err := os.MkdirAll(path, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	alias := filepath.Join(root, "alias")
	if err := os.Symlink(out, alias); err != nil {
		t.Skipf("cannot create symlink: %v", err)
	}
	config := filepath.Join(root, "tsconfig.json")
	cached := SolutionWatchSet{
		ProjectPath:         config,
		ArtifactDirectories: []string{out},
		RootDirs:            []string{alias},
		RojoDirectories:     []string{alias, external},
	}
	coordinator := &SolutionCoordinator{
		graph:     &SolutionGraph{Projects: []SolutionProject{{ConfigPath: config}}},
		watchSets: map[string]SolutionWatchSet{config: cached},
	}
	if got := coordinator.WatchSets()[0].RojoDirectories; !reflect.DeepEqual(got, []string{external}) {
		t.Fatalf("artifact alias was not excluded: %v", got)
	}
	if err := os.Remove(alias); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(external, alias); err != nil {
		t.Fatal(err)
	}
	sets := coordinator.WatchSets()
	if got := sets[0].RojoDirectories; !reflect.DeepEqual(got, []string{alias, external}) {
		t.Fatalf("retargeted alias was not restored without invalidation: %v", got)
	}
	if !reflect.DeepEqual(sets[0].RootDirs, cached.RootDirs) || !reflect.DeepEqual(coordinator.watchSets[config], cached) {
		t.Fatal("ownership filtering changed explicit roots or the cached set")
	}
}

func TestWatchSetsMemoPreservesRoutingAndCachedDirectories(t *testing.T) {
	root := t.TempDir()
	out := filepath.Join(root, "missing", "out")
	generated := filepath.Join(out, "nested")
	nfd := filepath.Join(root, "cafe\u0301")
	external := filepath.Join(root, "missing", "assets")
	if err := os.MkdirAll(nfd, 0o755); err != nil {
		t.Fatal(err)
	}
	config := filepath.Join(root, "tsconfig.json")
	cached := SolutionWatchSet{
		ProjectPath:         config,
		ArtifactDirectories: []string{out, out},
		RootDirs:            []string{generated},
		RojoDirectories:     []string{generated, nfd, external},
	}
	coordinator := &SolutionCoordinator{
		graph:     &SolutionGraph{Projects: []SolutionProject{{ConfigPath: config}}},
		watchSets: map[string]SolutionWatchSet{config: cached},
	}
	first := coordinator.WatchSets()
	second := coordinator.WatchSets()
	if !reflect.DeepEqual(first, second) || !reflect.DeepEqual(first[0].RojoDirectories, []string{nfd, external}) {
		t.Fatalf("cached watch routing changed: first=%+v, second=%+v", first, second)
	}
	if !reflect.DeepEqual(first[0].RootDirs, []string{generated}) {
		t.Fatalf("explicit source root inside artifact changed: %v", first[0].RootDirs)
	}
	first[0].RojoDirectories[0] = "caller mutation"
	if got := coordinator.WatchSets(); !reflect.DeepEqual(got, second) {
		t.Fatalf("returned Rojo directories mutated cache: %+v", got)
	}
	cached.ArtifactDirectories = []string{nfd}
	coordinator.watchSets[config] = cached
	if got := coordinator.WatchSets()[0].RojoDirectories; !reflect.DeepEqual(got, []string{generated, external}) {
		t.Fatalf("changed artifact ownership did not refresh filter: %v", got)
	}
}
