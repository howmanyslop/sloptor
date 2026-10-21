package checker_test

import (
	"context"
	"crypto/sha256"
	"fmt"
	"slices"
	"strings"
	"testing"

	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/checker"
	"rotor/tsgo/compiler"
	"rotor/tsgo/core"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/vfs/vfstest"
)

const unionFlattenMainPath = "/project/main.ts"

// unionFlattenMixedSource mixes plain string literals with other kinds of types and has
// intentional errors whose messages print flattened unions. "~" stands for a backtick.
var unionFlattenMixedSource = strings.ReplaceAll(`
enum Color { Red = "k001", Green = "k050", Blue = "zz" }
type Colors = Color;
interface Box { box: 1 }
type Tpl = ~k${number}~;
type Mixed = undefined | null | "k001" | "k100" | 1 | 2 | Color | Tpl | Box | "k150" | true | 10n;
type MixedNoNull = "k002" | Color.Red | "k003" | 3 | Box | void;
type Low = "a" | "c" | "e";
type High = "b" | "c" | "d";
interface Shape { kind: Low; size: High | 1 | undefined; tag: ~x${string}~ | "x1" | "y1" }
type Picked = Shape["kind" | "size"];
type Keys = keyof Shape;
type Mapped = { [K in Low | High]: ~m_${K}~ };
type MappedValues = Mapped[keyof Mapped];
type Spread = Low | High | Even | Alpha;
declare const pick: Low | High;
declare const maybe: Low | High | undefined | null;
declare const picked: Picked;
declare const mapped: MappedValues;
const bad1: "zz" = pick;
const bad2: "zz" = maybe;
const bad3: "zz" = picked;
const bad4: "zz" = mapped;
function choose<T extends Low | High>(value: T): T { return value; }
const chosen = choose("zz");
function exhaust(s: Shape["kind"]): number {
    switch (s) {
        case "a": return 1;
        case "c": return 2;
        default: { const rest: never = s; return rest; }
    }
}
const widened = Math.random() > 0.5 ? "k010" : "k011";
let widenedLet = widened;
const literalArray = ["k020", "k021", "k020"] as const;
type ArrayValues = (typeof literalArray)[number];
export {};
`, "~", "`")

// unionFlattenSource returns the fixture program. Its large unions of plain string literals
// are the shape of the observed hot path: flattening one union into a set of string literals.
func unionFlattenSource() string {
	var b strings.Builder
	writeLiterals := func(name string, values []string) {
		fmt.Fprintf(&b, "type %s =", name)
		for _, value := range values {
			fmt.Fprintf(&b, " | %q", value)
		}
		b.WriteString(";\n")
	}
	odd := numberedLiterals("k", 1, 200, 2)
	slices.Reverse(odd)
	writeLiterals("Big", numberedLiterals("k", 0, 200, 1))
	writeLiterals("BigCopy", numberedLiterals("k", 0, 200, 1))
	writeLiterals("Even", numberedLiterals("k", 0, 200, 2))
	writeLiterals("Odd", odd)
	writeLiterals("Alpha", numberedLiterals("a", 0, 100, 1))
	writeLiterals("Beta", numberedLiterals("b", 0, 100, 1))
	for i := range 30 {
		writeLiterals(fmt.Sprintf("SmallSeq%02d", i), numberedLiterals(fmt.Sprintf("s%02d_", i), 0, 3, 1))
		writeLiterals(fmt.Sprintf("SmallMix%02d", i), []string{fmt.Sprintf("t%03d", i), fmt.Sprintf("t%03d", i+30), fmt.Sprintf("t%03d", i+60)})
	}
	b.WriteString(unionFlattenMixedSource)
	return b.String()
}

func numberedLiterals(prefix string, from, to, step int) []string {
	var values []string
	for i := from; i < to; i += step {
		values = append(values, fmt.Sprintf("%s%03d", prefix, i))
	}
	return values
}

type unionFlattenFixture struct {
	file        *ast.SourceFile
	checker     *checker.Checker
	diagnostics []*ast.Diagnostic
	names       []string
	aliases     map[string]*checker.Type
}

