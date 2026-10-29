package transformer

import (
	"context"
	"path/filepath"
	"strings"
	"testing"

	"rotor/internal/luau"
	"rotor/internal/luau/render"
	"rotor/tsgo/ast"
	"rotor/tsgo/bundled"
	"rotor/tsgo/compiler"
	"rotor/tsgo/tsoptions"
	"rotor/tsgo/vfs/osvfs"
)

// buildLuauTypesState builds a program over testdata/luautypes and returns a
// fresh state for relPath. This is the internal-package twin of buildState
// (identifier_test.go): it cannot import internal/compile, so it applies the
// automatic-types default itself and skips the tsconfig sanitizer, which the
// fixture does not need.
func buildLuauTypesState(t *testing.T, relPath string) *State {
	t.Helper()
	dir, err := filepath.Abs(filepath.Join("testdata", "luautypes"))
	if err != nil {
		t.Fatal(err)
	}
	dir = filepath.ToSlash(dir)
	host := compiler.NewCompilerHost(dir, bundled.WrapFS(osvfs.FS()), bundled.LibPath(), nil, nil)
	parsed, configDiags := tsoptions.GetParsedCommandLineOfConfigFile(dir+"/tsconfig.json", nil, nil, host, nil)
	if len(configDiags) > 0 {
		t.Fatalf("config diagnostics: %v", configDiags)
	}
	if options := parsed.CompilerOptions(); options.Types == nil {
		options.Types = []string{"*"}
	}
	program := compiler.NewProgram(compiler.ProgramOptions{Host: host, Config: parsed})
	ctx := context.Background()
	sourceFile := program.GetSourceFile(dir + "/" + filepath.ToSlash(relPath))
	if sourceFile == nil {
		t.Fatalf("source file not in program: %s", relPath)
	}
	for _, diagnostic := range program.GetSemanticDiagnostics(ctx, sourceFile) {
		t.Errorf("unexpected semantic diagnostic: %s", diagnostic.String())
	}
	if t.Failed() {
		t.FailNow()
	}
	chk, release := program.GetTypeChecker(ctx)
	t.Cleanup(release)
	return NewState(program, chk, sourceFile, NewDiagService(), NewMultiState())
}

// typeFixture is one fixture file with its top-level declarations by name.
type typeFixture struct {
	t            *testing.T
	state        *State
	declarations map[string]*ast.Node
}

func loadTypeFixture(t *testing.T, relPath string) *typeFixture {
	t.Helper()
	state := buildLuauTypesState(t, relPath)
	fixture := &typeFixture{t: t, state: state, declarations: make(map[string]*ast.Node)}
	for _, statement := range state.SourceFile.AsNode().Statements() {
		switch {
		case ast.IsVariableStatement(statement):
			for _, declaration := range statement.AsVariableStatement().DeclarationList.AsVariableDeclarationList().Declarations.Nodes {
				fixture.declarations[declaration.Name().Text()] = declaration
			}
		case ast.IsFunctionDeclaration(statement), ast.IsInterfaceDeclaration(statement), ast.IsTypeAliasDeclaration(statement),
			ast.IsEnumDeclaration(statement), ast.IsClassDeclaration(statement):
			if name := statement.Name(); name != nil {
				if _, seen := fixture.declarations[name.Text()]; !seen {
					fixture.declarations[name.Text()] = statement
				}
			}
		}
	}
	return fixture
}

func (f *typeFixture) declaration(name string) *ast.Node {
	f.t.Helper()
	declaration := f.declarations[name]
	if declaration == nil {
		f.t.Fatalf("fixture has no top-level declaration %q", name)
	}
	return declaration
}

func (f *typeFixture) builder() *typeBuilder { return f.state.types() }

// variableType spells a declared variable's type at a local position.
func (f *typeFixture) variableType(name string) builtType {
	declaration := f.declaration(name)
	return f.builder().Build(f.state.Checker.GetTypeAtLocation(declaration.Name()), typeContext{Position: positionLocal, Owner: declaration})
}

