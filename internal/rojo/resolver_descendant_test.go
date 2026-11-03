package rojo

import (
	"math/rand/v2"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

// relDescendantOracle is the filepath.Rel definition of descendance that
// RojoResolver.ts uses: any relative path starting with ".." is outside.
func relDescendantOracle(filePath, dirPath string) bool {
	if dirPath == filePath {
		return true
	}
	rel, err := filepath.Rel(dirPath, filePath)
	if err != nil {
		return false
	}
	return !strings.HasPrefix(rel, "..")
}

// Catches a file being assigned to the wrong Rojo partition, or to none, because
// of how its path or the partition's path happens to be spelled.
func TestIsPathDescendantOfEdgeCases(t *testing.T) {
	t.Parallel()

	cases := []struct {
		name     string
		filePath string
		dirPath  string
		want     bool
	}{
		{"equal absolute", "/a/b", "/a/b", true},
		{"equal root", "/", "/", true},
		{"equal relative", "a/b", "a/b", true},
		{"equal empty", "", "", true},
		{"equal unclean", "/a//b", "/a//b", true},
		{"child", "/a/b/c", "/a/b", true},
		{"deep child", "/a/b/c/d/e.luau", "/a/b", true},
		{"parent", "/a", "/a/b", false},
		{"root is not inside child", "/", "/a", false},
		{"child of root", "/a", "/", true},
		{"deep child of root", "/a/b", "/", true},
		{"dotdot-named child of root", "/..foo", "/", false},
		{"dir with trailing separator", "/a/b/c", "/a/b/", true},
		{"file with trailing separator", "/a/b/c/", "/a/b", true},
		{"same dir with trailing separator", "/a/b", "/a/b/", true},
		{"same file with trailing separator", "/a/b/", "/a/b", true},
		{"root with doubled separator", "/a", "//", true},
		{"dotdot-named child", "/a/b/..foo/bar", "/a/b", false},
		{"bare dotdot-named child", "/a/b/..foo", "/a/b", false},
		{"triple-dot child", "/a/b/...", "/a/b", false},
		{"triple-dot child with tail", "/a/b/.../c", "/a/b", false},
		{"dotdot-suffixed child", "/a/b/foo../bar", "/a/b", true},
		{"dotdot-named grandchild", "/a/b/c/..foo", "/a/b", true},
		{"dot-named child", "/a/b/.foo", "/a/b", true},
		{"sibling sharing a name prefix", "/a/bc", "/a/b", false},
		{"child of sibling sharing a name prefix", "/a/bc/d", "/a/b", false},
		{"sibling", "/a/c", "/a/b", false},
		{"different root", "/x/y", "/a/b", false},
		{"different case", "a/B/c", "a/b/c", runtime.GOOS == "windows"},
		{"doubled separator in file", "/a//b/c", "/a/b", true},
		{"doubled separator in dir", "/a/b/c", "/a//b", true},
		{"dot segment in file", "/a/./b/c", "/a/b", true},
		{"dot segment in dir", "/a/b/c", "/a/./b", true},
		{"dotdot segment in file", "/a/x/../b/c", "/a/b", true},
		{"dotdot segment in dir", "/a/b/c", "/a/x/../b", true},
		{"dotdot segment leaves dir", "/a/b/../c", "/a/b", false},
		{"unclean file equal to dir once cleaned", "/a/x/../b", "/a/b", true},
		{"dotdot above root", "/../a", "/a", true},
		{"relative child", "a/b", "a", true},
		{"relative sibling sharing a name prefix", "ab/c", "a", false},
		{"relative dot dir", "a", ".", true},
		{"relative parent escape", "../a", ".", false},
		{"relative doubled separator", "a//b", "a/b", true},
		{"relative dot segment", "a/./b", "a/b", true},
		{"relative dotdot segment", "a/../b", "b", true},
		{"relative file under absolute dir", "a/b", "/a", false},
		{"absolute file under relative dir", "/a/b", "a", false},
		{"empty file", "", "/a", false},
		{"empty dir with absolute file", "/a", "", false},
		{"empty dir with relative file", "a", "", true},
		{"empty dir with dot", ".", "", true},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			if got := relDescendantOracle(tc.filePath, tc.dirPath); got != tc.want {
				t.Fatalf("oracle(%q, %q) = %v, table says %v", tc.filePath, tc.dirPath, got, tc.want)
			}
			if got := isPathDescendantOf(tc.filePath, tc.dirPath); got != tc.want {
				t.Fatalf("isPathDescendantOf(%q, %q) = %v, want %v", tc.filePath, tc.dirPath, got, tc.want)
			}
		})
	}
}

// Catches any spelling of a path, not just the ones listed above, deciding
// partition membership differently from the filepath.Rel definition.
func TestIsPathDescendantOfMatchesRel(t *testing.T) {
	t.Parallel()

	segments := []string{"", ".", "..", "...", "..foo", "foo..", ".foo", "foo", "foobar", "FOO", "b", "a b", `a\b`, "é"}
	roots := []string{"", "/", "//"}
	trailers := []string{"", "/", "/."}

	var shallow []string
	for _, root := range roots {
		for _, trailer := range trailers {
			shallow = append(shallow, root+trailer)
			for _, first := range segments {
				shallow = append(shallow, root+first+trailer)
				for _, second := range segments {
					shallow = append(shallow, root+first+"/"+second+trailer)
				}
			}
		}
	}

	check := func(t *testing.T, filePath, dirPath string) {
		t.Helper()
		want := relDescendantOracle(filePath, dirPath)
		if got := isPathDescendantOf(filePath, dirPath); got != want {
			t.Errorf("isPathDescendantOf(%q, %q) = %v, oracle says %v", filePath, dirPath, got, want)
		}
	}

	for _, dirPath := range shallow {
		for _, filePath := range shallow {
			check(t, filePath, dirPath)
		}
	}

	// Deep pairs that share a prefix, which independent random paths rarely do.
	random := rand.New(rand.NewPCG(0x726f746f72, 1))
	randomPath := func(maxDepth int) string {
		parts := make([]string, random.IntN(maxDepth+1))
		for i := range parts {
			parts[i] = segments[random.IntN(len(segments))]
		}
		return strings.Join(parts, "/")
	}
	for range 500_000 {
		root := roots[random.IntN(len(roots))]
		dirPath := root + randomPath(6)
		filePath := dirPath + "/" + randomPath(4)
		if random.IntN(4) == 0 {
			filePath = root + randomPath(6) + filePath[len(root):]
		}
		check(t, filePath, dirPath)
		check(t, dirPath, filePath)
	}
}

func BenchmarkIsPathDescendantOf(b *testing.B) {
	const (
		filePath = "/Users/dev/projects/game/packages/react-hooks/out/use-event/use-event-listener.luau"
		matching = "/Users/dev/projects/game/packages/react-hooks/out"
		sibling  = "/Users/dev/projects/game/packages/react-spring/out"
	)

	b.Run("match", func(b *testing.B) {
		b.ReportAllocs()
		for b.Loop() {
			if !isPathDescendantOf(filePath, matching) {
				b.Fatal("want descendant")
			}
		}
	})
	b.Run("non-match", func(b *testing.B) {
		b.ReportAllocs()
		for b.Loop() {
			if isPathDescendantOf(filePath, sibling) {
				b.Fatal("want non-descendant")
			}
		}
	})
}