func newUnionFlattenFixture(tb testing.TB, strict bool) *unionFlattenFixture {
	tb.Helper()
	config := fmt.Sprintf(`{"compilerOptions":{"strict":%t,"target":"es2022","lib":["es2022"],"noEmit":true},"files":["main.ts"]}`, strict)
	fs := bundled.WrapFS(vfstest.FromMap(map[string]string{
		"/project/tsconfig.json": config,
		unionFlattenMainPath:     unionFlattenSource(),
	}, true))
	host := compiler.NewCompilerHost("/project", fs, bundled.LibPath(), nil, nil)
	parsed, configDiagnostics := tsoptions.GetParsedCommandLineOfConfigFile("/project/tsconfig.json", nil, nil, host, nil)
	if len(configDiagnostics) != 0 {
		tb.Fatalf("config diagnostic: %s", configDiagnostics[0].String())
	}
	program := compiler.NewProgram(compiler.ProgramOptions{Host: host, Config: parsed, SingleThreaded: core.TSTrue})
	file := program.GetSourceFile(unionFlattenMainPath)
	if file == nil {
		tb.Fatal("main.ts is not in the program")
	}
	ctx := context.Background()
	f := &unionFlattenFixture{file: file, aliases: map[string]*checker.Type{}}
	f.diagnostics = program.GetSemanticDiagnostics(ctx, file)
	c, release := program.GetTypeChecker(ctx)
	tb.Cleanup(release)
	f.checker = c
	for _, statement := range file.Statements.Nodes {
		if statement.Kind == ast.KindTypeAliasDeclaration {
			name := statement.Name().Text()
			f.names = append(f.names, name)
			f.aliases[name] = c.GetDeclaredTypeOfSymbol(statement.Symbol())
		}
	}
	return f
}

func (f *unionFlattenFixture) types(tb testing.TB, names ...string) []*checker.Type {
	tb.Helper()
	types := make([]*checker.Type, len(names))
	for i, name := range names {
		types[i] = f.aliases[name]
		if types[i] == nil {
			tb.Fatalf("type alias %s is not in the fixture", name)
		}
	}
	return types
}

func (f *unionFlattenFixture) prefixed(prefix string) []string {
	var names []string
	for _, name := range f.names {
		if strings.HasPrefix(name, prefix) {
			names = append(names, name)
		}
	}
	return names
}

type unionFlattenCase struct {
	name  string
	start []string // aliases whose baseline flatten is the starting set
	types []*checker.Type
}

func (f *unionFlattenFixture) cases(tb testing.TB) []unionFlattenCase {
	tb.Helper()
	c := f.checker
	big := f.types(tb, "Big")[0].Types()
	even := f.types(tb, "Even")[0].Types()
	freshEven := make([]*checker.Type, 0, 20)
	for _, t := range even[:20] {
		freshEven = append(freshEven, checker.FreshLiteralTypeForTest(c, t))
	}
	// UnionReductionNone keeps the fresh and the regular type of the same value apart.
	freshAndRegular := c.GetUnionTypeEx(slices.Concat(freshEven, big[:30]), checker.UnionReductionNone)
	return []unionFlattenCase{
		{name: "empty input"},
		{name: "single literal", types: []*checker.Type{big[7]}},
		{name: "one union into empty set", types: f.types(tb, "Big")},
		{name: "disjoint", types: f.types(tb, "Alpha", "Beta")},
		{name: "interleaved", types: f.types(tb, "Even", "Odd")},
		{name: "duplicate heavy", types: f.types(tb, "Big", "BigCopy", "Even", "Odd", "Big")},
		{name: "consecutive duplicate union", types: f.types(tb, "Even", "Even", "Odd")},
		{name: "small sequential unions", types: f.types(tb, f.prefixed("SmallSeq")...)},
		{name: "small interleaved unions", types: f.types(tb, f.prefixed("SmallMix")...)},
		{name: "mixed kinds", types: f.types(tb, "Mixed", "MixedNoNull", "Low", "High", "Even", "Tpl")},
		{name: "enum literals with plain literal values", types: f.types(tb, "Colors", "Odd", "MixedNoNull", "Keys")},
		{name: "nullable and never", types: []*checker.Type{c.GetUndefinedType(), f.types(tb, "Low")[0], c.GetNullType(), c.GetNeverType(), f.types(tb, "High")[0], f.types(tb, "Mixed")[0], c.GetUndefinedType()}},
		{name: "fresh and regular literals", types: []*checker.Type{freshAndRegular, f.types(tb, "Odd")[0], freshEven[3]}},
		{name: "scalars around unions", types: slices.Concat([]*checker.Type{big[199], big[0]}, f.types(tb, "Odd", "Alpha"), []*checker.Type{freshEven[1]})},
		{name: "derived unions", types: f.types(tb, "Picked", "MappedValues", "ArrayValues", "Spread", "Keys")},
		{name: "nonempty start", start: []string{"Mixed", "Alpha"}, types: f.types(tb, "Even", "Odd", "Beta")},
	}
}