// parameterType spells a function parameter's type at a parameter position.
func (f *typeFixture) parameterType(function string, index int) builtType {
	declaration := f.declaration(function)
	parameter := declaration.Parameters()[index]
	return f.builder().Build(f.state.Checker.GetTypeAtLocation(parameter.Name()), typeContext{Position: positionParameter, Owner: declaration})
}

// functionType spells a declared function's signature as a function type.
func (f *typeFixture) functionType(name string) *luau.TypeFunction {
	declaration := f.declaration(name)
	signature := f.state.Checker.GetSignatureFromDeclaration(declaration)
	if signature == nil {
		f.t.Fatalf("no signature for %q", name)
	}
	return f.builder().BuildSignature(signature, nil, typeContext{Owner: declaration})
}

// symbol is the symbol a top-level declaration declares.
func (f *typeFixture) symbol(name string) *ast.Symbol {
	f.t.Helper()
	symbol := f.state.Checker.GetSymbolAtLocation(f.declaration(name).Name())
	if symbol == nil {
		f.t.Fatalf("no symbol for %q", name)
	}
	return symbol
}

// alias registers (on first use) and returns a named type's alias entry.
func (f *typeFixture) alias(name string) *typeAliasEntry {
	f.t.Helper()
	entry := f.state.typeAliasFor(f.symbol(name))
	if entry == nil {
		f.t.Fatalf("%q was not aliased", name)
	}
	return entry
}

// aliasStatement spells a named type's `type` statement.
func (f *typeFixture) aliasStatement(name string) string {
	f.t.Helper()
	return strings.TrimSpace(render.Render(render.NewRenderState(), f.alias(name).Statement))
}

func text(node luau.Node) string {
	return render.Render(render.NewRenderState(), node)
}

// lossesSince reports the loss kinds recorded after the first `from` losses.
func lossesSince(state *State, from int) []fidelityLossKind {
	var kinds []fidelityLossKind
	for _, loss := range state.LuauTypeLosses()[from:] {
		kinds = append(kinds, loss.Kind)
	}
	return kinds
}

func hasLoss(kinds []fidelityLossKind, want fidelityLossKind) bool {
	for _, kind := range kinds {
		if kind == want {
			return true
		}
	}
	return false
}

// typeCase is one declaration and the text the builder must spell for it.
type typeCase struct {
	name string
	want string
	// losses are the distinct loss kinds building the declaration records;
	// nil means none.
	losses []fidelityLossKind
}

// distinctLosses is the set of loss kinds recorded since from, in order of
// first appearance.
func distinctLosses(state *State, from int) []fidelityLossKind {
	var kinds []fidelityLossKind
	for _, kind := range lossesSince(state, from) {
		if !hasLoss(kinds, kind) {
			kinds = append(kinds, kind)
		}
	}
	return kinds
}

func sameLossSet(got, want []fidelityLossKind) bool {
	if len(got) != len(want) {
		return false
	}
	for _, kind := range want {
		if !hasLoss(got, kind) {
			return false
		}
	}
	return true
}

// runTypeCases spells every case through spell and checks text and losses.
func runTypeCases(t *testing.T, fixture *typeFixture, cases []typeCase, spell func(name string) string) {
	t.Helper()
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			from := len(fixture.state.LuauTypeLosses())
			if got := spell(c.name); got != c.want {
				t.Errorf("spelled %q, want %q", got, c.want)
			}
			if got := distinctLosses(fixture.state, from); !sameLossSet(got, c.losses) {
				t.Errorf("losses %v, want %v", got, c.losses)
			}
		})
	}
}

