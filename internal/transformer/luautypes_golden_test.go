package transformer_test

import (
	"errors"
	"flag"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"
)

// updateLuauTypesGoldens rewrites testdata/luautypes/golden from the current
// renderer output instead of comparing against it:
//
//	go test ./internal/transformer -run TestLuauTypesGolden -update
//
// Review the resulting diff before committing it; the analyzer gate
// (TestLuauTypesFixturesAnalyzeClean) checks that the output type-checks, the
// goldens pin what it looks like.
var updateLuauTypesGoldens = flag.Bool("update", false, "rewrite testdata/luautypes/golden/*.luau from the current output")

// luauTypesGoldenDir is the golden root; each fixture src/<path>.ts has its
// rendered output at golden/<path>.luau.
const luauTypesGoldenDir = "../../testdata/transformer-unit/luautypes/golden"

// luauTypesGoldenUpdateHint names the command that refreshes the goldens.
const luauTypesGoldenUpdateHint = "go test ./internal/transformer -run TestLuauTypesGolden -update"

// luauTypesGoldenPins are lines that must appear in a fixture's rendered
// output no matter what `-update` rewrites. They pin the policy decisions a
// blind golden refresh could silently regress: the locals rule (an
// initialized local is inferred even with a written type, unless that type
// differs from the initializer's; an uninitialized one is annotated), the
// async spelling
// (the local carries the Promise type, the TS.async body the awaited one),
// the pack spellings of LuaTuple and void, the flat class alias with
// `self`-typed methods, the non-exported import mirrors and the loss comment
// a type function the translator cannot express leaves behind.
var luauTypesGoldenPins = map[string][]string{
	"src/locals.ts": {
		"\tlocal count = 0\n",
		"\tlocal mode: \"r\" | \"w\" = \"r\"\n",
		"\tlocal label: string?\n",
		"\tlocal names = { \"a\", \"b\" }\n",
	},
	"src/emission.ts": {
		"local fetched: (n: number) -> Promise<number> = TS.async(function(n: number): number\n",
		"local function luaTuple(): (number, string)\n",
		"local function nothing(): ()\n",
	},
	"src/classes.ts": {
		"\tlocal box: Box<number> = pair\n",
		"export type Animal = {\n",
		"\tspeak: (self: Animal) -> string,\n",
		"\tfunction Animal.new(name: string): Animal\n",
		"\tfunction Animal.constructor(self: Animal, name: string)\n",
	},
	"src/imports.ts": {
		"\ntype Shape = { kind: ShapeKind, area: (self: Shape) -> number }\n",
		"\ntype ShapeKind = \"circle\" | \"square\"\n",
	},
	"src/typefunctions.ts": {
		"export type function Partial2(T: type)\n",
		"-- luauTypes: Unwrap<T> spelled as any: ",
		"export type Unwrap<T> = any\n",
	},
}

// TestLuauTypesGolden renders every fixture under testdata/luautypes/src with
// luauTypes on and compares the output byte for byte with
// testdata/luautypes/golden/<same path>.luau. A golden without a fixture
// fails too, so a deleted fixture cannot leave its output behind.
func TestLuauTypesGolden(t *testing.T) {
	fixtures := luauTypesFixtures(t)
	expected := map[string]bool{}
	for _, relPath := range fixtures {
		name := strings.TrimSuffix(strings.TrimPrefix(relPath, "src/"), ".ts")
		goldenRel := name + ".luau"
		expected[goldenRel] = true
		t.Run(name, func(t *testing.T) {
			got := renderLuauTypes(t, relPath)
			goldenPath := filepath.Join(luauTypesGoldenDir, filepath.FromSlash(goldenRel))

			if *updateLuauTypesGoldens {
				if err := os.MkdirAll(filepath.Dir(goldenPath), 0o755); err != nil {
					t.Fatal(err)
				}
				if err := os.WriteFile(goldenPath, []byte(got), 0o644); err != nil {
					t.Fatal(err)
				}
				t.Logf("wrote %s (%d lines)", goldenPath, strings.Count(got, "\n"))
			}

			want, err := os.ReadFile(goldenPath)
			if errors.Is(err, fs.ErrNotExist) {
				t.Fatalf("no golden at %s; generate it with `%s`", goldenPath, luauTypesGoldenUpdateHint)
			}
			if err != nil {
				t.Fatal(err)
			}
			if diff := cmp.Diff(string(want), got); diff != "" {
				t.Errorf("%s differs from %s (-golden +got):\n%s\nIf the new output is intended, refresh it with `%s`.",
					relPath, goldenPath, diff, luauTypesGoldenUpdateHint)
			}
			for _, pin := range luauTypesGoldenPins[relPath] {
				if !strings.Contains(got, pin) {
					t.Errorf("%s output lost the pinned line %q", relPath, pin)
				}
			}
		})
	}

	err := filepath.WalkDir(luauTypesGoldenDir, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() {
			return nil
		}
		rel, relErr := filepath.Rel(luauTypesGoldenDir, path)
		if relErr != nil {
			return relErr
		}
		if rel = filepath.ToSlash(rel); !expected[rel] {
			t.Errorf("golden %s has no fixture under testdata/luautypes/src; delete it", path)
		}
		return nil
	})
	if err != nil && !errors.Is(err, fs.ErrNotExist) {
		t.Fatal(err)
	}
}
