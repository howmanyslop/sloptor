package rojo

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"runtime"
	"slices"
	"testing"
)

func directoryLink(t *testing.T, kind, link, target string) {
	t.Helper()
	if kind == "junction" {
		if runtime.GOOS != "windows" {
			t.Skip("directory junctions are Windows-only")
		}
		if output, err := exec.Command("cmd", "/c", "mklink", "/J", link, target).CombinedOutput(); err != nil {
			t.Fatalf("create directory junction: %v: %s", err, output)
		}
	} else if err := os.Symlink(target, link); err != nil {
		t.Skipf("directory symlinks unavailable: %v", err)
	}
}

func TestResolverDirectoryLinkCycles(t *testing.T) {
	for _, kind := range []string{"junction", "symlink"} {
		for _, constructor := range []struct {
			name string
			new  func(string) *RojoResolver
		}{
			{"Synthetic", Synthetic},
			{"PackageImports", func(path string) *RojoResolver { return PackageImports(path, NewProjectFiles()) }},
		} {
			t.Run(kind+"/"+constructor.name, func(t *testing.T) {
				dir := writeProject(t, map[string]string{
					"data/default.project.json": `{"name":"Data","tree":{"$path":"src"}}`,
					"data/src/init.luau":        "return {}",
				})
				directoryLink(t, kind, filepath.Join(dir, "loop"), dir)

				r := constructor.new(dir)
				if got := len(r.GetPartitions()); got != 2 {
					t.Fatalf("cycle produced %d partitions, want root and one nested project", got)
				}
				if got := mustRbxPath(t, r, filepath.Join(dir, "data", "src", "init.luau")); !reflect.DeepEqual(got, RbxPath{"Data"}) {
					t.Errorf("nested project path = %v, want [Data]", got)
				}
			})
		}
	}
}

func TestResolverDirectoryLinkCycleThroughNestedProject(t *testing.T) {
	dir := writeProject(t, map[string]string{
		"data/default.project.json": `{"name":"Data","tree":{"$path":"src"}}`,
		"data/src/init.luau":        "return {}",
	})
	directoryLink(t, "symlink", filepath.Join(dir, "data", "src", "back"), dir)

	r := Synthetic(dir)
	if got := len(r.GetPartitions()); got != 2 {
		t.Fatalf("cycle through nested project produced %d partitions, want 2", got)
	}
}

func TestResolverDirectoryLinkCycleThroughDefaultProject(t *testing.T) {
	dir := writeProject(t, map[string]string{
		"pkg/default.project.json": `{"name":"Pkg","tree":{"$path":"src"}}`,
	})
	kind := "symlink"
	if runtime.GOOS == "windows" {
		kind = "junction"
	}
	directoryLink(t, kind, filepath.Join(dir, "pkg", "src"), filepath.Join(dir, "pkg"))

	r := Synthetic(dir)
	if got := len(r.GetPartitions()); got != 1 {
		t.Fatalf("default-project cycle produced %d partitions, want only the outer root", got)
	}
	if warnings := r.GetWarnings(); len(warnings) != 0 {
		t.Fatalf("cycle through default project must stop before filesystem errors: %v", warnings)
	}
	if got := len(r.GetState().WalkedConfigs); got != 1 {
		t.Fatalf("cycle visited %d project paths, want 1", got)
	}
}

func TestResolverWorkspacePackageCycle(t *testing.T) {
	dir := writeProject(t, map[string]string{
		"a/data/default.project.json": `{"name":"Data","tree":{"$path":"src"}}`,
		"a/data/src/init.luau":        "return {}",
		"a/node_modules/":             "",
		"b/node_modules/":             "",
	})
	kind := "symlink"
	if runtime.GOOS == "windows" {
		kind = "junction"
	}
	directoryLink(t, kind, filepath.Join(dir, "a", "node_modules", "b"), filepath.Join(dir, "b"))
	directoryLink(t, kind, filepath.Join(dir, "b", "node_modules", "a"), filepath.Join(dir, "a"))

	r := PackageImports(dir, NewProjectFiles())
	if got := len(r.GetPartitions()); got != 2 {
		t.Fatalf("workspace cycle produced %d partitions, want root and one nested project", got)
	}
	if got := mustRbxPath(t, r, filepath.Join(dir, "a", "data", "src", "init.luau")); !reflect.DeepEqual(got, RbxPath{"a", "Data"}) {
		t.Errorf("workspace package path = %v, want [a Data]", got)
	}
}

