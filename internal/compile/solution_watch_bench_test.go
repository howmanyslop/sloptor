package compile

import (
	"fmt"
	"path/filepath"
	"testing"
)

// newManyReferenceFixture builds a solution where game references count
// packages that each reference one shared core project.
func newManyReferenceFixture(tb testing.TB, count int) *referenceFixture {
	tb.Helper()
	f := newReferenceFixture(tb)
	f.project("core", nil, nil)
	references := make([]string, 0, count)
	for index := range count {
		name := fmt.Sprintf("pkg%02d", index)
		f.project(name, []string{"core"}, nil)
		for file := range 3 {
			f.write(fmt.Sprintf("%s/src/file%d.ts", name, file), fmt.Sprintf("export const value%d = %d;\n", file, file))
		}
		references = append(references, name)
	}
	f.project("game", references, nil)
	f.write("game/src/index.ts", `import { value } from "../../pkg00/src"; export const result = value;`)
	return f
}

// BenchmarkSolutionWatchEditCycle measures one watch cycle after a text edit
// in a leaf package: Invalidate, Drain, and WatchSets.
func BenchmarkSolutionWatchEditCycle(b *testing.B) {
	f := newManyReferenceFixture(b, 40)
	coordinator, err := NewSolutionCoordinator(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err != nil {
		b.Fatal(err)
	}
	if _, messages, err := coordinator.Drain(); err != nil {
		b.Fatalf("first build: %v (%v)", err, messages)
	}
	leaf := filepath.Join(f.dir, "pkg39/tsconfig.json")
	b.ResetTimer()
	for index := range b.N {
		f.write("pkg39/src/file0.ts", fmt.Sprintf("export const value0 = %d;\n", index+10))
		coordinator.Invalidate(leaf)
		if _, messages, err := coordinator.Drain(); err != nil {
			b.Fatalf("edit build: %v (%v)", err, messages)
		}
		coordinator.WatchSets()
	}
}
