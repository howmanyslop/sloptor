package compile

import "testing"

func TestSourceMapColumns(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{
			name: "operators",
			source: `export function check(a: boolean, b: boolean, c: boolean) {
	const ok = a &&
		b ||
		c;
	return ok;
}
`,
			want: ` 1 -                                                | -- Compiled with sloptor vX
 2 0->1:0                                           | local function check(a, b, c)
 3 0->2:1 12->2:12 14->2:14 18->3:2 20->3:4 23->4:2 | 	local ok = a and b or c
 4 0->5:1 8->5:8                                    | 	return ok
 5 0->6:0                                           | end
 6 -                                                | return {
 7 0->1:0                                           | 	check = check,
 8 -                                                | }
`,
		},
		{
			name: "nullish",
			source: `export function pick(a: string | undefined, b: string) {
	const value = a ?? b;
	return value;
}
`,
			want: ` 1 -               | -- Compiled with sloptor vX
 2 0->1:0          | local function pick(a, b)
 3 0->2:1 20->2:15 | 	local _condition = a
 4 0->2:1 4->2:17  | 	if _condition == nil then
 5 0->2:20         | 		_condition = b
 6 0->2:21         | 	end
 7 0->2:1 15->2:15 | 	local value = _condition
 8 0->3:1 8->3:8   | 	return value
 9 0->4:0          | end
10 -               | return {
11 0->1:0          | 	pick = pick,
12 -               | }
`,
		},
		{
			name: "temporaries",
			source: `export function bump(state: { value: number }) {
	const previous = state.value++;
	return previous;
}
`,
			want: ` 1 -                       | -- Compiled with sloptor vX
 2 0->1:0                  | local function bump(state)
 3 0->2:1 19->2:18         | 	local _original = state.value
 4 0->2:1 1->2:18 13->2:29 | 	state.value += 1
 5 0->2:1 18->2:18         | 	local previous = _original
 6 0->3:1 8->3:8           | 	return previous
 7 0->4:0                  | end
 8 -                       | return {
 9 0->1:0                  | 	bump = bump,
10 -                       | }
`,
		},
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