func TestLuauTypesBuilderPrimitives(t *testing.T) {
	fixture := loadTypeFixture(t, "src/primitives.ts")
	runTypeCases(t, fixture, []typeCase{
		{name: "aNumber", want: "number"},
		{name: "aString", want: "string"},
		{name: "aBoolean", want: "boolean"},
		{name: "anAny", want: "any"},
		{name: "anUnknown", want: "unknown"},
		{name: "aNever", want: "never"},
		{name: "anUndefined", want: "nil"},
		{name: "aNull", want: "nil"},
		{name: "aVoid", want: "nil"},
		{name: "aBigInt", want: "number", losses: []fidelityLossKind{lossBigInt}},
		{name: "aSymbol", want: "any", losses: []fidelityLossKind{lossSymbolKey}},
		{name: "anObject", want: "{ [any]: any }"},
		{name: "anEmpty", want: "any"},
		{name: "stringLiteral", want: `"x"`},
		{name: "trueLiteral", want: "true"},
		{name: "falseLiteral", want: "false"},
		{name: "numberLiteral", want: "number", losses: []fidelityLossKind{lossNumberLiteral}},
		{name: "template", want: "string", losses: []fidelityLossKind{lossTemplateLiteral}},
		{name: "escaped", want: `"a\"b\\c\n"`},
		{name: "aVector", want: "vector"},
		{name: "aVector3", want: "Vector3"},
		{name: "aCFrame", want: "CFrame"},
		{name: "aPart", want: "Part"},
		{name: "anInstance", want: "Instance"},
		{name: "aMaterial", want: "Enum.Material"},
		// Luau names the enum, never one of its items.
		{name: "plastic", want: "Enum.Material"},
		{name: "anEnumItem", want: "EnumItem"},
		{name: "aSignal", want: "RBXScriptSignal"},
		{name: "config", want: "{ depth: number }"},
	}, func(name string) string { return text(fixture.variableType(name).Node) })
}

func TestLuauTypesBuilderUnions(t *testing.T) {
	fixture := loadTypeFixture(t, "src/unions.ts")
	runTypeCases(t, fixture, []typeCase{
		{name: "maybeNumber", want: "number?"},
		{name: "nullableString", want: "string?"},
		{name: "numberOrString", want: "string | number"},
		{name: "optionalUnion", want: "(string | number)?"},
		{name: "boolPair", want: "boolean"},
		{name: "boolOrUndefined", want: "boolean?"},
		{name: "literals", want: `"a" | "b"`},
		{name: "numberLiterals", want: "number", losses: []fidelityLossKind{lossNumberLiteral}},
		{name: "mixedLiterals", want: `"a" | number`, losses: []fidelityLossKind{lossNumberLiteral}},
		{name: "fnOrNil", want: "((n: number) -> string)?"},
		{name: "fnOrString", want: "string | ((n: number) -> string)"},
		{name: "tableUnion", want: "{ a: number } | { b: string }"},
		{name: "arrayUnion", want: "{ string } | { number }"},
		{name: "dupes", want: "{ number }"},
		{name: "onlyNil", want: "nil"},
		{name: "tableIntersection", want: "{ a: number } & { b: string }"},
		{name: "emptyIntersection", want: "{ a: number }"},
		{name: "brandedString", want: "any", losses: []fidelityLossKind{lossUnspellable}},
		{name: "overloads", want: "((n: number) -> string) & ((s: string) -> number)"},
		{name: "overloadedAlias", want: "Overloaded"},
	}, func(name string) string { return text(fixture.variableType(name).Node) })

	if got, want := fixture.aliasStatement("Overloaded"), "type Overloaded = ((value: number) -> string) & ((value: string) -> number)"; got != want {
		t.Errorf("Overloaded alias %q, want %q", got, want)
	}
}

