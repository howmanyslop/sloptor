package main

import (
	"os"
	"path/filepath"
	"testing"
)

// TestHasRbxtsTypes catches the language server running a census on a plain
// TypeScript project (no roblox-ts), which would pop up "rootDir must be
// defined" in the editor, and equally a roblox-ts project with hoisted
// node_modules being skipped. The expectations are the requirement's three
// cases: found beside, found above, absent.
func TestHasRbxtsTypes(t *testing.T) {
	tests := []struct {
		name string
		// rbxtsAt is where node_modules/@rbxts is created, relative to the temp
		// root; nil means nowhere.
		rbxtsAt *string
		want    bool
	}{
		{name: "same directory", rbxtsAt: strPtr("pkg"), want: true},
		{name: "ancestor directory", rbxtsAt: strPtr("."), want: true},
		{name: "absent", rbxtsAt: nil, want: false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			root := t.TempDir()
			project := filepath.Join(root, "pkg")
			if err := os.MkdirAll(project, 0o750); err != nil {
				t.Fatal(err)
			}
			if tt.rbxtsAt != nil {
				if err := os.MkdirAll(filepath.Join(root, *tt.rbxtsAt, "node_modules", "@rbxts"), 0o750); err != nil {
					t.Fatal(err)
				}
			}
			if got := hasRbxtsTypes(project); got != tt.want {
				t.Errorf("hasRbxtsTypes = %v, want %v", got, tt.want)
			}
		})
	}
}
