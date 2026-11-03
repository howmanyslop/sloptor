package transformer

import (
	"testing"

	"rotor/internal/luau"
	"rotor/tsgo/ast"
)

// The native policy on hand-built trees. Every expected spelling is a line
// the `//!native` fixtures pin (nativetypes_test.go) or a rule of the policy
// as documented in nativetypes_policy.go, written out as the literal Luau the
// rule promises; none is computed from the code under test.

// fakeNativeOrigins stands in for the builder: which symbol a node came from,
// what a named alias stands for, and which generics are the annotated
// function's own.
type fakeNativeOrigins struct {
	origins map[luau.TypeNode]*ast.Symbol
	kinds   map[*ast.Symbol]nativeTypeKind
	own     map[string]bool
}

func newFakeNativeOrigins() *fakeNativeOrigins {
	return &fakeNativeOrigins{
		origins: make(map[luau.TypeNode]*ast.Symbol),
		kinds:   make(map[*ast.Symbol]nativeTypeKind),
		own:     make(map[string]bool),
	}
}

func (f *fakeNativeOrigins) originOf(node luau.TypeNode) *ast.Symbol { return f.origins[node] }

func (f *fakeNativeOrigins) aliasKindOf(symbol *ast.Symbol) (nativeTypeKind, bool) {
	kind, ok := f.kinds[symbol]
	return kind, ok
}

func (f *fakeNativeOrigins) ownGeneric(name string) bool { return f.own[name] }

func (f *fakeNativeOrigins) transferOrigin(from, to luau.TypeNode) {
	if symbol, ok := f.origins[from]; ok {
		f.origins[to] = symbol
	}
}

// from marks node as spelled from symbol.
func (f *fakeNativeOrigins) from(node luau.TypeNode, symbol *ast.Symbol) luau.TypeNode {
	f.origins[node] = symbol
	return node
}

// alias registers symbol as a `type` statement of the given kind and returns
// a reference to it.
func (f *fakeNativeOrigins) alias(name string, kind nativeTypeKind, arguments ...luau.TypeNode) luau.TypeNode {
	symbol := &ast.Symbol{Name: name, Flags: ast.SymbolFlagsInterface}
	f.kinds[symbol] = kind
	var list *luau.List[luau.TypeNode]
	if len(arguments) > 0 {
		list = luau.NewList(arguments...)
	}
	return f.from(luau.NewTypeReference(name, list), symbol)
}

func (f *fakeNativeOrigins) generic(name string, own bool) luau.TypeNode {
	f.own[name] = own
	return f.from(luau.TypeName(name), &ast.Symbol{Name: name, Flags: ast.SymbolFlagsTypeParameter})
}

func (f *fakeNativeOrigins) roblox(name string) luau.TypeNode {
	return f.from(luau.TypeName(name), &ast.Symbol{Name: name, Flags: ast.SymbolFlagsInterface})
}

func policyUnion(members ...luau.TypeNode) luau.TypeNode {
	return luau.NewTypeUnion(luau.NewList(members...))
}

func policyIntersection(members ...luau.TypeNode) luau.TypeNode {
	return luau.NewTypeIntersection(luau.NewList(members...))
}

func policyTable(fields ...luau.TypeTableField) *luau.TypeTable {
	return luau.NewTypeTable(luau.NewList(fields...))
}

func policyProp(access luau.TypeAccess, name string, value luau.TypeNode) luau.TypeTableField {
	return luau.NewTypeTableProp(access, name, value)
}

func policyFunction(parameters []luau.TypeFunctionParameter, varArgs, returnType luau.TypeNode) luau.TypeNode {
	return luau.NewTypeFunction(nil, parameters, varArgs, returnType)
}

func policyParameter(name string, t luau.TypeNode) luau.TypeFunctionParameter {
	return luau.TypeFunctionParameter{Name: name, Type: t}
}

// nestedTables builds `{ a: { a: ... { a: leaf } } }` with the given number of
// table levels.
func nestedTables(levels int, leaf luau.TypeNode) luau.TypeNode {
	node := leaf
	for range levels {
		node = policyTable(policyProp(luau.TypeAccessNone, "a", node))
	}
	return node
}