func TestPackageImportsBranchingDirectoryLinks(t *testing.T) {
	const levels = 8
	files := make(map[string]string)
	for level := range levels {
		files[fmt.Sprintf("level%d/", level)] = ""
	}
	files[fmt.Sprintf("level%d/data/default.project.json", levels-1)] = `{"name":"Data","tree":{"$path":"src"}}`
	files[fmt.Sprintf("level%d/data/src/init.luau", levels-1)] = "return {}"
	dir := writeProject(t, files)
	root := filepath.Join(dir, "level0")
	kind := "symlink"
	if runtime.GOOS == "windows" {
		kind = "junction"
	}
	for level := range levels {
		parent := filepath.Join(dir, fmt.Sprintf("level%d", level))
		if level == levels-1 {
			directoryLink(t, kind, filepath.Join(parent, "back"), root)
			continue
		}
		target := filepath.Join(dir, fmt.Sprintf("level%d", level+1))
		for _, name := range []string{"a", "b"} {
			directoryLink(t, kind, filepath.Join(parent, name), target)
		}
	}

	r := PackageImports(root, NewProjectFiles())
	const maxDirectories = 3 * (levels + 2)
	if got := len(r.GetState().WalkedDirectories); got > maxDirectories {
		t.Fatalf("branching links recorded %d watch directories, want at most %d", got, maxDirectories)
	}
	if got := len(r.GetPartitions()); got != 2 {
		t.Fatalf("branching aliases produced %d partitions, want root and one nested project", got)
	}
}

func TestPackageImportsRepeatedDirectoryLinksUseFirstDiscovery(t *testing.T) {
	dir := writeProject(t, map[string]string{
		"root/":                             "",
		"package/data/default.project.json": `{"name":"Data","tree":{"$path":"src"}}`,
		"package/data/src/init.luau":        "return {}",
	})
	root := filepath.Join(dir, "root")
	for _, name := range []string{"a", "b"} {
		directoryLink(t, "symlink", filepath.Join(root, name), filepath.Join(dir, "package"))
	}

	r := PackageImports(root, NewProjectFiles())
	if got := len(r.GetPartitions()); got != 2 {
		t.Fatalf("repeated links produced %d partitions, want root and one nested project", got)
	}
	for _, test := range []struct {
		name string
		want RbxPath
	}{
		{"a", RbxPath{"a", "Data"}},
		{"b", RbxPath{"b", "data", "src"}},
	} {
		if got := mustRbxPath(t, r, filepath.Join(root, test.name, "data", "src", "init.luau")); !reflect.DeepEqual(got, test.want) {
			t.Errorf("link %s path = %v, want %v", test.name, got, test.want)
		}
	}
	if !slices.Contains(r.GetState().WalkedDirectories, filepath.Join(root, "b")) {
		t.Error("skipped package link must retain its logical watch path")
	}
}

func TestFromTreeRepeatedDirectoryMounts(t *testing.T) {
	dir := writeProject(t, map[string]string{
		"data/default.project.json": `{"name":"Data","tree":{"$path":"src"}}`,
		"data/src/init.luau":        "return {}",
	})
	path := "."
	r := FromTree(dir, &Tree{Children: []TreeEntry{
		{Name: "First", Tree: &Tree{Path: &path}},
		{Name: "Second", Tree: &Tree{Path: &path}},
	}})
	if got := len(r.GetPartitions()); got != 4 {
		t.Fatalf("separate mounts produced %d partitions, want 4", got)
	}
	if got := mustRbxPath(t, r, filepath.Join(dir, "data", "src", "init.luau")); !reflect.DeepEqual(got, RbxPath{"Second", "Data"}) {
		t.Errorf("later mount path = %v, want [Second Data]", got)
	}
}

func TestResolverDiscoveredProjectRepeatedDirectoryMounts(t *testing.T) {
	dir := writeProject(t, map[string]string{
		"root/pkg/default.project.json":    `{"name":"Pkg","tree":{"First":{"$path":"../../shared"},"Second":{"$path":"../../shared"}}}`,
		"shared/data/default.project.json": `{"name":"Data","tree":{"$path":"src"}}`,
		"shared/data/src/init.luau":        "return {}",
	})
	r := Synthetic(filepath.Join(dir, "root"))
	if got := mustRbxPath(t, r, filepath.Join(dir, "shared", "data", "src", "init.luau")); !reflect.DeepEqual(got, RbxPath{"Pkg", "Second", "Data"}) {
		t.Errorf("later mount in discovered project = %v, want [Pkg Second Data]", got)
	}
}
