package compile

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"rotor/tsgo/sourcemap"
)

// compileWithSourceMap builds a package fixture and reads the emitted artifacts.
// name is the source file's stem, without its .ts extension.
func compileWithSourceMap(t *testing.T, name, source string) (luau, mapJSON string) {
	t.Helper()
	dir := writeProject(t, "@scope/source-map-"+name, "")
	sourcePath := filepath.Join(dir, "src", name+".ts")
	if name != "main" {
		if err := os.Rename(filepath.Join(dir, "src", "main.ts"), sourcePath); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(sourcePath, []byte(source), 0o644); err != nil {
		t.Fatal(err)
	}
	tsconfigPath := filepath.Join(dir, "tsconfig.json")
	tsconfig, err := os.ReadFile(tsconfigPath)
	if err != nil {
		t.Fatal(err)
	}
	withMaps := strings.Replace(string(tsconfig), `"outDir": "out"`, `"outDir": "out", "sourceMap": true`, 1)
	if err := os.WriteFile(tsconfigPath, []byte(withMaps), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, diags, err := BuildProjectWithOptions(dir, ProjectOptions{}); err != nil || len(diags) != 0 {
		t.Fatalf("BuildProjectWithOptions: %v (diagnostics: %v)", err, diags)
	}
	read := func(suffix string) string {
		t.Helper()
		text, err := os.ReadFile(filepath.Join(dir, "out", name+suffix))
		if err != nil {
			t.Fatal(err)
		}
		return string(text)
	}
	return read(".luau"), read(".luau.map")
}

// annotateSourceMap uses 1-based lines and 0-based UTF-16 columns. It retains
// every segment, including generated-only segments and duplicate mappings.
func annotateSourceMap(luauText, mapJSON string) string {
	var sourceMap struct {
		Mappings string `json:"mappings"`
	}
	if err := json.Unmarshal([]byte(mapJSON), &sourceMap); err != nil {
		panic(fmt.Errorf("decode source map: %w", err))
	}
	segments := map[int][]string{}
	decoder := sourcemap.DecodeMappings(sourceMap.Mappings)
	for mapping := range decoder.Values() {
		segment := fmt.Sprintf("%d->?", mapping.GeneratedCharacter)
		if mapping.IsSourceMapping() {
			segment = fmt.Sprintf("%d->%d:%d", mapping.GeneratedCharacter, mapping.SourceLine+1, mapping.SourceCharacter)
		}
		segments[mapping.GeneratedLine] = append(segments[mapping.GeneratedLine], segment)
	}
	if err := decoder.Error(); err != nil {
		panic(fmt.Errorf("decode source map mappings: %w", err))
	}
	lines := strings.Split(strings.TrimSuffix(luauText, "\n"), "\n")
	annotations := make([]string, len(lines))
	segmentWidth := 1
	for line := range lines {
		annotation := strings.Join(segments[line], " ")
		if annotation == "" {
			annotation = "-"
		}
		annotations[line] = annotation
		segmentWidth = max(segmentWidth, len(annotation))
	}
	lineWidth := max(2, len(fmt.Sprint(len(lines))))
	var annotated strings.Builder
	for line, text := range lines {
		if strings.HasPrefix(text, "-- Compiled with sloptor v") {
			text = "-- Compiled with sloptor vX"
		}
		fmt.Fprintf(&annotated, "%*d %-*s | %s\n", lineWidth, line+1, segmentWidth, annotations[line], text)
	}
	return annotated.String()
}

func TestSourceMapLines(t *testing.T) {
	tests := []struct {
		name   string
		source string
		want   string
	}{
		{
			name: "header",
			source: `declare class Base {}
export let current: unknown = undefined;
export function isBase(value: unknown) {
	return value instanceof Base;
}
`,
			want: ` 1 -      | -- Compiled with sloptor vX
 2 -      | local TS = _G[script]
 3 -      | local exports = {}
 4 0->2:0 | exports.current = nil
 5 0->3:0 | local function isBase(value)
 6 0->4:1 | 	return TS.instanceof(value, Base)
 7 0->5:0 | end
 8 0->3:0 | exports.isBase = isBase
 9 -      | return exports
`,
		},
		{
			name: "call-args",
			source: `declare function register(callback: () => void): number;
declare function use(value: unknown): void;

register(() => {
	use(1);
});
const handle = register(() => {
	use(2);
});
use(handle);
`,
			want: ` 1 -       | -- Compiled with sloptor vX
 2 0->4:0  | register(function()
 3 0->5:1  | 	use(1)
 4 0->6:0  | end)
 5 0->7:0  | local handle = register(function()
 6 0->8:1  | 	use(2)
 7 0->9:0  | end)
 8 0->10:0 | use(handle)
 9 -       | return nil
`,
		},
		{
			name: "method-call",
			source: `declare const signal: { Connect(callback: () => void): void };
declare function wrap(callback: () => void): () => void;

signal.Connect(() => {
	print("fired");
});
print(wrap(() => {
	print("inner");
}));
`,
			want: ` 1 -      | -- Compiled with sloptor vX
 2 0->4:0 | signal:Connect(function()
 3 0->5:1 | 	print("fired")
 4 0->6:0 | end)
 5 0->7:0 | print(wrap(function()
 6 0->8:1 | 	print("inner")
 7 0->9:0 | end))
 8 -      | return nil
`,
		},
		{
			name: "duplicate-callbacks",
			source: `declare function both(first: () => void, second: () => void): void;
both(() => {
	print("same");
}, () => {
	print("same");
});
`,
			want: ` 1 -      | -- Compiled with sloptor vX
 2 0->2:0 | both(function()
 3 0->3:1 | 	print("same")
 4 0->4:0 | end, function()
 5 0->5:1 | 	print("same")
 6 0->6:0 | end)
 7 -      | return nil
`,
		},
		{
			name: "returned-function",
			source: `export function makeCounter() {
	let count = 0;
	return () => {
		count += 1;
		return count;
	};
}
`,
			want: ` 1 -      | -- Compiled with sloptor vX
 2 0->1:0 | local function makeCounter()
 3 0->2:1 | 	local count = 0
 4 0->3:1 | 	return function()
 5 0->4:2 | 		count += 1
 6 0->5:2 | 		return count
 7 0->6:1 | 	end
 8 0->7:0 | end
 9 -      | return {
10 0->1:0 | 	makeCounter = makeCounter,
11 -      | }
`,
		},
		{
			name: "object-methods",
			source: `export function makeApi() {
	return {
		greet(name: string) {
			return "hi " + name;
		},
		run: () => {
			print("run");
		},
	};
}
`,
			want: ` 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function makeApi()
 3 0->2:1  | 	return {
 4 0->3:2  | 		greet = function(self, name)
 5 0->4:3  | 			return "hi " .. name
 6 0->5:2  | 		end,
 7 0->6:7  | 		run = function()
 8 0->7:3  | 			print("run")
 9 0->8:2  | 		end,
10 0->2:1  | 	}
11 0->10:0 | end
12 -       | return {
13 0->1:0  | 	makeApi = makeApi,
14 -       | }
`,
		},
		{
			name: "concise-arrow",
			source: `declare function register(callback: () => unknown): number;
declare function use(value: unknown): unknown;
register(() => use(1));
`,
			want: ` 1 -       | -- Compiled with sloptor vX
 2 0->3:0  | register(function()
 3 0->3:15 | 	return use(1)
 4 0->3:20 | end)
 5 -       | return nil
`,
		},
		{
			name: "loops",
			source: `export function spin(limit: number) {
	let i = 0;
	do {
		i += 1;
	} while (i < limit);
	return i;
}
`,
			want: ` 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function spin(limit)
 3 0->2:1  | 	local i = 0
 4 0->3:1  | 	repeat
 5 0->3:1  | 		do
 6 0->4:2  | 			i += 1
 7 0->3:1  | 		end
 8 0->5:20 | 	until not (i < limit)
 9 0->6:1  | 	return i
10 0->7:0  | end
11 -       | return {
12 0->1:0  | 	spin = spin,
13 -       | }
`,
		},
		{
			name: "elseif",
			source: `export function sign(n: number) {
	if (n > 0) {
		return 1;
	} else if (n < 0) {
		return -1;
	} else {
		return 0;
	}
}
`,
			want: ` 1 -      | -- Compiled with sloptor vX
 2 0->1:0 | local function sign(n)
 3 0->2:1 | 	if n > 0 then
 4 0->3:2 | 		return 1
 5 0->4:8 | 	elseif n < 0 then
 6 0->5:2 | 		return -1
 7 0->4:8 | 	else
 8 0->7:2 | 		return 0
 9 0->8:1 | 	end
10 0->9:0 | end
11 -      | return {
12 0->1:0 | 	sign = sign,
13 -      | }
`,
		},
		{
			name: "else-if-prereq",
			source: `export function label(a: string | undefined, b: string) {
	if (a === "x") {
		return 1;
	} else if ((a ?? b) === "y") {
		return 2;
	}
	return 0;
}
`,
			want: ` 1 -      | -- Compiled with sloptor vX
 2 0->1:0 | local function label(a, b)
 3 0->2:1 | 	if a == "x" then
 4 0->3:2 | 		return 1
 5 0->2:1 | 	else
 6 0->4:8 | 		local _condition = a
 7 0->4:8 | 		if _condition == nil then
 8 0->4:8 | 			_condition = b
 9 0->6:1 | 		end
10 0->4:8 | 		if _condition == "y" then
11 0->5:2 | 			return 2
12 0->6:1 | 		end
13 0->6:1 | 	end
14 0->7:1 | 	return 0
15 0->8:0 | end
16 -      | return {
17 0->1:0 | 	label = label,
18 -      | }
`,
		},
		{
			name: "switch",
			source: `export function kind(n: number) {
	let result = "none";
	switch (n) {
		case 1:
			result = "one";
		case 2:
			result = "low";
			break;
		default:
			result = "high";
	}
	return result;
}
`,
			want: ` 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function kind(n)
 3 0->2:1  | 	local result = "none"
 4 0->3:1  | 	repeat
 5 0->3:1  | 		local _fallthrough = false
 6 0->4:2  | 		if n == 1 then
 7 0->5:3  | 			result = "one"
 8 0->4:2  | 			_fallthrough = true
 9 0->5:17 | 		end
10 0->6:2  | 		if _fallthrough or n == 2 then
11 0->7:3  | 			result = "low"
12 0->8:3  | 			break
13 0->8:8  | 		end
14 0->10:3 | 		result = "high"
15 0->11:1 | 	until true
16 0->12:1 | 	return result
17 0->13:0 | end
18 -       | return {
19 0->1:0  | 	kind = kind,
20 -       | }
`,
		},
		{
			name: "try",
			source: `declare function risky(): void;
export function attempt() {
	try {
		risky();
	} catch (e) {
		print(e);
	} finally {
		print("done");
	}
}
`,
			want: ` 1 -       | -- Compiled with sloptor vX
 2 -       | local TS = _G[script]
 3 0->2:0  | local function attempt()
 4 0->3:1  | 	TS.try(function()
 5 0->4:2  | 		risky()
 6 0->5:1  | 	end, function(e)
 7 0->6:2  | 		print(e)
 8 0->7:1  | 	end, function()
 9 0->8:2  | 		print("done")
10 0->9:1  | 	end)
11 0->10:0 | end
12 -       | return {
13 0->2:0  | 	attempt = attempt,
14 -       | }
`,
		},
		{
			name: "parameters",
			source: `export function greet(name = "world", { loud }: { loud: boolean }) {
	print(name, loud);
}
`,
			want: ` 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function greet(name, _param)
 3 0->1:22 | 	if name == nil then
 4 0->1:22 | 		name = "world"
 5 0->1:35 | 	end
 6 0->1:38 | 	local loud = _param.loud
 7 0->2:1  | 	print(name, loud)
 8 0->3:0  | end
 9 -       | return {
10 0->1:0  | 	greet = greet,
11 -       | }
`,
		},
		{
			name: "class-methods",
			source: `export class Counter {
	private count = 0;
	increment(step = 1) {
		this.count += step;
		return this.count;
	}
}
`,
			want: ` 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local Counter
 3 0->1:0  | do
 4 0->1:0  | 	Counter = setmetatable({}, {
 5 0->1:0  | 		__tostring = function()
 6 0->1:0  | 			return "Counter"
 7 0->1:0  | 		end,
 8 0->1:0  | 	})
 9 0->1:0  | 	Counter.__index = Counter
10 0->1:0  | 	function Counter.new(...)
11 0->1:0  | 		local self = setmetatable({}, Counter)
12 0->1:0  | 		return self:constructor(...) or self
13 0->1:0  | 	end
14 0->1:0  | 	function Counter:constructor()
15 0->2:1  | 		self.count = 0
16 0->1:0  | 	end
17 0->3:1  | 	function Counter:increment(step)
18 0->3:11 | 		if step == nil then
19 0->3:11 | 			step = 1
20 0->3:18 | 		end
21 0->4:2  | 		self.count += step
22 0->5:2  | 		return self.count
23 0->6:1  | 	end
24 0->7:0  | end
25 -       | return {
26 0->1:0  | 	Counter = Counter,
27 -       | }
`,
		},
		{
			name: "class-ctor-static",
			source: `export class Point {
	static origin = 0;
	static {
		print("init");
	}
	constructor(public x: number) {
		print(x);
	}
}
`,
			want: ` 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local Point
 3 0->1:0  | do
 4 0->1:0  | 	Point = setmetatable({}, {
 5 0->1:0  | 		__tostring = function()
 6 0->1:0  | 			return "Point"
 7 0->1:0  | 		end,
 8 0->1:0  | 	})
 9 0->1:0  | 	Point.__index = Point
10 0->1:0  | 	function Point.new(...)
11 0->1:0  | 		local self = setmetatable({}, Point)
12 0->1:0  | 		return self:constructor(...) or self
13 0->1:0  | 	end
14 0->6:1  | 	function Point:constructor(x)
15 0->6:13 | 		self.x = x
16 0->7:2  | 		print(x)
17 0->8:1  | 	end
18 0->2:1  | 	Point.origin = 0
19 0->3:1  | 	do
20 0->4:2  | 		print("init")
21 0->5:1  | 	end
22 0->9:0  | end
23 -       | return {
24 0->1:0  | 	Point = Point,
25 -       | }
`,
		},
		{
			name: "nullish",
			source: `export function pick(a: string | undefined, b: string) {
	const value = a ?? b;
	return value;
}
`,
			want: ` 1 -       | -- Compiled with sloptor vX
 2 0->1:0  | local function pick(a, b)
 3 0->2:1  | 	local _condition = a
 4 0->2:1  | 	if _condition == nil then
 5 0->2:1  | 		_condition = b
 6 0->2:21 | 	end
 7 0->2:1  | 	local value = _condition
 8 0->3:1  | 	return value
 9 0->4:0  | end
10 -       | return {
11 0->1:0  | 	pick = pick,
12 -       | }
`,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			luau, mapJSON := compileWithSourceMap(t, test.name, test.source)
			if got := annotateSourceMap(luau, mapJSON); got != test.want {
				t.Errorf("source map lines:\ngot:\n%swant:\n%s", got, test.want)
			}
		})
	}
}