func TestLuauTypesBuilderTables(t *testing.T) {
	fixture := loadTypeFixture(t, "src/tables.ts")
	runTypeCases(t, fixture, []typeCase{
		{name: "Point", want: "type Point = { x: number, y: number }"},
		// readonly -> read, a method takes the alias as self, optional members
		// and optional methods take `?`, non-identifier and keyword keys are
		// quoted, a `this` parameter is spelled as its type, and the
		// polymorphic this type is the alias.
		{name: "Shape", want: `type Shape = { read kind: string, area: (self: Shape) -> number, optional: string?, ["kebab-case"]: number, ["1"]: boolean, ["end"]: string, onEvent: (value: number) -> (), maybeRun: ((self: Shape) -> ())?, explicitThis: (self: Point, n: number) -> number, clone: (self: Shape) -> Shape }`},
		{name: "Dict", want: "type Dict = { [string]: number }"},
		{name: "NumDict", want: "type NumDict = { [number]: string }"},
		{name: "Both", want: "type Both = { [any]: number }", losses: []fidelityLossKind{lossIndexerCollapsed}},
		{name: "BothDiffer", want: "type BothDiffer = { [any]: any }", losses: []fidelityLossKind{lossIndexerCollapsed}},
		{name: "ReadonlyDict", want: "type ReadonlyDict = { read [string]: number }"},
		{name: "Node", want: "type Node = { value: number, next: Node? }"},
		{name: "Tree", want: "type Tree<T> = { value: T, children: { Tree<T> } }"},
		{name: "Pair", want: "type Pair<T> = { first: T, second: T }"},
		{name: "Mode", want: `type Mode = "r" | "w"`},
		{name: "Callback", want: "type Callback<T> = (value: T) -> ()"},
		{name: "Empty", want: "type Empty = {}"},
		{name: "Direction", want: `type Direction = "up" | "down"`},
		{name: "Codes", want: "type Codes = number", losses: []fidelityLossKind{lossNumberLiteral}},
	}, fixture.aliasStatement)

	runTypeCases(t, fixture, []typeCase{
		{name: "record", want: "{ [string]: number }"},
		{name: "literalRecord", want: "{ a: number, b: number }"},
		{name: "partial", want: "{ x: number?, y: number? }"},
		{name: "readonlyPoint", want: "{ read x: number, read y: number }"},
		{name: "numbers", want: "{ number }"},
		{name: "readonlyNumbers", want: "{ read string }"},
		{name: "readonlyArraySyntax", want: "{ read boolean }"},
		{name: "pair", want: "{ string | number }", losses: []fidelityLossKind{lossTuple}},
		{name: "readonlyPair", want: "{ read string | number }", losses: []fidelityLossKind{lossTuple}},
		{name: "samePair", want: "{ number }", losses: []fidelityLossKind{lossTuple}},
		{name: "map", want: "{ [string]: Point }"},
		{name: "readonlyMap", want: "{ read [string]: number }"},
		{name: "weakMap", want: "{ [Point]: number }"},
		{name: "set", want: "{ [string]: true }"},
		{name: "readonlySet", want: "{ read [number]: true }"},
		{name: "weakSet", want: "{ [Point]: true }"},
		{name: "nested", want: "{ inner: { deep: { value: number } } }"},
		{name: "inlineMethod", want: "{ run: (self: any) -> number }", losses: []fidelityLossKind{lossObjectLiteralSelf}},
		{name: "numberPair", want: "Pair<number>"},
		{name: "mode", want: "Mode"},
		{name: "callback", want: "Callback<string>"},
		{name: "empty", want: "Empty"},
		{name: "direction", want: "Direction"},
		{name: "code", want: "Codes"},
		{name: "up", want: `"up"`},
		{name: "directionObject", want: `{ read Up: "up", read Down: "down" }`},
		{name: "foo", want: "Foo"},
		// The class value is structural; its recursive `self` static is the
		// anonymous-recursion guard at work.
		{name: "fooClass", want: "{ new: (x: number) -> Foo, self: { [any]: any }, make: () -> Foo, prototype: Foo }", losses: []fidelityLossKind{lossUnspellable}},
		{name: "box", want: "Box<number>"},
	}, func(name string) string { return text(fixture.variableType(name).Node) })
}