type policyCase struct {
	name     string
	build    func(f *fakeNativeOrigins) luau.TypeNode
	position nativePosition
	// want is the rendered result, "" when the annotation must be dropped.
	want      string
	wantKind  nativeTypeKind
	wantOuter bool
}

func runPolicyCases(t *testing.T, cases []policyCase) {
	t.Helper()
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			f := newFakeNativeOrigins()
			node, kind, outer := applyNativePolicy(c.build(f), f, c.position, 0)
			got := ""
			if node != nil {
				got = renderTypeText(node)
			}
			if got != c.want {
				t.Errorf("spelling = %q, want %q", got, c.want)
			}
			if kind != c.wantKind {
				t.Errorf("kind = %d, want %d", kind, c.wantKind)
			}
			if outer != c.wantOuter {
				t.Errorf("outer generic = %v, want %v", outer, c.wantOuter)
			}
			if node == nil {
				return
			}
			// The spelling hook probes a constraint with the policy before
			// the parameter pass runs over the whole annotation, so a second
			// pass must change nothing.
			again, againKind, _ := applyNativePolicy(node, f, c.position, 0)
			if again == nil || renderTypeText(again) != got || againKind != kind {
				t.Errorf("second pass changed the result: %q", renderTypeText(again))
			}
		})
	}
}

// Catches a parameter annotated with a type Luau tags as something other than
// a table, vector, string or number: the entry guard then fails or buys
// nothing. Expected spellings are the fixture lines of TestNativeFiles*.
func TestNativePolicyParameterTags(t *testing.T) {
	runPolicyCases(t, []policyCase{
		{
			name: "mixed-kind union is dropped",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				return policyUnion(luau.TypeName("number"), luau.TypeName("string"))
			},
			position: nativePositionParameter,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
		{
			name: "same-kind table union keeps its tag",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				return policyUnion(luau.NewTypeArray(luau.TypeName("string")), luau.NewTypeArray(luau.TypeName("number")))
			},
			position: nativePositionParameter,
			want:     "{ string } | { number }",
			wantKind: nativeKindTable,
		},
		{
			name: "optional table union keeps its tag",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				return luau.OptionalType(policyUnion(luau.NewTypeArray(luau.TypeName("string")), luau.NewTypeArray(luau.TypeName("number"))))
			},
			position: nativePositionParameter,
			want:     "({ string } | { number })?",
			wantKind: nativeKindTable,
		},
		{
			name:     "both vector names share a tag",
			build:    func(f *fakeNativeOrigins) luau.TypeNode { return policyUnion(f.roblox("Vector3"), f.roblox("vector")) },
			position: nativePositionParameter,
			want:     "Vector3 | vector",
			wantKind: nativeKindVector,
		},
		{
			name: "string singletons share the string tag",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				return policyUnion(luau.NewTypeStringSingleton("x"), luau.NewTypeStringSingleton("y"))
			},
			position: nativePositionParameter,
			want:     `"x" | "y"`,
			wantKind: nativeKindString,
		},
		{
			name:     "boolean is dropped",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return luau.TypeName("boolean") },
			position: nativePositionParameter,
			want:     "",
			wantKind: nativeKindBoolean,
		},
		{
			name:     "optional boolean literal is dropped",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return luau.OptionalType(luau.NewTypeBooleanSingleton(true)) },
			position: nativePositionParameter,
			want:     "",
			wantKind: nativeKindBoolean,
		},
		{
			name:     "nil is dropped",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return luau.TypeName("nil") },
			position: nativePositionParameter,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
		{
			name:     "unknown is dropped",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return luau.TypeName("unknown") },
			position: nativePositionParameter,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
		{
			name:     "an Instance subclass is dropped",
			build:    func(f *fakeNativeOrigins) luau.TypeNode { return f.roblox("Part") },
			position: nativePositionParameter,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
		{
			name:     "a vector is kept",
			build:    func(f *fakeNativeOrigins) luau.TypeNode { return f.roblox("vector") },
			position: nativePositionParameter,
			want:     "vector",
			wantKind: nativeKindVector,
		},
		{
			name: "a class instance is dropped",
			build: func(f *fakeNativeOrigins) luau.TypeNode {
				return f.from(luau.TypeName("Counter"), &ast.Symbol{Name: "Counter", Flags: ast.SymbolFlagsClass})
			},
			position: nativePositionParameter,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
		{
			name:     "an empty table is dropped",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return policyTable() },
			position: nativePositionParameter,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
		{
			name:     "a table alias is kept",
			build:    func(f *fakeNativeOrigins) luau.TypeNode { return f.alias("Foo", nativeKindTable) },
			position: nativePositionParameter,
			want:     "Foo",
			wantKind: nativeKindTable,
		},
		{
			name:     "a function alias is kept",
			build:    func(f *fakeNativeOrigins) luau.TypeNode { return f.alias("Notify", nativeKindFunction) },
			position: nativePositionParameter,
			want:     "Notify",
			wantKind: nativeKindFunction,
		},
		{
			name: "a table alias and a function alias mix",
			build: func(f *fakeNativeOrigins) luau.TypeNode {
				return policyUnion(f.alias("Foo", nativeKindTable), f.alias("Notify", nativeKindFunction))
			},
			position: nativePositionParameter,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
	})
}

