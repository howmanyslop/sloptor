//! `luautypes_class_internal_test.go` and `luautypes_class_test.go`.

use super::super::luautypes::{AliasPlacement, type_alias_for};
use super::super::nativetypes::NativeTypeKind;
use super::emit::{require_clean, require_lines};
use super::{assert_lines_in_order, render_luau_types, render_luau_types_losses, with_fixture};

/// The registry entries classes get: a class declaration is placed at its declaration and
/// exported with the class, a class expression is hoisted under the class name or the assigned
/// variable, and a generic class keeps its generics on the alias.
#[test]
fn luau_types_class_alias_entries() {
	with_fixture("src/classes.ts", |fixture| {
		for (name, exported, placement, generics) in [
			("Animal", true, AliasPlacement::AtDeclaration, 0),
			("Dog", true, AliasPlacement::AtDeclaration, 0),
			("Box", true, AliasPlacement::AtDeclaration, 1),
			("Late", true, AliasPlacement::AtDeclaration, 0),
		] {
			let id = fixture.alias(name);
			let entry = fixture.entry(id);
			assert!(
				entry.exported == exported && entry.placement == placement && entry.generics.len() == generics,
				"{name}: Exported={} Placement={:?} Generics={}, want {exported} {placement:?} {generics}",
				entry.exported,
				entry.placement,
				entry.generics.len()
			);
			let body = entry
				.statement
				.and_then(|statement| fixture.state.ast.get::<rotor_luau::TypeAlias>(statement))
				.and_then(|alias| alias.body);
			assert!(
				body.is_some() && !entry.building && entry.kind == NativeTypeKind::Table,
				"{name}: entry must carry a finished table alias, got {entry:?}"
			);
		}

		// A class expression assigned to a variable is hoisted under the inner class name when
		// it has one, else the variable's name; neither is exported on its own.
		for (variable, alias) in [("Named", "Inner"), ("Anonymous", "Anonymous")] {
			let declaration = fixture.declaration(variable);
			let class = declaration.initializer().expect("an initializer");
			let id = type_alias_for(fixture.state, class.symbol())
				.unwrap_or_else(|| panic!("{variable}: the class expression was not aliased"));
			let entry = fixture.entry(id);
			assert!(
				entry.name == alias && entry.placement == AliasPlacement::Hoisted && !entry.exported,
				"{variable}: Name={:?} Placement={:?} Exported={}, want {alias:?} hoisted unexported",
				entry.name,
				entry.placement,
				entry.exported
			);
		}
	});
}

