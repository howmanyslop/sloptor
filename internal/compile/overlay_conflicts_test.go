package compile

import (
	"path/filepath"
	"testing"
)

func TestProjectRejectsConflictingOverlayAliases(t *testing.T) {
	// Two spellings of one file must not let map iteration choose its contents.
	// Identical text is unambiguous; conflicting text must reject the build.
	for _, conflicting := range []bool{false, true} {
		name := "identical"
		if conflicting {
			name = "conflicting"
		}
		t.Run(name, func(t *testing.T) {
			dir := writeProject(t, "overlay-aliases", "")
			file := filepath.Join(dir, "src", "main.ts")
			alias := filepath.Join(dir, "src") + string(filepath.Separator) + "." + string(filepath.Separator) + "main.ts"
			const text = "export const value = 1;\n"
			aliasText := text
			if conflicting {
				aliasText = "export const value = 2;\n"
			}
			_, program, _, err := newProjectProgramWithOptions(dir, "", ProjectOptions{Overlays: map[string]string{
				file: text, alias: aliasText,
			}})
			if conflicting {
				if err == nil || program != nil {
					t.Fatal("conflicting aliases produced a program instead of rejecting the ambiguous input")
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			source := program.GetSourceFile(filepath.ToSlash(file))
			if source == nil || source.Text() != text {
				t.Fatal("identical aliases did not preserve the requested source text")
			}
		})
	}
}