// Catches a parameter annotated with an enclosing function's generic, which
// Luau tags as userdata so the native entry guard fails on every call (rule
// R1); the function's own generic is tagged `any` and stays, and a nested
// occurrence is never tagged at all.
func TestNativePolicyOuterGenerics(t *testing.T) {
	runPolicyCases(t, []policyCase{
		{
			name:     "own generic at a parameter",
			build:    func(f *fakeNativeOrigins) luau.TypeNode { return f.generic("K", true) },
			position: nativePositionParameter,
			want:     "K",
			wantKind: nativeKindGeneric,
		},
		{
			name:      "outer generic at a parameter",
			build:     func(f *fakeNativeOrigins) luau.TypeNode { return f.generic("K", false) },
			position:  nativePositionParameter,
			want:      "",
			wantKind:  nativeKindGeneric,
			wantOuter: true,
		},
		{
			name:      "outer generic behind an optional",
			build:     func(f *fakeNativeOrigins) luau.TypeNode { return luau.OptionalType(f.generic("K", false)) },
			position:  nativePositionParameter,
			want:      "",
			wantKind:  nativeKindGeneric,
			wantOuter: true,
		},
		{
			name: "outer generic as an alias argument",
			build: func(f *fakeNativeOrigins) luau.TypeNode {
				return f.alias("Node", nativeKindTable, f.generic("K", false), f.generic("V", false))
			},
			position: nativePositionParameter,
			want:     "Node<K, V>",
			wantKind: nativeKindTable,
		},
		{
			name:     "outer generic at a return",
			build:    func(f *fakeNativeOrigins) luau.TypeNode { return f.generic("V", false) },
			position: nativePositionReturn,
			want:     "V",
			wantKind: nativeKindGeneric,
		},
	})
}

// Catches a declared return that claims a value the body cannot produce or
// that Luau cannot spell as a pack: `void` is `()`, an unknown pack or an
// unspellable type stays unannotated, and a mixed union is fine (returns are
// not tagged).
func TestNativePolicyReturns(t *testing.T) {
	runPolicyCases(t, []policyCase{
		{
			name:     "nil becomes the empty pack",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return luau.TypeName("nil") },
			position: nativePositionReturn,
			want:     "()",
			wantKind: nativeKindNil,
		},
		{
			name:     "an unknown pack stays unannotated",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return luau.NewTypeVariadic(luau.TypeName("any")) },
			position: nativePositionReturn,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
		{
			name: "a class instance stays unannotated",
			build: func(f *fakeNativeOrigins) luau.TypeNode {
				return f.from(luau.TypeName("Counter"), &ast.Symbol{Name: "Counter", Flags: ast.SymbolFlagsClass})
			},
			position: nativePositionReturn,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
		{
			name: "a union with an unspellable member stays unannotated",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				return policyUnion(luau.TypeName("number"), luau.TypeName("never"))
			},
			position: nativePositionReturn,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
		{
			name: "a mixed union is spelled",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				return policyUnion(luau.TypeName("number"), luau.TypeName("string"))
			},
			position: nativePositionReturn,
			want:     "number | string",
			wantKind: nativeKindNumber,
		},
		{
			name:     "boolean is spelled",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return luau.TypeName("boolean") },
			position: nativePositionReturn,
			want:     "boolean",
			wantKind: nativeKindBoolean,
		},
		{
			name:     "an Instance is spelled by name",
			build:    func(f *fakeNativeOrigins) luau.TypeNode { return f.roblox("Instance") },
			position: nativePositionReturn,
			want:     "Instance",
			wantKind: nativeKindVector,
		},
		{
			name: "pack elements are nested slots",
			build: func(f *fakeNativeOrigins) luau.TypeNode {
				return luau.NewTypePack(luau.NewList[luau.TypeNode](
					luau.TypeName("number"),
					luau.OptionalType(f.from(luau.TypeName("Counter"), &ast.Symbol{Name: "Counter", Flags: ast.SymbolFlagsClass})),
					luau.NewTypeVariadic(luau.TypeName("string"))))
			},
			position: nativePositionReturn,
			want:     "(number, any, ...string)",
			wantKind: nativeKindUnspellable,
		},
	})
}