/// `luauColonMethod`: a colon-form function statement, which luauTypes never emits.
fn colon_method(got: &str) -> Option<&str> {
	got.lines().find(|line| {
		let Some(rest) = line.trim_start().strip_prefix("function ") else {
			return false;
		};
		let name: String = rest
			.chars()
			.take_while(|&c| c.is_ascii_alphanumeric() || c == '_')
			.collect();
		let starts = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
		starts
			&& rest[name.len()..]
				.strip_prefix(':')
				.is_some_and(|tail| tail.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'))
	})
}

/// `luauAliasStatement`: a `type` or `export type` statement.
fn has_alias_statement(got: &str) -> bool {
	got.lines().any(|line| {
		let line = line.strip_prefix("export ").unwrap_or(line);
		line.strip_prefix("type ")
			.is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'))
	})
}

#[test]
#[expect(clippy::too_many_lines, reason = "the Go test, kept whole")]
#[ignore = "needs slice vi: import declarations"]
fn luau_types_classes() {
	let (got, losses) = render_luau_types_losses("src/classes.ts", true);

	// The imported class is mirrored as a hoisted alias (with the interface it implements,
	// which types the `self` of its `area`), the class expressions' aliases are hoisted under
	// the class name or the assigned variable, and the import local is left to inference.
	assert_lines_in_order(
		&got,
		&[
			"local TS = _G[script]",
			"type Inner = { id: number, twice: (self: Inner) -> number }",
			"type Anonymous = { label: string }",
			"type Circle = { kind: ShapeKind, radius: number, area: (self: Shape) -> number }",
			r#"type ShapeKind = "circle" | "square""#,
			"type Shape = { kind: ShapeKind, area: (self: Shape) -> number }",
			"-- Classes (plan WP7): a base class with a static and a toString, a derived",
			r#"local Circle = TS.import(script, script.Parent, "lib", "shapes").Circle"#,
		],
	);

	// A base class: alias at the declaration without `constructor`, private and readonly
	// members as plain fields, the table on the declaration line, `new` through the class
	// table, the constructor and methods in dot form, the static on `typeof(Animal)`,
	// `__tostring` typed, and the static property assigned afterwards.
	assert_lines_in_order(
		&got,
		&[
			"export type Animal = {",
			"\tsecret: number,",
			"\ttag: string?,",
			"\tname: string,",
			"\tspeak: (self: Animal) -> string,",
			"\ttoString: (self: Animal) -> string,",
			"}",
			"local Animal = setmetatable({}, {",
			"\t__tostring = function()",
			r#"		return "Animal""#,
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
		],
	);

	// A derived class re-lists the inherited members flat; an inherited or overridden method
	// keeps `self` typed as the class that first declared it while its own definition takes the
	// derived class. `super` is declared before the table so the heritage expression runs once.
	assert_lines_in_order(
		&got,
		&[
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
		],
	);

	// An implicit derived constructor forwards the inherited construct signature instead of
	// rbxtsc's bare `...`.
	assert_lines_in_order(
		&got,
		&[
			"export type Puppy = {",
			"local super = Dog",
			"\tfunction Puppy.new(name: string, tricks: number): Puppy",
			"\t\treturn Puppy.constructor(self, name, tricks) or self",
			"\tfunction Puppy.constructor(self: Puppy, name: string, tricks: number)",
			"\t\tsuper.constructor(self, name, tricks)",
			"\tend",
		],
	);

	// An abstract class without `extends` stays a plain table and gets no `new`; its abstract
	// method is listed on the alias, and the subclass implementing it keeps the base as that
	// method's `self`.
	assert_lines_in_order(
		&got,
		&[
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
		],
	);
	assert!(
		!got.contains("function Figure.new"),
		"an abstract class must not get `new`:\n{got}"
	);

	// Generic classes: a generic alias, every function re-declaring the class generics (own
	// generics after them), the generic subclass passing its parameter down and an instantiated
	// subclass passing `number`.
	assert_lines_in_order(
		&got,
		&[
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
		],
	);

	// A class referenced before its declaration keeps the hoisted local, annotated `any`, and
	// is assigned on its declaration line.
	assert_lines_in_order(
		&got,
		&[
			"local Late: any",
			"local function makeLate(): Late",
			"\treturn Late.new(1)",
			"export type Late = { n: number }",
			"Late = setmetatable({}, {",
			"\tLate.__index = Late",
			"\tfunction Late.new(n: number): Late",
			"\tfunction Late.constructor(self: Late, n: number)",
		],
	);

	// A named class expression keeps its inner name as a local of the block and assigns the
	// temp afterwards; an unnamed one is the temp itself, typed by the alias named after the
	// assigned variable.
	assert_lines_in_order(
		&got,
		&[
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
		],
	);

	for name in [
		"type Animal =",
		"type Dog =",
		"type Box<T> =",
		"type Circle =",
		"type Shape =",
	] {
		assert_eq!(
			got.matches(name).count(),
			1,
			"{name:?} must be emitted exactly once:\n{got}"
		);
	}
	if let Some(found) = colon_method(&got) {
		panic!(
			"luauTypes must never emit a colon-form method, found {:?}:\n{got}",
			found.trim()
		);
	}
	assert!(
		!got.contains("constructor:"),
		"`constructor` must stay out of the instance aliases:\n{got}"
	);

	// Private and readonly members are exposed (one loss each per class) and the hoisted class
	// records its `any`.
	for want in ["PrivateExposed", "ClassReadonlyDropped", "Unspellable"] {
		assert!(
			losses.iter().any(|loss| loss == want),
			"losses {losses:?} must include {want}"
		);
	}

	require_clean(&got);
}

#[test]
fn luau_types_enums() {
	let (got, losses) = render_luau_types_losses("src/enums.ts", true);

	// The ambient enum is mirrored as a hoisted alias; every other enum gets its alias at the
	// declaration: a string enum is a union and a `read` map, a numeric or mixed enum is
	// `number` (or the union with its string members) and a writable table with the inverse
	// indexer, whose `setmetatable` is cast to `any` until the members are assigned; a const
	// enum is the alias alone.
	assert_lines_in_order(
		&got,
		&[
			"type Ambient = number",
			"-- Enums (plan WP7): a string enum, a numeric enum, a mixed enum, a const enum",
			r#"export type Direction = "up" | "down""#,
			r#"local Direction: { read Up: "up", read Down: "down" } = {"#,
			r#"	Up = "up","#,
			r#"	Down = "down","#,
			"}",
			"export type Codes = number",
			"local Codes: { A: number, B: number, C: number, [number]: string }",
			"do",
			"\tlocal _inverse = {}",
			"\tCodes = setmetatable({}, {",
			"\t\t__index = _inverse,",
			"\t}) :: any",
			"\tCodes.A = 0",
			r#"	_inverse[0] = "A""#,
			"end",
			r#"export type Mixed = number | "two""#,
			r#"local Mixed: { One: number, Two: "two", [number]: string }"#,
			"export type Flags = number",
			"local function flip(direction: Direction): Direction",
			"local function codeName(): string",
			"\treturn Codes[Codes.C]",
			"local function ambient(value: Ambient): number",
		],
	);

	// A string enum referenced before its declaration keeps the hoisted local, annotated with
	// its map type, and is assigned at the declaration after its alias.
	assert_lines_in_order(
		&got,
		&[
			r#"local Hoisted: { read Yes: "yes", read No: "no" }"#,
			"local function early(): Hoisted",
			"\treturn Hoisted.Yes",
			r#"export type Hoisted = "yes" | "no""#,
			"Hoisted = {",
			r#"	Yes = "yes","#,
		],
	);

	for unwanted in ["local Flags", "local Ambient", "Flags = Flags"] {
		assert!(!got.contains(unwanted), "output must not contain {unwanted:?}:\n{got}");
	}
	assert_eq!(losses, vec!["NumberLiteral"], "the numeric members of the aliases");

	require_clean(&got);
}

/// With luauTypes off, classes and enums keep rbxtsc's shape: no alias, the bare `local` before
/// the `do` block, colon-form constructor and `self:constructor(...)`.
#[test]
#[ignore = "needs slice vi: import declarations"]
fn luau_types_classes_off_keep_rbxtsc_shape() {
	for rel_path in ["src/classes.ts", "src/enums.ts"] {
		let got = render_luau_types(rel_path, false);
		assert!(
			!has_alias_statement(&got) && !got.contains(":: any") && !got.contains("(self: "),
			"{rel_path}: luauTypes off must emit no alias or annotation:\n{got}"
		);
		if rel_path == "src/classes.ts" {
			require_lines(
				&got,
				&[
					"local Animal",
					"\tAnimal = setmetatable({}, {",
					"\tfunction Animal.new(...)",
					"\t\tlocal self = setmetatable({}, Animal)",
					"\t\treturn self:constructor(...) or self",
					"\tfunction Animal:constructor(name)",
					"\tfunction Animal:speak()",
					"\tfunction Dog:constructor(name, tricks)",
					"\tfunction Puppy:constructor(...)",
				],
			);
		} else {
			require_lines(&got, &["local Codes", "\tCodes = setmetatable({}, {"]);
		}
	}
}

/// The alias statements of `alias_names`, laid out and rendered as the file renders them.
fn rendered_aliases(fixture: &mut super::Fixture<'_, '_>, names: &[&str]) -> String {
	use super::super::luautypes::types;
	use super::super::luautypes_alias::entry_statements;

	let all = fixture.state.ast.new_list::<rotor_luau::element::Statement>();
	for name in names {
		let declaration = fixture.declaration(name);
		let symbol = if rotor_tsrs::ast::is_variable_declaration(declaration) {
			declaration.initializer().and_then(|initializer| initializer.symbol())
		} else {
			Some(fixture.symbol(name))
		};
		let entry = type_alias_for(fixture.state, symbol).unwrap_or_else(|| panic!("{name} was not aliased"));
		let statements = entry_statements(&mut types(fixture.state), entry);
		fixture.state.ast.list_push_list(all, statements);
	}
	rotor_luau::render::layout_types(&mut fixture.state.ast, all, rotor_luau::render::DEFAULT_TYPE_LINE_WIDTH)
		.expect("the aliases lay out");
	crate::tests::render_list(fixture.state, all)
}

/// The instance aliases of `TestLuauTypesClasses`, straight from the registry.
#[test]
fn luau_types_class_instance_aliases() {
	with_fixture("src/classes.ts", |fixture| {
		fixture.state.is_luau_typed = true;
		let got = rendered_aliases(
			fixture,
			&[
				"Animal",
				"Dog",
				"Puppy",
				"Figure",
				"Square",
				"Box",
				"Pair",
				"NumberBox",
				"Late",
				"Named",
				"Anonymous",
			],
		);
		assert_lines_in_order(
			&got,
			&[
				"export type Animal = {",
				"\tsecret: number,",
				"\ttag: string?,",
				"\tname: string,",
				"\tspeak: (self: Animal) -> string,",
				"\ttoString: (self: Animal) -> string,",
				"}",
				"export type Dog = {",
				"\ttricks: number,",
				"\tspeak: (self: Animal) -> string,",
				"\tboth: (self: Dog) -> string,",
				"\tsecret: number,",
				"\ttag: string?,",
				"\tname: string,",
				"\ttoString: (self: Animal) -> string,",
				"}",
				"export type Puppy = {",
				"export type Figure = { area: (self: Figure) -> number, describe: (self: Figure) -> string }",
				"export type Square = {",
				"\tside: number,",
				"\tarea: (self: Figure) -> number,",
				"\tdescribe: (self: Figure) -> string,",
				"}",
				"export type Box<T> = {",
				"\tvalue: T,",
				"\tget: (self: Box<T>) -> T,",
				"\tmap: <U>(self: Box<T>, transform: (value: T) -> U) -> U,",
				"}",
				"export type Pair<T> = {",
				"\tother: T,",
				"\tswap: (self: Pair<T>) -> (),",
				"\tvalue: T,",
				"\tget: (self: Box<T>) -> T,",
				"\tmap: <U>(self: Box<T>, transform: (value: T) -> U) -> U,",
				"}",
				"export type NumberBox = {",
				"\tdouble: (self: NumberBox) -> number,",
				"\tvalue: number,",
				"\tget: (self: Box<number>) -> number,",
				"\tmap: <U>(self: Box<number>, transform: (value: number) -> U) -> U,",
				"}",
				"export type Late = { n: number }",
				"type Inner = { id: number, twice: (self: Inner) -> number }",
				"type Anonymous = { label: string }",
			],
		);
		assert!(
			!got.contains("constructor:"),
			"`constructor` must stay out of the instance aliases:\n{got}"
		);
	});
}

/// The class table, `new` and `__tostring` lines of `TestLuauTypesClasses`, straight from the
/// `LuauClass` the class port will drive.
#[test]
fn luau_types_class_boilerplate() {
	with_fixture("src/classes.ts", |fixture| {
		fixture.state.is_luau_typed = true;
		let render_class = |fixture: &mut super::Fixture<'_, '_>, name: &str, to_string: bool| {
			let node = fixture.declaration(name);
			let id = fixture.state.ast.id(name);
			let class = super::super::new_luau_class(fixture.state, node, id);
			let generics = class.generic_names();
			super::super::push_type_generics(fixture.state, generics, false);
			let (outer, inner) = class.boilerplate(fixture.state, false);
			if to_string {
				let function = class.to_string_function(fixture.state);
				fixture.state.ast.list_push(inner, function);
			}
			super::super::pop_type_generics(fixture.state);
			fixture.state.ast.list_push_list(outer, inner);
			crate::tests::render_list(fixture.state, outer)
		};
		let animal = render_class(fixture, "Animal", true);
		assert_lines_in_order(
			&animal,
			&[
				"local Animal = setmetatable({}, {",
				"\t__tostring = function()",
				r#"		return "Animal""#,
				"\tend,",
				"})",
				"Animal.__index = Animal",
				"function Animal.new(name: string): Animal",
				"\tlocal self = (setmetatable({}, Animal) :: any) :: Animal",
				"\treturn Animal.constructor(self, name) or self",
				"end",
				"function Animal.__tostring(self: Animal): string",
				"\treturn self:toString()",
			],
		);
		let dog = render_class(fixture, "Dog", false);
		assert_lines_in_order(
			&dog,
			&[
				"local super = Animal",
				"local Dog = setmetatable({}, {",
				"\t__index = super,",
				"})",
				"Dog.__index = Dog",
				"function Dog.new(name: string, tricks: number): Dog",
				"\tlocal self = (setmetatable({}, Dog) :: any) :: Dog",
				"\treturn Dog.constructor(self, name, tricks) or self",
			],
		);
		let puppy = render_class(fixture, "Puppy", false);
		assert_lines_in_order(
			&puppy,
			&[
				"local super = Dog",
				"function Puppy.new(name: string, tricks: number): Puppy",
				"\treturn Puppy.constructor(self, name, tricks) or self",
			],
		);
		let figure = render_class(fixture, "Figure", false);
		assert_lines_in_order(&figure, &["local Figure = {}"]);
		assert!(
			!figure.contains("function Figure.new"),
			"an abstract class must not get `new`:\n{figure}"
		);
		let boxed = render_class(fixture, "Box", false);
		assert_lines_in_order(
			&boxed,
			&[
				"function Box.new<T>(value: T): Box<T>",
				"\tlocal self = (setmetatable({}, Box) :: any) :: Box<T>",
				"\treturn Box.constructor(self, value) or self",
			],
		);
		let diagnostics = fixture.state.diags.flush();
		assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
	});
}

/// The enum aliases and object types of `TestLuauTypesEnums`, straight from the registry.
#[test]
fn luau_types_enum_aliases() {
	with_fixture("src/enums.ts", |fixture| {
		fixture.state.is_luau_typed = true;
		let got = rendered_aliases(fixture, &["Direction", "Codes", "Mixed", "Flags", "Hoisted"]);
		assert_lines_in_order(
			&got,
			&[
				r#"export type Direction = "up" | "down""#,
				"export type Codes = number",
				r#"export type Mixed = number | "two""#,
				"export type Flags = number",
				r#"export type Hoisted = "yes" | "no""#,
			],
		);
		for (name, want) in [
			("Direction", r#"{ read Up: "up", read Down: "down" }"#),
			("Codes", "{ A: number, B: number, C: number, [number]: string }"),
			("Mixed", r#"{ One: number, Two: "two", [number]: string }"#),
			("Hoisted", r#"{ read Yes: "yes", read No: "no" }"#),
		] {
			let declaration = fixture.declaration(name);
			let object = super::super::luautypes_class::luau_enum_object_type(fixture.state, declaration);
			assert_eq!(fixture.text(object), want, "{name}");
		}
	});
}
