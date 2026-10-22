package compile

import "testing"

func TestSourceMapColumns(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{
			name: "continuation",
			source: `const config = {
	retries: 3,
	label: "x",
};
print(config);
`,
			want: ` 1 -               | -- Compiled with sloptor vX
 2 0->1:0 15->1:15 | local config = {
 3 0->2:1 11->2:10 | 	retries = 3,
 4 0->3:1 9->3:8   | 	label = "x",
 5 0->1:15         | }
 6 0->5:0 6->5:6   | print(config)
 7 -               | return nil
`,
		},
		{
			name: "call-index",
			source: `declare const store: { get(key: string): number; nested: { value: number } };
export function read(key: string) {
	return store.get(key) +
		store.nested.value;
}
`,
			want: ` 1 -                                       | -- Compiled with sloptor vX
 2 0->2:0                                  | local function read(key)
 3 0->3:1 8->3:8 18->3:18 23->3:23 25->4:2 | 	return store:get(key) + store.nested.value
 4 0->5:0                                  | end
 5 -                                       | return {
 6 0->2:0                                  | 	read = read,
 7 -                                       | }
`,
		},
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