// Catches an overload set annotated where it hurts: at a parameter an
// intersection is tagged `any` and is stricter than TS, so the function tag
// `(...any) -> ...any` wins; everywhere else the overloads are kept. A TS
// intersection of anything else has no spelling.
func TestNativePolicyOverloads(t *testing.T) {
	overloads := func(f *fakeNativeOrigins) luau.TypeNode {
		receiver := f.alias("OverloadedMethod", nativeKindTable)
		return policyIntersection(
			policyFunction([]luau.TypeFunctionParameter{policyParameter("self", receiver), policyParameter("value", luau.TypeName("number"))}, nil, luau.TypeName("string")),
			policyFunction([]luau.TypeFunctionParameter{policyParameter("self", cloneType(receiver)), policyParameter("value", luau.TypeName("string"))}, nil, luau.TypeName("number")),
		)
	}
	runPolicyCases(t, []policyCase{
		{
			name:     "overloads fall back at a parameter",
			build:    overloads,
			position: nativePositionParameter,
			want:     "(...any) -> ...any",
			wantKind: nativeKindFunction,
		},
		{
			name:     "overloads are kept nested, with the receiver widened",
			build:    overloads,
			position: nativePositionNested,
			want:     "((self: any, value: number) -> string) & ((self: any, value: string) -> number)",
			wantKind: nativeKindFunction,
		},
		{
			name:     "overloads are kept at a return",
			build:    overloads,
			position: nativePositionReturn,
			want:     "((self: any, value: number) -> string) & ((self: any, value: string) -> number)",
			wantKind: nativeKindFunction,
		},
		{
			name: "a table intersection widens nested",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				return policyIntersection(policyTable(policyProp(luau.TypeAccessNone, "a", luau.TypeName("number"))), policyTable(policyProp(luau.TypeAccessNone, "b", luau.TypeName("string"))))
			},
			position: nativePositionNested,
			want:     "any",
			wantKind: nativeKindUnspellable,
		},
		{
			name: "a table intersection is dropped at a parameter",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				return policyIntersection(policyTable(policyProp(luau.TypeAccessNone, "a", luau.TypeName("number"))), policyTable(policyProp(luau.TypeAccessNone, "b", luau.TypeName("string"))))
			},
			position: nativePositionParameter,
			want:     "",
			wantKind: nativeKindUnspellable,
		},
	})
}