func TestLuauTypesBuilderFunctions(t *testing.T) {
	fixture := loadTypeFixture(t, "src/functions.ts")
	runTypeCases(t, fixture, []typeCase{
		{name: "plain", want: "(a: number, b: string) -> boolean"},
		{name: "optionalParams", want: "(a: number, b: string?, c: boolean?) -> ()"},
		{name: "restParams", want: "(first: number, ...string) -> number"},
		{name: "genericFn", want: "<T>(value: T) -> T"},
		{name: "constrained", want: "<T>(value: T) -> T", losses: []fidelityLossKind{lossConstraintDropped}},
		{name: "thisParam", want: "(self: { x: number }, n: number) -> number"},
		{name: "returnsUndefined", want: "() -> ()"},
		{name: "returnsOptional", want: "() -> number?"},
		{name: "returnsFn", want: "() -> (n: number) -> (s: string) -> boolean"},
		{name: "returnsUnion", want: "() -> ({ string } | { number })"},
		{name: "voidCallback", want: "(cb: () -> ()) -> ()"},
		{name: "describe", want: "(value: unknown) -> never"},
		// `until` is a Luau keyword, so the parameter goes unnamed.
		{name: "keyword", want: "(number) -> ()"},
		{name: "luaTuple", want: "() -> (number, string)"},
		{name: "optionalTuple", want: "() -> (number, string?)"},
		{name: "restTuple", want: "() -> (number, ...string)"},
		{name: "emptyTuple", want: "() -> ()"},
		{name: "maybeTuple", want: "() -> ...any", losses: []fidelityLossKind{lossOptionalPack}},
		{name: "looseTuple", want: "() -> ...any", losses: []fidelityLossKind{lossUnspellable}},
		{name: "tupleParam", want: "(pair: any) -> ()", losses: []fidelityLossKind{lossLuaTupleOutsideReturn}},
		{name: "tupleCallback", want: "(cb: () -> (number, string)) -> ()"},
		{name: "genericCallback", want: "(identify: <T>(value: T) -> T) -> ()"},
		{name: "outerGeneric", want: "<K>(key: K) -> (other: K) -> ()"},
		{name: "keys", want: "<T>(value: T) -> keyof<T>"},
		{name: "indexed", want: "<T, K>(value: T, key: K) -> index<T, K>", losses: []fidelityLossKind{lossConstraintDropped}},
		{name: "restGeneric", want: "<A>(...any) -> ()", losses: []fidelityLossKind{lossConstraintDropped, lossUnspellable}},
		{name: "tupleRest", want: "(...any) -> ()", losses: []fidelityLossKind{lossUnspellable}},
	}, func(name string) string { return text(fixture.functionType(name)) })

	if got, want := text(fixture.variableType("overloaded").Node), "((n: number) -> string) & ((s: string) -> number)"; got != want {
		t.Errorf("overload set %q, want %q", got, want)
	}

	// A library interface is mirrored as one alias instead of being
	// re-instantiated structurally at every use; references into its own
	// body with other arguments are `any`.
	from := len(fixture.state.LuauTypeLosses())
	if got, want := text(fixture.functionType("promised")), "() -> Promise<number>"; got != want {
		t.Errorf("promised %q, want %q", got, want)
	}
	if !hasLoss(lossesSince(fixture.state, from), lossRecursiveAliasArgs) {
		t.Errorf("mirroring Promise<T> must record RecursiveAliasArgs, got %v", distinctLosses(fixture.state, from))
	}
	var names []string
	for _, entry := range fixture.builder().Aliases() {
		names = append(names, entry.Name)
	}
	if !strings.Contains(strings.Join(names, ","), "Promise") {
		t.Errorf("Promise alias not registered: %v", names)
	}
}

// Rule R1 stays a query: a generic of the innermost function is "own", one
// of an enclosing function is "outer" at a parameter position only.
func TestLuauTypesBuilderOuterGeneric(t *testing.T) {
	fixture := loadTypeFixture(t, "src/functions.ts")

	fixture.state.PushTypeGenerics([]string{"K"}, false)
	own := fixture.parameterType("outerGeneric", 0)
	fixture.state.PushTypeGenerics(nil, false)
	outer := fixture.parameterType("outerGeneric", 0)
	nested := fixture.builder().Build(fixture.state.Checker.GetTypeAtLocation(fixture.declaration("outerGeneric").Parameters()[0].Name()), typeContext{Position: positionNested})
	fixture.state.PopTypeGenerics()
	fixture.state.PopTypeGenerics()

	for _, c := range []struct {
		name string
		got  builtType
		want bool
	}{{"own", own, false}, {"outer", outer, true}, {"nested", nested, false}} {
		if text(c.got.Node) != "K" {
			t.Errorf("%s: spelled %q, want K", c.name, text(c.got.Node))
		}
		if c.got.OuterGeneric != c.want {
			t.Errorf("%s: OuterGeneric = %v, want %v", c.name, c.got.OuterGeneric, c.want)
		}
		if c.got.Kind != nativeKindGeneric {
			t.Errorf("%s: Kind = %v, want generic", c.name, c.got.Kind)
		}
	}

	// With no scope the parameter is not declarable at the site.
	from := len(fixture.state.LuauTypeLosses())
	outOfScope := fixture.parameterType("genericFn", 0)
	if got := text(outOfScope.Node); got != "any" || !outOfScope.Lossy {
		t.Errorf("out-of-scope generic spelled %q (lossy %v), want any", got, outOfScope.Lossy)
	}
	if got := distinctLosses(fixture.state, from); !sameLossSet(got, []fidelityLossKind{lossUnspellable}) {
		t.Errorf("out-of-scope losses %v, want [Unspellable]", got)
	}
}