type unionFlattenOrder struct {
	name  string
	types []*checker.Type
}

func unionFlattenOrders(types []*checker.Type) []unionFlattenOrder {
	reversed := slices.Clone(types)
	slices.Reverse(reversed)
	rotated := slices.Clone(types)
	if len(types) > 1 {
		rotated = append(slices.Clone(types[1:]), types[0])
	}
	return []unionFlattenOrder{
		{name: "given", types: types},
		{name: "reversed", types: reversed},
		{name: "rotated", types: rotated},
		{name: "repeated", types: slices.Concat(types, types)},
	}
}

func describeTypes(c *checker.Checker, types []*checker.Type) string {
	parts := make([]string, len(types))
	for i, t := range types {
		parts[i] = fmt.Sprintf("%s#%d", c.TypeToString(t), t.Id())
	}
	return strings.Join(parts, " | ")
}

func TestAddTypesToUnion_matchesBaselineForFlattenedUnions(t *testing.T) {
	for _, strict := range []bool{true, false} {
		t.Run(fmt.Sprintf("strict=%t", strict), func(t *testing.T) {
			// Given: real types from a checked program and, for some cases, a nonempty set.
			f := newUnionFlattenFixture(t, strict)
			for _, test := range f.cases(t) {
				start, startIncludes := checker.AddTypesToUnionForTest(f.checker, nil, 0, f.types(t, test.start...), true)
				for _, order := range unionFlattenOrders(test.types) {
					t.Run(test.name+"/"+order.name, func(t *testing.T) {
						// When: the current helper and the retained baseline add the same input.
						got, gotIncludes := checker.AddTypesToUnionForTest(f.checker, start, startIncludes, order.types, false)
						want, wantIncludes := checker.AddTypesToUnionForTest(f.checker, start, startIncludes, order.types, true)

						// Then: both give the same types in the same order and the same includes.
						if !slices.Equal(got, want) {
							t.Fatalf("type set differs\ngot:  %s\nwant: %s", describeTypes(f.checker, got), describeTypes(f.checker, want))
						}
						if gotIncludes != wantIncludes {
							t.Fatalf("includes = %v, want %v", gotIncludes, wantIncludes)
						}
					})
				}
			}
		})
	}
}

func TestAddTypesToUnion_flattensInterleavedLiteralsInValueOrder(t *testing.T) {
	// Given: Even and Odd hold alternate members of Big, which are plain string literals.
	f := newUnionFlattenFixture(t, true)
	big := f.types(t, "Big")[0].Types()
	if len(big) != 200 {
		t.Fatalf("Big has %d members, want 200", len(big))
	}
	for i, member := range big {
		if member.Flags() != checker.TypeFlagsStringLiteral || member.Alias() != nil {
			t.Fatalf("Big member %d is not a plain string literal: %s", i, describeTypes(f.checker, []*checker.Type{member}))
		}
		if want := fmt.Sprintf("k%03d", i); member.AsLiteralType().Value() != want {
			t.Fatalf("Big member %d = %v, want %q", i, member.AsLiteralType().Value(), want)
		}
	}
	for _, input := range [][]string{{"Even", "Odd"}, {"Odd", "Even"}, {"Big", "Even"}, {"Odd", "Big"}} {
		// When: the current helper flattens them.
		got, includes := checker.AddTypesToUnionForTest(f.checker, nil, 0, f.types(t, input...), false)

		// Then: the set is Big's list in value order, and only the union and string literal includes are set.
		if !slices.Equal(got, big) {
			t.Fatalf("%v flattened to %s", input, describeTypes(f.checker, got))
		}
		if want := checker.TypeFlagsUnion | checker.TypeFlagsStringLiteral; includes != want {
			t.Fatalf("%v includes = %v, want %v", input, includes, want)
		}
	}
}

func TestGetUnionType_matchesBaselineFlattenForRealQueries(t *testing.T) {
	for _, strict := range []bool{true, false} {
		t.Run(fmt.Sprintf("strict=%t", strict), func(t *testing.T) {
			// Given: real types from a checked program.
			f := newUnionFlattenFixture(t, strict)
			for _, test := range f.cases(t) {
				if len(test.start) != 0 || len(test.types) == 0 {
					continue
				}
				for _, order := range unionFlattenOrders(test.types) {
					t.Run(test.name+"/"+order.name, func(t *testing.T) {
						// When: the checker builds the union without reductions.
						result := f.checker.GetUnionTypeEx(order.types, checker.UnionReductionNone)

						// Then: its constituents are the baseline flatten of the same input.
						want, _ := checker.AddTypesToUnionForTest(f.checker, nil, 0, order.types, true)
						got := []*checker.Type{result}
						switch {
						case result.Flags()&checker.TypeFlagsUnion != 0:
							got = result.Types()
						case len(want) == 0:
							want = []*checker.Type{f.checker.GetNeverType()}
						}
						if !slices.Equal(got, want) {
							t.Fatalf("union differs\ngot:  %s\nwant: %s", describeTypes(f.checker, got), describeTypes(f.checker, want))
						}
					})
				}
			}
		})
	}
}

