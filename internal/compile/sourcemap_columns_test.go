package compile

import "testing"

func TestSourceMapColumns(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{
			name:   "unicode-columns",
			source: "const face = \"😀\"; print(\"😀\", face);\n",
			want: ` 1 -                        | -- Compiled with sloptor vX
 2 0->1:0 13->1:13          | local face = "😀"
 3 0->1:19 6->1:25 12->1:31 | print("😀", face)
 4 -                        | return nil
`,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			text, sourceMap := compileWithSourceMap(t, test.name, test.source)
			if got := annotateSourceMap(text, sourceMap); got != test.want {
				t.Errorf("source map columns:\ngot:\n%swant:\n%s", got, test.want)
			}
		})
	}
}
