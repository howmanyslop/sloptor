package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestGenDocsWritesPagePerVisibleCommand runs the hidden gen-docs command and
// checks the contract the docs site depends on: one MDX page per visible
// top-level command, each opening with frontmatter, all linked from index.mdx,
// and no page for hidden commands.
func TestGenDocsWritesPagePerVisibleCommand(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "commands")
	code, _, errOut := runCLI(t, "gen-docs", dir)
	if code != 0 {
		t.Fatalf("gen-docs exit = %d, stderr: %s", code, errOut)
	}

	index := readDocsPage(t, dir, "index")
	if !strings.HasPrefix(index, "---\ntitle: Commands\n") {
		t.Errorf("index.mdx frontmatter:\n%s", index)
	}
	for _, name := range []string{"build", "check", "diagnostics", "clean", "schema", "completion", "version"} {
		page := readDocsPage(t, dir, name)
		if !strings.HasPrefix(page, "---\ntitle: "+name+"\n") {
			t.Errorf("%s.mdx does not open with its frontmatter title:\n%s", name, page)
		}
		if !strings.Contains(page, "```sh\nsloptor "+name) {
			t.Errorf("%s.mdx missing its sloptor usage block:\n%s", name, page)
		}
		if !strings.Contains(index, "(./"+name+"/)") {
			t.Errorf("index.mdx does not link %s:\n%s", name, index)
		}
	}
	for _, hidden := range []string{"api-server", "gen-docs", "help"} {
		if _, err := os.Stat(filepath.Join(dir, hidden+".mdx")); err == nil {
			t.Errorf("gen-docs wrote a page for hidden command %s", hidden)
		}
	}
}

// TestGenDocsPageSections checks that flags, subcommands, and examples reach
// the pages, and that no line carries trailing whitespace.
func TestGenDocsPageSections(t *testing.T) {
	dir := t.TempDir()
	if code, _, errOut := runCLI(t, "gen-docs", dir); code != 0 {
		t.Fatalf("gen-docs exit = %d, stderr: %s", code, errOut)
	}
	tests := []struct {
		page string
		want []string
	}{
		{"build", []string{"## Options", "| `-w, --watch` | enable watch mode |", "| `--max-errors <n>` |", "| `50` |"}},
		{"completion", []string{"## Examples", "sloptor completion fish"}},
		{"index", []string{"[`diagnostics`](./diagnostics/)", "[`schema`](./schema/)"}},
	}
	for _, tt := range tests {
		t.Run(tt.page, func(t *testing.T) {
			page := readDocsPage(t, dir, tt.page)
			for _, want := range tt.want {
				if !strings.Contains(page, want) {
					t.Errorf("%s.mdx missing %q:\n%s", tt.page, want, page)
				}
			}
			for i, line := range strings.Split(page, "\n") {
				if strings.TrimRight(line, " \t") != line {
					t.Errorf("%s.mdx line %d has trailing whitespace: %q", tt.page, i+1, line)
				}
			}
		})
	}
}

// TestGenDocsHiddenFromHelp keeps the docs generator off the public surface.
func TestGenDocsHiddenFromHelp(t *testing.T) {
	_, out, _ := runCLI(t, "--help")
	if strings.Contains(out, "gen-docs") {
		t.Errorf("root help lists gen-docs:\n%s", out)
	}
}

func TestEscapeMDX(t *testing.T) {
	tests := []struct{ in, want string }{
		{"default <project>/include", "default &lt;project&gt;/include"},
		{"use `<entry>` as-is", "use `<entry>` as-is"},
		{"a {b} `{c}` d", "a &#123;b&#125; `{c}` d"},
		{"open ` <x>", "open ` &lt;x&gt;"},
	}
	for _, tt := range tests {
		if got := escapeMDX(tt.in); got != tt.want {
			t.Errorf("escapeMDX(%q) = %q, want %q", tt.in, got, tt.want)
		}
	}
}

func readDocsPage(t *testing.T, dir, name string) string {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(dir, name+".mdx"))
	if err != nil {
		t.Fatalf("read %s.mdx: %v", name, err)
	}
	return string(data)
}