// `typeof x` keeps Luau's typeof only for a top-level const or function of
// this file declared before the use; everything else resolves.
func TestLuauTypesBuilderTypeof(t *testing.T) {
	fixture := loadTypeFixture(t, "src/tables.ts")
	spell := func(name string) string {
		declaration := fixture.declaration(name)
		typeNode := declaration.AsVariableDeclaration().Type
		if typeNode == nil {
			t.Fatalf("%s has no type annotation", name)
		}
		return text(fixture.builder().BuildTypeNode(typeNode, typeContext{Position: positionLocal, Owner: declaration}).Node)
	}
	runTypeCases(t, fixture, []typeCase{
		{name: "configType", want: "typeof(config)"},
		{name: "importedType", want: "number"},
		{name: "counterType", want: "number"},
		{name: "laterType", want: "{ x: number }"},
		{name: "foo", want: "Foo"},
	}, spell)
}

// Every reference spelled from a TS symbol is recorded in Origins, and the
// table built for an interface body carries the interface.
func TestLuauTypesBuilderOrigins(t *testing.T) {
	fixture := loadTypeFixture(t, "src/tables.ts")
	builder := fixture.builder()

	for _, c := range []struct {
		name  string
		flags ast.SymbolFlags
	}{
		{"foo", ast.SymbolFlagsClass},
		{"numberPair", ast.SymbolFlagsTypeAlias},
		{"direction", ast.SymbolFlagsEnum},
		{"map", 0},
	} {
		node := fixture.variableType(c.name).Node
		if c.flags == 0 {
			if _, ok := builder.Origins[node]; ok {
				t.Errorf("%s: a structural table must not have an origin", c.name)
			}
			continue
		}
		symbol := builder.Origins[node]
		if symbol == nil || symbol.Flags&c.flags == 0 {
			t.Errorf("%s: origin %v, want a symbol with flags %v", c.name, symbol, c.flags)
		}
	}

	point := builder.Origins[fixture.alias("Point").Statement.Body]
	if point == nil || point.Name != "Point" {
		t.Errorf("interface body origin = %v, want Point", point)
	}

	primitives := loadTypeFixture(t, "src/primitives.ts")
	material := primitives.variableType("aMaterial").Node
	if symbol := primitives.builder().Origins[material]; symbol == nil || symbol.Name != "Material" {
		t.Errorf("Enum.Material origin = %v, want the Material alias", symbol)
	}
}

// Kind classifies the built node the way the native policy expects.
func TestLuauTypesBuilderKinds(t *testing.T) {
	fixture := loadTypeFixture(t, "src/tables.ts")
	for _, c := range []struct {
		name string
		want nativeTypeKind
	}{
		{"numbers", nativeKindTable},
		{"foo", nativeKindTable},
		{"mode", nativeKindString},
		{"code", nativeKindNumber},
		{"callback", nativeKindFunction},
		{"direction", nativeKindString},
		{"fooClass", nativeKindTable},
	} {
		if got := fixture.variableType(c.name).Kind; got != c.want {
			t.Errorf("%s: Kind = %v, want %v", c.name, got, c.want)
		}
	}
	primitives := loadTypeFixture(t, "src/primitives.ts")
	for _, c := range []struct {
		name string
		want nativeTypeKind
	}{
		{"aNumber", nativeKindNumber},
		{"aString", nativeKindString},
		{"aBoolean", nativeKindBoolean},
		{"anUndefined", nativeKindNil},
		{"aVector3", nativeKindVector},
		{"aPart", nativeKindVector},
		{"aMaterial", nativeKindVector},
		{"anUnknown", nativeKindUnspellable},
		{"stringLiteral", nativeKindString},
	} {
		if got := primitives.variableType(c.name).Kind; got != c.want {
			t.Errorf("%s: Kind = %v, want %v", c.name, got, c.want)
		}
	}
	unions := loadTypeFixture(t, "src/unions.ts")
	if got := unions.variableType("boolOrUndefined").Kind; got != nativeKindBoolean {
		t.Errorf("boolean? Kind = %v, want boolean", got)
	}
	if got := unions.variableType("overloads").Kind; got != nativeKindFunction {
		t.Errorf("overload set Kind = %v, want function", got)
	}
}

