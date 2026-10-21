package compile

import (
	"fmt"
	"slices"
	"strings"
	"testing"

	"rotor/tsgo/ast"
)

// Catches a sharded transform that loses a file or sends one to two workers,
// which would leave that file untransformed or transform it twice. The
// expected set is the input set.
func TestSplitSidecarShardsPartitionsEveryFileOnce(t *testing.T) {
	var files []*ast.SourceFile
	for index := range 1000 {
		body := strings.Repeat("export const value = 1;\n", 1+index%37)
		files = append(files, parseSourceFile(fmt.Sprintf("/proj/src/%04d.ts", (index*7919)%1000), body))
	}
	for _, count := range []int{2, 3, 4, 7} {
		shards := splitSidecarShards(files, count)
		if len(shards) != count {
			t.Fatalf("count %d: got %d shards", count, len(shards))
		}
		var seen []string
		for _, shard := range shards {
			if len(shard) == 0 {
				t.Fatalf("count %d: empty shard", count)
			}
			for _, file := range shard {
				seen = append(seen, file.FileName())
			}
		}
		var want []string
		for _, file := range files {
			want = append(want, file.FileName())
		}
		slices.Sort(seen)
		slices.Sort(want)
		if !slices.Equal(seen, want) {
			t.Fatalf("count %d: shards do not cover the input exactly once", count)
		}
	}
}

// Catches a small project or an incremental rebuild fanning out across
// several Node workers, each building its own program, for a handful of
// files. The threshold is sidecarShardMinFiles per shard.
func TestSidecarShardCountKeepsSmallRequestsOnOneWorker(t *testing.T) {
	t.Setenv(sidecarShardsEnv, "4")
	for _, files := range []int{0, 1, 3, sidecarShardMinFiles*2 - 1} {
		if got := sidecarShardCount(files); got > 1 && files < sidecarShardMinFiles*2 {
			t.Fatalf("%d files: got %d shards, want 1", files, got)
		}
	}
	if got := sidecarShardCount(sidecarShardMinFiles * 4); got != 4 {
		t.Fatalf("%d files: got %d shards, want 4", sidecarShardMinFiles*4, got)
	}
	t.Setenv(sidecarShardsEnv, "1")
	if got := sidecarShardCount(sidecarShardMinFiles * 4); got != 1 {
		t.Fatalf("sharding off: got %d shards, want 1", got)
	}
}