func TestUnionFlatten_programParityDigest(t *testing.T) {
	for _, strict := range []bool{true, false} {
		t.Run(fmt.Sprintf("strict=%t", strict), func(t *testing.T) {
			// Given: a checked program with large, mixed and derived unions and intentional errors.
			f := newUnionFlattenFixture(t, strict)

			// When: every alias, top-level variable and diagnostic is printed.
			dump := f.dump()

			// Then: the digest is logged for comparison with a run on the unchanged checker, and
			// the overlapping named unions print as one list in value order.
			t.Logf("union flatten parity strict=%t sha256=%x lines=%d", strict, sha256.Sum256([]byte(dump)), strings.Count(dump, "\n"))
			t.Log(dump)
			if len(f.diagnostics) == 0 {
				t.Fatal("the fixture reported none of its intentional errors")
			}
			if !strings.Contains(dump, `"a" | "b" | "c" | "d" | "e"`) {
				t.Fatal(`no printed type is "a" | "b" | "c" | "d" | "e"`)
			}
		})
	}
}

func (f *unionFlattenFixture) dump() string {
	c := f.checker
	var b strings.Builder
	writeType := func(kind, name string, t *checker.Type) {
		fmt.Fprintf(&b, "%s %s: %s flags=%v", kind, name, c.TypeToString(t), t.Flags())
		if t.Flags()&checker.TypeFlagsUnion != 0 {
			members := make([]string, len(t.Types()))
			for i, member := range t.Types() {
				members[i] = c.TypeToString(member)
			}
			fmt.Fprintf(&b, " members=%s", strings.Join(members, " | "))
		}
		b.WriteByte('\n')
	}
	for _, name := range f.names {
		writeType("type", name, f.aliases[name])
	}
	for _, statement := range f.file.Statements.Nodes {
		if statement.Kind != ast.KindVariableStatement {
			continue
		}
		for _, declaration := range statement.AsVariableStatement().DeclarationList.AsVariableDeclarationList().Declarations.Nodes {
			writeType("var", declaration.Name().Text(), c.GetTypeAtLocation(declaration))
		}
	}
	var writeDiagnostic func(d *ast.Diagnostic, depth int)
	writeDiagnostic = func(d *ast.Diagnostic, depth int) {
		fmt.Fprintf(&b, "%sTS%d %d+%d %s\n", strings.Repeat("  ", depth), d.Code(), d.Pos(), d.Len(), d.String())
		for _, chained := range d.MessageChain() {
			writeDiagnostic(chained, depth+1)
		}
	}
	for _, d := range f.diagnostics {
		writeDiagnostic(d, 0)
	}
	return b.String()
}

func BenchmarkAddTypesToUnion(b *testing.B) {
	// Given: real union types. Building the program is outside the timed loop.
	f := newUnionFlattenFixture(b, true)
	cases := []struct {
		name  string
		types []*checker.Type
	}{
		{name: "one union into empty set", types: f.types(b, "Big")},
		{name: "disjoint", types: f.types(b, "Alpha", "Beta")},
		{name: "interleaved", types: f.types(b, "Even", "Odd")},
		{name: "duplicate heavy", types: f.types(b, "Big", "BigCopy")},
		{name: "small sequential unions", types: f.types(b, f.prefixed("SmallSeq")...)},
		{name: "small interleaved unions", types: f.types(b, f.prefixed("SmallMix")...)},
		{name: "mixed kinds", types: f.types(b, "Mixed", "MixedNoNull", "Low", "High", "Colors")},
	}
	for _, test := range cases {
		for _, arm := range []struct {
			name     string
			baseline bool
		}{{name: "baseline", baseline: true}, {name: "candidate", baseline: false}} {
			b.Run(test.name+"/"+arm.name, func(b *testing.B) {
				b.ReportAllocs()
				for b.Loop() {
					checker.AddTypesToUnionForTest(f.checker, nil, 0, test.types, arm.baseline)
				}
			})
		}
	}
}
