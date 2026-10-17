package compile

import (
	"fmt"
	"os"
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

func BenchmarkSolutionWatchSetsCached(b *testing.B) {
	f := newManyReferenceFixture(b, 40)
	coordinator, err := NewSolutionCoordinator(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
	if err != nil {
		b.Fatal(err)
	}
	coordinator.WatchSets()
	b.ReportAllocs()
	b.ResetTimer()
	for range b.N {
		coordinator.WatchSets()
	}
}

// BenchmarkSolutionWatchEditCycle measures one watch cycle: Invalidate,
// Drain, and WatchSets. leaf edits text in one package, core edits text in
// the project every package references, and file adds or removes a source
// file in one package, which changes its file set.
func BenchmarkSolutionWatchEditCycle(b *testing.B) {
	cases := []struct {
		name    string
		project string
		edit    func(f *referenceFixture, index int)
	}{
		{"leaf", "pkg39", func(f *referenceFixture, index int) {
			f.write("pkg39/src/file0.ts", fmt.Sprintf("export const value0 = %d;\n", index+10))
		}},
		{"core", "core", func(f *referenceFixture, index int) {
			f.write("core/src/index.ts", fmt.Sprintf("export const value = %d;\n", index+10))
		}},
		{"file", "pkg39", func(f *referenceFixture, index int) {
			if index%2 == 0 {
				f.write("pkg39/src/added.ts", "export const added = 1;\n")
				return
			}
			if err := os.Remove(filepath.Join(f.dir, "pkg39/src/added.ts")); err != nil {
				b.Fatal(err)
			}
		}},
	}
	for _, test := range cases {
		b.Run(test.name, func(b *testing.B) {
			f := newManyReferenceFixture(b, 40)
			coordinator, err := NewSolutionCoordinator(filepath.Join(f.dir, "game/tsconfig.json"), f.options())
			if err != nil {
				b.Fatal(err)
			}
			if _, messages, err := coordinator.Drain(); err != nil {
				b.Fatalf("first build: %v (%v)", err, messages)
			}
			project := filepath.Join(f.dir, test.project, "tsconfig.json")
			b.ResetTimer()
			for index := range b.N {
				test.edit(f, index)
				coordinator.Invalidate(project)
				if _, messages, err := coordinator.Drain(); err != nil {
					b.Fatalf("edit build: %v (%v)", err, messages)
				}
				coordinator.WatchSets()
			}
		})
	}
}
