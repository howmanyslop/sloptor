package transformer_test

import (
	"path/filepath"
	"regexp"
	"slices"
	"strings"
	"testing"

	"rotor/internal/luau/render"
	"rotor/internal/transformer"
)

// Classes and enums in luauTypes mode (plan WP7, luautypes_class.go), seen
// through whole-file output: the instance alias at the declaration, the
// class table initialized on its declaration line, `new` through the class
// table's constructor, dot-form functions with an annotated `self`, class
// generics re-declared on every function, statics on `typeof(Class)`, the
// `super` local, the hoisted and class-expression shapes, the imported class
// mirror, and the enum alias plus annotated object local. Every fixture is
// also handed to the Luau analyzer in strict mode.

// luauColonMethod matches a colon-form function statement, which luauTypes
// never emits (probe P9b: colon-form `self` is a fresh, unrelated table).
var luauColonMethod = regexp.MustCompile(`(?m)^\s*function [A-Za-z_][A-Za-z0-9_]*:[A-Za-z_]`)

// luauAliasStatement matches a `type` or `export type` statement.
var luauAliasStatement = regexp.MustCompile(`(?m)^(export )?type [A-Za-z_]`)

func TestLuauTypesClasses(t *testing.T) {
	got, losses := renderLuauTypesLosses(t, "src/classes.ts")

	// The imported class is mirrored as a hoisted alias (with the interface
	// it implements, which types the `self` of its `area`), the class
	// expressions' aliases are hoisted under the class name or the assigned
	// variable, and the import local is left to inference like any other
	// initialized local.
	assertLinesInOrder(t, got, []string{
		"local TS = _G[script]",
		"type Inner = { id: number, twice: (self: Inner) -> number }",
		"type Anonymous = { label: string }",
		"type Circle = { kind: ShapeKind, radius: number, area: (self: Shape) -> number }",
		`type ShapeKind = "circle" | "square"`,
		"type Shape = { kind: ShapeKind, area: (self: Shape) -> number }",
		"-- Classes (plan WP7): a base class with a static and a toString, a derived",
		`local Circle = TS.import(script, script.Parent, "lib", "shapes").Circle`,
	})

	// A base class: alias at the declaration without `constructor`, private
	// and readonly members as plain fields, the table on the declaration
	// line, `new` through the class table, the constructor and methods in
	// dot form, the static on `typeof(Animal)`, `__tostring` typed, and the
	// static property assigned afterwards.
	assertLinesInOrder(t, got, []string{
		"export type Animal = {",
		"\tsecret: number,",
		"\ttag: string?,",
		"\tname: string,",
		"\tspeak: (self: Animal) -> string,",
		"\ttoString: (self: Animal) -> string,",
		"}",
		"local Animal = setmetatable({}, {",
		"\t__tostring = function()",
		`		return "Animal"`,
		"\tend,",
		"})",
		"do",
		"\tAnimal.__index = Animal",
		"\tfunction Animal.new(name: string): Animal",
		"\t\tlocal self = (setmetatable({}, Animal) :: any) :: Animal",
		"\t\treturn Animal.constructor(self, name) or self",
		"\tend",
		"\tfunction Animal.constructor(self: Animal, name: string)",
		"\t\tself.name = name",
		"\t\tself.secret = 1",
		"\t\tAnimal.count += 1",
		"\tend",
		"\tfunction Animal.speak(self: Animal): string",
		"\tfunction Animal.toString(self: Animal): string",
		"\tfunction Animal.create(self: typeof(Animal), name: string): Animal",
		"\t\treturn Animal.new(name)",
		"\tfunction Animal.__tostring(self: Animal): string",
		"\t\treturn self:toString()",
		"\tAnimal.count = 0",
		"end",
	})

	// A derived class re-lists the inherited members flat; an inherited or
	// overridden method keeps `self` typed as the class that first declared
	// it while its own definition takes the derived class. `super` is
	// declared before the table so the heritage expression runs once.
	assertLinesInOrder(t, got, []string{
		"export type Dog = {",
		"\ttricks: number,",
		"\tspeak: (self: Animal) -> string,",
		"\tboth: (self: Dog) -> string,",
		"\tsecret: number,",
		"\ttag: string?,",
		"\tname: string,",
		"\ttoString: (self: Animal) -> string,",
		"}",
		"local super = Animal",
		"local Dog = setmetatable({}, {",
		"\t__index = super,",
		"})",
		"do",
		"\tDog.__index = Dog",
		"\tfunction Dog.new(name: string, tricks: number): Dog",
		"\t\tlocal self = (setmetatable({}, Dog) :: any) :: Dog",
		"\t\treturn Dog.constructor(self, name, tricks) or self",
		"\tfunction Dog.constructor(self: Dog, name: string, tricks: number)",
		"\t\tsuper.constructor(self, name)",
		"\t\tself.tricks = tricks",
		"\tfunction Dog.speak(self: Dog): string",
		"\tfunction Dog.both(self: Dog): string",
		"\tfunction Dog.__tostring(self: Dog): string",
	})

	// An implicit derived constructor forwards the inherited construct
	// signature instead of rbxtsc's bare `...`.
	assertLinesInOrder(t, got, []string{
		"export type Puppy = {",
		"local super = Dog",
		"\tfunction Puppy.new(name: string, tricks: number): Puppy",
		"\t\treturn Puppy.constructor(self, name, tricks) or self",
		"\tfunction Puppy.constructor(self: Puppy, name: string, tricks: number)",
		"\t\tsuper.constructor(self, name, tricks)",
		"\tend",
	})

	// An abstract class without `extends` stays a plain table and gets no
	// `new`; its abstract method is listed on the alias, and the subclass
	// implementing it keeps the base as that method's `self`.
	assertLinesInOrder(t, got, []string{
		"export type Figure = { area: (self: Figure) -> number, describe: (self: Figure) -> string }",
		"local Figure = {}",
		"do",
		"\tfunction Figure.constructor(self: Figure)",
		"\tend",
		"\tfunction Figure.describe(self: Figure): string",
		"export type Square = {",
		"\tside: number,",
		"\tarea: (self: Figure) -> number,",
		"\tdescribe: (self: Figure) -> string,",
		"}",
		"local super = Figure",
		"\tfunction Square.new(side: number): Square",
		"\tfunction Square.constructor(self: Square, side: number)",
		"\t\tsuper.constructor(self)",
		"\tfunction Square.area(self: Square): number",
	})
	if strings.Contains(got, "function Figure.new") {
		t.Errorf("an abstract class must not get `new`:\n%s", got)
	}

	// Generic classes: a generic alias, every function re-declaring the
	// class generics (own generics after them), the generic subclass passing
	// its parameter down and an instantiated subclass passing `number`.
	assertLinesInOrder(t, got, []string{
		"export type Box<T> = {",
		"\tvalue: T,",
		"\tget: (self: Box<T>) -> T,",
		"\tmap: <U>(self: Box<T>, transform: (value: T) -> U) -> U,",
		"}",
		"\tfunction Box.new<T>(value: T): Box<T>",
		"\t\tlocal self = (setmetatable({}, Box) :: any) :: Box<T>",
		"\t\treturn Box.constructor(self, value) or self",
		"\tfunction Box.constructor<T>(self: Box<T>, value: T)",
		"\tfunction Box.get<T>(self: Box<T>): T",
		"\tfunction Box.map<T, U>(self: Box<T>, transform: (value: T) -> U): U",
		"export type Pair<T> = {",
		"\tother: T,",
		"\tswap: (self: Pair<T>) -> (),",
		"\tvalue: T,",
		"\tget: (self: Box<T>) -> T,",
		"\tmap: <U>(self: Box<T>, transform: (value: T) -> U) -> U,",
		"}",
		"local super = Box",
		"\tfunction Pair.new<T>(value: T, other: T): Pair<T>",
		"\tfunction Pair.constructor<T>(self: Pair<T>, value: T, other: T)",
		"\t\tsuper.constructor(self, value)",
		"\tfunction Pair.swap<T>(self: Pair<T>): ()",
		"export type NumberBox = {",
		"\tdouble: (self: NumberBox) -> number,",
		"\tvalue: number,",
		"\tget: (self: Box<number>) -> number,",
		"\tmap: <U>(self: Box<number>, transform: (value: number) -> U) -> U,",
		"}",
		"\tfunction NumberBox.new(value: number): NumberBox",
		"\tfunction NumberBox.constructor(self: NumberBox, value: number)",
		"\t\tsuper.constructor(self, value)",
		"\tfunction NumberBox.double(self: NumberBox): number",
	})

	// A class referenced before its declaration keeps the hoisted local,
	// annotated `any`, and is assigned on its declaration line.
	assertLinesInOrder(t, got, []string{
		"local Late: any",
		"local function makeLate(): Late",
		"\treturn Late.new(1)",
		"export type Late = { n: number }",
		"Late = setmetatable({}, {",
		"\tLate.__index = Late",
		"\tfunction Late.new(n: number): Late",
		"\tfunction Late.constructor(self: Late, n: number)",
	})

	// A named class expression keeps its inner name as a local of the block
	// and assigns the temp afterwards; an unnamed one is the temp itself,
	// typed by the alias named after the assigned variable.
	assertLinesInOrder(t, got, []string{
		"local _class",
		"do",
		"\tlocal Inner = setmetatable({}, {",
		"\tInner.__index = Inner",
		"\tfunction Inner.new(id: number): Inner",
		"\tfunction Inner.constructor(self: Inner, id: number)",
		"\tfunction Inner.twice(self: Inner): number",
		"\t_class = Inner",
		"end",
		"local Named = _class",
		"local _class_1 = setmetatable({}, {",
		"\t_class_1.__index = _class_1",
		"\tfunction _class_1.new(): Anonymous",
		"\t\tlocal self = (setmetatable({}, _class_1) :: any) :: Anonymous",
		"\t\treturn _class_1.constructor(self) or self",
		"\tfunction _class_1.constructor(self: Anonymous)",
		"local Anonymous = _class_1",
		"local function unitCircle(): Circle",
		"\treturn Circle.new(1)",
	})

	for _, name := range []string{"type Animal =", "type Dog =", "type Box<T> =", "type Circle =", "type Shape ="} {
		if count := strings.Count(got, name); count != 1 {
			t.Errorf("%q must be emitted exactly once, got %d:\n%s", name, count, got)
		}
	}
	if match := luauColonMethod.FindString(got); match != "" {
		t.Errorf("luauTypes must never emit a colon-form method, found %q:\n%s", strings.TrimSpace(match), got)
	}
	if strings.Contains(got, "constructor:") {
		t.Errorf("`constructor` must stay out of the instance aliases:\n%s", got)
	}

	// Private and readonly members are exposed (one loss each per class) and
	// the hoisted class records its `any`.
	for _, want := range []string{"PrivateExposed", "ClassReadonlyDropped", "Unspellable"} {
		if !slices.Contains(losses, want) {
			t.Errorf("losses %v must include %s", losses, want)
		}
	}

	requireClean(t, got)
}