// The receiver and the class-instance flag reach a table built directly.
func TestLuauTypesBuilderReceiver(t *testing.T) {
	fixture := loadTypeFixture(t, "src/tables.ts")
	builder := fixture.builder()
	shape := fixture.state.Checker.GetDeclaredTypeOfSymbol(fixture.symbol("Shape"))

	from := len(fixture.state.LuauTypeLosses())
	table := builder.BuildTable(shape, typeContext{Position: positionNested, Receiver: luau.TypeName("Custom"), This: luau.TypeName("Custom"), ClassInstance: true})
	if table == nil {
		t.Fatal("BuildTable returned nil")
	}
	// The checker instantiates `this` in a declared interface's members to
	// the interface itself, so `clone(): this` returns the Shape alias.
	want := `{ kind: string, area: (self: Custom) -> number, optional: string?, ["kebab-case"]: number, ["1"]: boolean, ["end"]: string, onEvent: (value: number) -> (), maybeRun: ((self: Custom) -> ())?, explicitThis: (self: Point, n: number) -> number, clone: (self: Custom) -> Shape }`
	if got := text(table); got != want {
		t.Errorf("receiver table %q, want %q", got, want)
	}
	if got := distinctLosses(fixture.state, from); !sameLossSet(got, []fidelityLossKind{lossClassReadonlyDropped}) {
		t.Errorf("losses %v, want [ClassReadonlyDropped]", got)
	}

	inline := fixture.state.Checker.GetTypeAtLocation(fixture.declaration("inlineMethod").Name())
	from = len(fixture.state.LuauTypeLosses())
	named := builder.BuildTable(inline, typeContext{Receiver: luau.TypeName("Obj")})
	if named == nil {
		t.Fatal("BuildTable returned nil")
	}
	if got, want := text(named), "{ run: (self: Obj) -> number }"; got != want {
		t.Errorf("inline receiver %q, want %q", got, want)
	}
	if got := distinctLosses(fixture.state, from); len(got) != 0 {
		t.Errorf("a named receiver must not record losses, got %v", got)
	}
}

// Aliases register once, in first-use order, and keep their statement.
func TestLuauTypesBuilderAliasRegistry(t *testing.T) {
	fixture := loadTypeFixture(t, "src/tables.ts")
	fixture.variableType("map")        // registers Point
	fixture.variableType("numberPair") // registers Pair
	fixture.variableType("map")

	var names []string
	for _, entry := range fixture.builder().Aliases() {
		names = append(names, entry.Name)
	}
	if got, want := strings.Join(names, ","), "Point,Pair"; got != want {
		t.Errorf("registration order %q, want %q", got, want)
	}
	if first, second := fixture.alias("Point"), fixture.alias("Point"); first != second || first.building || first.Statement.Body == nil {
		t.Errorf("typeAliasFor must return the same finished entry")
	}
	pair := fixture.alias("Pair")
	if len(pair.Params) != 1 || len(pair.Generics) != 1 || pair.Generics[0].Name != "T" || pair.Kind != nativeKindTable {
		t.Errorf("Pair entry = %+v", pair)
	}
	callback := fixture.alias("Callback")
	if !callback.Callable || callback.Kind != nativeKindFunction {
		t.Errorf("Callback entry must be callable, got %+v", callback)
	}
	if fixture.state.typeAliasFor(nil) != nil {
		t.Errorf("a nil symbol has no alias")
	}
}
