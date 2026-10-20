package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/spf13/cobra"
	"github.com/spf13/pflag"

	"rotor/internal/term"
)

// newGenDocsCommand is the hidden `sloptor gen-docs <dir>`: it writes the CLI
// reference for the docs site as Starlight MDX pages, one per visible
// top-level command plus an index.mdx overview table. Pages are built from the
// same command tree, flag placeholders, and visibility rules the help renderer
// uses, so the docs cannot drift from --help. The directory is created if
// missing and its pages are overwritten in place; it is generated output and
// belongs in .gitignore, so stale pages never need pruning.
func newGenDocsCommand() *cobra.Command {
	return &cobra.Command{
		Use:    "gen-docs <dir>",
		Short:  "write the CLI reference as MDX pages for the docs site",
		Hidden: true,
		Args:   cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, argv []string) error {
			if err := writeCommandDocs(cmd.Root(), argv[0]); err != nil {
				return runtimeFailure(err)
			}
			return nil
		},
	}
}

// writeCommandDocs renders index.mdx and one <name>.mdx per visible top-level
// command of root into dir.
func writeCommandDocs(root *cobra.Command, dir string) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	commands := visibleCommands(root)
	pages := map[string]string{"index.mdx": renderDocsIndex(commands)}
	for _, cmd := range commands {
		pages[cmd.Name()+".mdx"] = renderCommandPage(cmd)
	}
	for name, content := range pages {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
			return err
		}
	}
	return nil
}

func renderDocsIndex(commands []*cobra.Command) string {
	var b strings.Builder
	b.WriteString("---\ntitle: Commands\ndescription: Every sloptor command, generated from the CLI.\nsidebar:\n  order: 0\n  label: Overview\n---\n\n")
	b.WriteString("| Command | Summary |\n| --- | --- |\n")
	for _, cmd := range commands {
		fmt.Fprintf(&b, "| [`%s`](./%s/) | %s |\n", cmd.Name(), cmd.Name(), docsCell(cmd.Short))
	}
	return b.String()
}

func renderCommandPage(cmd *cobra.Command) string {
	var b strings.Builder
	fmt.Fprintf(&b, "---\ntitle: %s\ndescription: %s\n---\n\n", cmd.Name(), strconv.Quote(sentence(cmd.Short)))
	renderCommandBody(&b, cmd, 2)
	if inherited := docsFlags(cmd.InheritedFlags()); len(inherited) > 0 {
		b.WriteString("## Global options\n\n")
		renderFlagTable(&b, cmd, inherited)
	}
	return strings.TrimRight(b.String(), "\n") + "\n"
}

// sentence turns a lowercase help summary into prose: a capital first letter
// and a closing period, so the docs page reads as a sentence.
func sentence(summary string) string {
	summary = strings.TrimSpace(summary)
	if summary == "" {
		return summary
	}
	summary = strings.ToUpper(summary[:1]) + summary[1:]
	if !strings.HasSuffix(summary, ".") {
		summary += "."
	}
	return summary
}

// renderCommandBody writes a command's usage, description, options, examples,
// and subcommands. level is the Markdown heading depth for its own sections;
// each subcommand nests one level deeper so the page outline mirrors the tree.
func renderCommandBody(b *strings.Builder, cmd *cobra.Command, level int) {
	fmt.Fprintf(b, "```sh\n%s\n```\n\n", cmd.UseLine())
	description := cmd.Long
	if description == "" {
		description = cmd.Short
	}
	if description != "" {
		fmt.Fprintf(b, "%s\n\n", escapeMDX(sentence(description)))
	}

	heading := strings.Repeat("#", min(level, 6))
	cmd.InitDefaultHelpFlag()
	useHouseHelpFlagUsage(cmd)
	if flags := visibleFlags(cmd); len(flags) > 0 {
		fmt.Fprintf(b, "%s Options\n\n", heading)
		renderFlagTable(b, cmd, flags)
	}

	if cmd.Example != "" {
		fmt.Fprintf(b, "%s Examples\n\n```sh\n", heading)
		for _, line := range strings.Split(strings.TrimRight(cmd.Example, "\n"), "\n") {
			fmt.Fprintf(b, "%s\n", strings.TrimSpace(line))
		}
		b.WriteString("```\n\n")
	}

	if children := visibleCommands(cmd); len(children) > 0 {
		fmt.Fprintf(b, "%s Subcommands\n\n", heading)
		for _, child := range children {
			fmt.Fprintf(b, "%s# %s\n\n", heading, child.Name())
			renderCommandBody(b, child, level+2)
		}
	}
}

func renderFlagTable(b *strings.Builder, cmd *cobra.Command, flags []*pflag.Flag) {
	b.WriteString("| Flag | Description | Default |\n| --- | --- | --- |\n")
	for _, f := range flags {
		def := ""
		if !isZeroFlagDefault(f) {
			def = "`" + f.DefValue + "`"
		}
		syntax := flagSyntax(&term.Styler{}, cmd, f)
		fmt.Fprintf(b, "| %s | %s | %s |\n", docsCell("`"+syntax+"`"), docsCell(f.Usage), docsCell(def))
	}
	b.WriteString("\n")
}

// docsFlags collects the visible flags of a set in registration order, the
// same filter visibleFlags applies to a command's own flags.
func docsFlags(set *pflag.FlagSet) []*pflag.Flag {
	var out []*pflag.Flag
	set.VisitAll(func(f *pflag.Flag) {
		if !f.Hidden {
			out = append(out, f)
		}
	})
	return out
}

func isZeroFlagDefault(f *pflag.Flag) bool {
	switch f.DefValue {
	case "", "false", "0", "[]":
		return true
	}
	return false
}

// docsCell makes text safe inside a Markdown table cell: MDX-escaped outside
// code spans, pipes escaped everywhere (GFM splits cells on `|` even inside
// backticks), and newlines folded to spaces.
func docsCell(text string) string {
	text = strings.ReplaceAll(escapeMDX(text), "|", `\|`)
	return strings.Join(strings.Fields(text), " ")
}

// mdxEscaper replaces the characters MDX would parse as JSX or expressions.
var mdxEscaper = strings.NewReplacer("<", "&lt;", ">", "&gt;", "{", "&#123;", "}", "&#125;")

// escapeMDX escapes JSX/expression syntax outside inline code spans; text
// between backticks is literal in MDX and is left untouched. A trailing
// unmatched backtick opens no code span, so the text after it is escaped too.
func escapeMDX(text string) string {
	segments := strings.Split(text, "`")
	for i := 0; i < len(segments); i += 2 {
		segments[i] = mdxEscaper.Replace(segments[i])
	}
	if last := len(segments) - 1; last%2 == 1 {
		segments[last] = mdxEscaper.Replace(segments[last])
	}
	return strings.Join(segments, "`")
}