func TestLuauTypesEnums(t *testing.T) {
	got, losses := renderLuauTypesLosses(t, "src/enums.ts")

	// The ambient enum is mirrored as a hoisted alias; every other enum gets
	// its alias at the declaration: a string enum is a union and a `read`
	// map, a numeric or mixed enum is `number` (or the union with its string
	// members) and a writable table with the inverse indexer, whose
	// `setmetatable` is cast to `any` until the members are assigned; a
	// const enum is the alias alone.
	assertLinesInOrder(t, got, []string{
		"type Ambient = number",
		"-- Enums (plan WP7): a string enum, a numeric enum, a mixed enum, a const enum",
		`export type Direction = "up" | "down"`,
		`local Direction: { read Up: "up", read Down: "down" } = {`,
		`	Up = "up",`,
		`	Down = "down",`,
		"}",
		"export type Codes = number",
		"local Codes: { A: number, B: number, C: number, [number]: string }",
		"do",
		"\tlocal _inverse = {}",
		"\tCodes = setmetatable({}, {",
		"\t\t__index = _inverse,",
		"\t}) :: any",
		"\tCodes.A = 0",
		`	_inverse[0] = "A"`,
		"end",
		`export type Mixed = number | "two"`,
		`local Mixed: { One: number, Two: "two", [number]: string }`,
		"export type Flags = number",
		"local function flip(direction: Direction): Direction",
		"local function codeName(): string",
		"\treturn Codes[Codes.C]",
		"local function ambient(value: Ambient): number",
	})

	// A string enum referenced before its declaration keeps the hoisted
	// local, annotated with its map type, and is assigned at the
	// declaration after its alias.
	assertLinesInOrder(t, got, []string{
		`local Hoisted: { read Yes: "yes", read No: "no" }`,
		"local function early(): Hoisted",
		"\treturn Hoisted.Yes",
		`export type Hoisted = "yes" | "no"`,
		"Hoisted = {",
		`	Yes = "yes",`,
	})

	for _, unwanted := range []string{"local Flags", "local Ambient", "Flags = Flags"} {
		if strings.Contains(got, unwanted) {
			t.Errorf("output must not contain %q:\n%s", unwanted, got)
		}
	}
	if !slices.Equal(losses, []string{"NumberLiteral"}) {
		t.Errorf("losses = %v, want [NumberLiteral] (the numeric members of the aliases)", losses)
	}

	requireClean(t, got)
}