// Catches a nested shape the analyzer rejects or native codegen cannot use:
// `read` is not legal on a native-annotated table, a method's exact receiver
// only adds a guard, an unspellable member widens to `any`, and a table
// nested deeper than nativeTypeMaxDepth is still a table.
func TestNativePolicyNestedShapes(t *testing.T) {
	runPolicyCases(t, []policyCase{
		{
			name: "readonly members are stripped",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				return policyTable(
					policyProp(luau.TypeAccessRead, "hi", luau.TypeName("string")),
					luau.NewTypeTableIndexer(luau.TypeAccessRead, luau.TypeName("string"), luau.TypeName("number")))
			},
			position: nativePositionNested,
			want:     "{ hi: string, [string]: number }",
			wantKind: nativeKindTable,
		},
		{
			name: "readonly arrays are stripped",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				array := luau.NewTypeArray(luau.TypeName("string"))
				array.Access = luau.TypeAccessRead
				return array
			},
			position: nativePositionParameter,
			want:     "{ string }",
			wantKind: nativeKindTable,
		},
		{
			name: "the receiver is any",
			build: func(f *fakeNativeOrigins) luau.TypeNode {
				return policyFunction([]luau.TypeFunctionParameter{policyParameter("self", f.alias("WithMethod", nativeKindTable))}, nil, luau.TypeName("number"))
			},
			position: nativePositionNested,
			want:     "(self: any) -> number",
			wantKind: nativeKindFunction,
		},
		{
			name: "function type slots are nested",
			build: func(f *fakeNativeOrigins) luau.TypeNode {
				class := &ast.Symbol{Name: "Counter", Flags: ast.SymbolFlagsClass}
				return policyFunction(
					[]luau.TypeFunctionParameter{policyParameter("start", luau.TypeName("number")), policyParameter("flag", luau.TypeName("boolean"))},
					luau.NewTypeVariadic(f.from(luau.TypeName("Counter"), class)),
					f.from(luau.TypeName("Counter"), class))
			},
			position: nativePositionParameter,
			want:     "(start: number, flag: boolean, ...any) -> any",
			wantKind: nativeKindFunction,
		},
		{
			name: "a nested unspellable union member widens the union",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				return policyUnion(luau.TypeName("number"), luau.TypeName("unknown"))
			},
			position: nativePositionNested,
			want:     "any",
			wantKind: nativeKindUnspellable,
		},
		{
			name:     "nested booleans are spelled",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return luau.OptionalType(luau.TypeName("boolean")) },
			position: nativePositionNested,
			want:     "boolean?",
			wantKind: nativeKindBoolean,
		},
		{
			name: "a nested Instance is spelled by name",
			build: func(f *fakeNativeOrigins) luau.TypeNode {
				return policyTable(policyProp(luau.TypeAccessNone, "part", f.roblox("Part")))
			},
			position: nativePositionParameter,
			want:     "{ part: Part }",
			wantKind: nativeKindTable,
		},
		{
			name: "an enum object is any",
			build: func(f *fakeNativeOrigins) luau.TypeNode {
				return f.from(policyTable(policyProp(luau.TypeAccessNone, "Up", luau.TypeName("string"))), &ast.Symbol{Name: "Direction", Flags: ast.SymbolFlagsRegularEnum})
			},
			position: nativePositionNested,
			want:     "any",
			wantKind: nativeKindUnspellable,
		},
		{
			name: "a Roblox enum is any",
			build: func(f *fakeNativeOrigins) luau.TypeNode {
				return f.from(luau.NewTypeQualifiedReference("Enum", "Material", nil), &ast.Symbol{Name: "Material"})
			},
			position: nativePositionNested,
			want:     "any",
			wantKind: nativeKindUnspellable,
		},
		{
			name:     "a table at the depth limit is spelled",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return nestedTables(5, luau.TypeName("number")) },
			position: nativePositionParameter,
			want:     "{ a: { a: { a: { a: { a: number } } } } }",
			wantKind: nativeKindTable,
		},
		{
			name:     "a table past the depth limit is any table",
			build:    func(*fakeNativeOrigins) luau.TypeNode { return nestedTables(6, luau.TypeName("number")) },
			position: nativePositionParameter,
			want:     "{ a: { a: { a: { a: { a: { [any]: any } } } } } }",
			wantKind: nativeKindTable,
		},
		{
			name: "duplicate members after widening collapse",
			build: func(*fakeNativeOrigins) luau.TypeNode {
				readonly := luau.NewTypeArray(luau.TypeName("number"))
				readonly.Access = luau.TypeAccessRead
				return policyUnion(luau.NewTypeArray(luau.TypeName("number")), readonly)
			},
			position: nativePositionNested,
			want:     "{ number }",
			wantKind: nativeKindTable,
		},
	})
}