// With luauTypes off, classes and enums keep rbxtsc's shape: no alias, the
// bare `local` before the `do` block, colon-form constructor and
// `self:constructor(...)`.
func TestLuauTypesClassesOffKeepRbxtscShape(t *testing.T) {
	projDir := filepath.Join("testdata", "luautypes")
	for _, relPath := range []string{"src/classes.ts", "src/enums.ts"} {
		s := buildState(t, projDir, relPath)
		s.Rojo = luauTypesRojoContext(t, projDir)
		got := render.RenderAST(transformer.TransformSourceFile(s))
		if luauAliasStatement.MatchString(got) || strings.Contains(got, ":: any") || strings.Contains(got, "(self: ") {
			t.Errorf("%s: luauTypes off must emit no alias or annotation:\n%s", relPath, got)
		}
		if relPath == "src/classes.ts" {
			requireLines(t, got, []string{
				"local Animal",
				"\tAnimal = setmetatable({}, {",
				"\tfunction Animal.new(...)",
				"\t\tlocal self = setmetatable({}, Animal)",
				"\t\treturn self:constructor(...) or self",
				"\tfunction Animal:constructor(name)",
				"\tfunction Animal:speak()",
				"\tfunction Dog:constructor(name, tricks)",
				"\tfunction Puppy:constructor(...)",
			})
		} else {
			requireLines(t, got, []string{
				"local Codes",
				"\tCodes = setmetatable({}, {",
			})
		}
	}
}
