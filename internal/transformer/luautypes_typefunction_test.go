package transformer_test

import (
	"regexp"
	"slices"
	"strings"
	"testing"
)

// Type-function translation (plan WP8) seen through whole-file output: the
// exact `type function` bodies a generic alias with a mapped, conditional or
// template literal body becomes, the `any` fallback with its loss comment,
// how uses are spelled at call sites, the prelude twins of the built-in
// helper types, and, because the analyzer evaluates type functions, that
// the emitted bodies accept the right tables and reject the wrong ones.

// The helpers a body declares before its own statements.
const (
	helperDistributeText = `	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
`
	helperJoinText = `	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
`
	helperWithoutNilText = `	local function withoutnil(value: type): type
		local parts = {}
		for _, component in distribute(value) do
			if not component:is("nil") then
				table.insert(parts, component)
			end
		end
		return join(parts)
	end
`
	helperKeysText = `	local function keys(value: type): type
		if not value:is("table") then
			error("keyof: expected a table type")
		end
		local parts = {}
		for key in value:properties() do
			table.insert(parts, key)
		end
		return join(parts)
	end
`
	helperIndexOfText = `	local function indexof(object: type, key: type): type
		if key:is("union") then
			local parts = {}
			for _, component in key:components() do
				table.insert(parts, indexof(object, component))
			end
			return join(parts)
		end
		if not object:is("table") then
			error("indexed access: expected a table type")
		end
		local value = object:readproperty(key)
		if not value then
			error("indexed access: the property does not exist")
		end
		return value
	end
`
	helperTemplateText = `	local function template(parts: { string | type }): type
		local text = ""
		for _, part in parts do
			if type(part) == "string" then
				text = text .. part
			elseif part:is("singleton") then
				text = text .. tostring(part:value())
			else
				return types.string
			end
		end
		return types.singleton(text)
	end
`
	helperReturnOfText = `	local function returnof(fn: type): type
		local returns = fn:returns()
		if returns.head and returns.head[1] then
			return returns.head[1]
		end
		if returns.tail then
			return returns.tail
		end
		return types.singleton(nil)
	end
`
)

// partial2Body is the plan's worked example, verbatim: a homomorphic mapped
// type copies the table and rewrites each property's read and write slots.
const partial2Body = `export type function Partial2(T: type)
	if not T:is("table") then
		return T
	end
	local result = types.copy(T)
	for key, prop in T:properties() do
		if prop.read then
			result:setreadproperty(key, types.optional(prop.read))
		end
		if prop.write then
			result:setwriteproperty(key, types.optional(prop.write))
		end
	end
	return result
end
`

// typeFunctionBodies are the complete expected blocks, keyed by alias.
var typeFunctionBodies = map[string]string{
	"Partial2": partial2Body,
	// `-?` strips nil through the withoutnil helper.
	"Required2": "export type function Required2(T: type)\n" + helperDistributeText + helperJoinText + helperWithoutNilText + `	if not T:is("table") then
		return T
	end
	local result = types.copy(T)
	for key, prop in T:properties() do
		if prop.read then
			result:setreadproperty(key, withoutnil(prop.read))
		end
		if prop.write then
			result:setwriteproperty(key, withoutnil(prop.write))
		end
	end
	return result
end
`,
	// `readonly` keeps the read slot and clears the write slot.
	"Readonly2": `export type function Readonly2(T: type)
	if not T:is("table") then
		return T
	end
	local result = types.copy(T)
	for key, prop in T:properties() do
		local current = prop.read or prop.write
		if current then
			result:setreadproperty(key, current)
			result:setwriteproperty(key, nil)
		end
	end
	return result
end
`,
	// A conditional over a naked T distributes; `(infer U)[]` is the
	// number indexer's read result.
	"Elem": "export type function Elem(T: type)\n" + helperDistributeText + helperJoinText + `	local parts = {}
	for _, item in distribute(T) do
		local indexer = if item:is("table") then item:indexer() else nil
		if indexer and indexer.index == types.number then
			table.insert(parts, indexer.readresult)
		else
			table.insert(parts, item)
		end
	end
	return join(parts)
end
`,
	// A mapped type over a key union builds a new table; the `as` clause is
	// a template over the key, and `() => void` an empty function type.
	"Handlers": "export type function Handlers(T: type)\n" + helperDistributeText + helperTemplateText + `	local result = types.newtable()
	for _, key in distribute(T) do
		local name = template({ "on", key })
		local value = types.newfunction({}, {})
		if name:is("singleton") then
			result:setproperty(name, value)
		elseif name:is("string") or name:is("number") then
			result:setindexer(name, value)
		elseif not name:is("never") then
			error("Handlers: a mapped key must be a string literal, string or number")
		end
	end
	return result
end
`,
	// A static extends type is a subtyping test; literal branches are
	// singletons.
	"Stringy": "export type function Stringy(T: type)\n" + helperDistributeText + helperJoinText + `	local parts = {}
	for _, item in distribute(T) do
		if item:issubtypeof(types.string) then
			table.insert(parts, types.singleton("yes"))
		else
			table.insert(parts, types.singleton("no"))
		end
	end
	return join(parts)
end
`,
	// `(...args: any) => infer R` binds R to the function's return.
	"Returned": "export type function Returned(T: type)\n" + helperDistributeText + helperJoinText + helperReturnOfText + `	local parts = {}
	for _, item in distribute(T) do
		if item:is("function") then
			local R = returnof(item)
			table.insert(parts, R)
		else
			table.insert(parts, types.never)
		end
	end
	return join(parts)
end
`,
	// A template literal body is one template call.
	"Greeting": "export type function Greeting(T: type)\n" + helperTemplateText + `	return template({ "hello ", T })
end
`,
	// `keyof T` and `T[K]` outside a mapped type go through helpers.
	"Keys": "export type function Keys(T: type)\n" + helperDistributeText + helperJoinText + helperKeysText + `	local parts = {}
	for _, item in distribute(T) do
		if item:issubtypeof(types.string) then
			table.insert(parts, types.never)
		else
			table.insert(parts, keys(item))
		end
	end
	return join(parts)
end
`,
	"Lookup": "export type function Lookup(T: type, K: type)\n" + helperDistributeText + helperJoinText + helperIndexOfText + `	local parts = {}
	for _, item in distribute(T) do
		if item:issubtypeof(types.string) then
			table.insert(parts, types.never)
		else
			table.insert(parts, indexof(item, K))
		end
	end
	return join(parts)
end
`,
}

// typeFunctionBlock matches one `type function` declaration with its body.
var typeFunctionBlock = regexp.MustCompile(`(?ms)^(?:export )?type function (\w+)\(.*?^end\n`)

// typeFunctionBlocks maps every rendered type function to its text.
func typeFunctionBlocks(got string) map[string]string {
	blocks := map[string]string{}
	for _, match := range typeFunctionBlock.FindAllStringSubmatch(got, -1) {
		blocks[match[1]] = match[0]
	}
	return blocks
}

func TestLuauTypesTypeFunctionBodies(t *testing.T) {
	got, _ := renderLuauTypesLosses(t, "src/typefunctions.ts")
	blocks := typeFunctionBlocks(got)
	for name, want := range typeFunctionBodies {
		if blocks[name] != want {
			t.Errorf("%s rendered:\n%s\nwant:\n%s", name, blocks[name], want)
		}
		if count := strings.Count(got, "type function "+name+"("); count != 1 {
			t.Errorf("%s must be declared once, got %d", name, count)
		}
	}
	// A body that is only `T[keyof T]` stays an alias over the built-in
	// type functions, exported at its declaration.
	assertLinesInOrder(t, got, []string{"export type Values<T> = index<T, keyof<T>>"})
	if strings.Contains(got, "type function Values") {
		t.Errorf("Values must not become a type function:\n%s", got)
	}
}

func TestLuauTypesTypeFunctionFallbacks(t *testing.T) {
	got, losses := renderLuauTypesLosses(t, "src/typefunctions.ts")

	// Each fallback is `any` under a comment naming what stopped the
	// translation, with the loss kind of the construct that failed.
	assertLinesInOrder(t, got, []string{
		"-- luauTypes: Unwrap<T> spelled as any: `infer` is only translated in `T extends (infer U)[]` and `T extends (...args: any) => infer R`",
		"export type Unwrap<T> = any",
		"-- luauTypes: Renamed<T> spelled as any: `Uppercase` names a type outside the type function; type-function bodies see no outer names",
		"export type Renamed<T> = any",
		"-- luauTypes: Boxed<T> spelled as any: `Box` names a type outside the type function; type-function bodies see no outer names",
		"export type Boxed<T> = any",
		"-- luauTypes: IsBox<T> spelled as any: `Box` names a type outside the type function; type-function bodies see no outer names",
		"export type IsBox<T> = any",
	})
	for _, name := range []string{"Unwrap", "Renamed", "Boxed", "IsBox"} {
		if strings.Contains(got, "type function "+name+"(") {
			t.Errorf("%s must fall back to an alias:\n%s", name, got)
		}
	}
	for _, want := range []string{"InferType", "KeyRemap", "MappedType", "ConditionalType", "ConstraintDropped"} {
		if !slices.Contains(losses, want) {
			t.Errorf("losses %v must include %s", losses, want)
		}
	}
}

func TestLuauTypesTypeFunctionCallSites(t *testing.T) {
	got, _ := renderLuauTypesLosses(t, "src/typefunctions.ts")

	// A complete translation keeps the call-site spelling at every use,
	// closed arguments included; a return annotation comes from the
	// signature's resolved type, so `Elem<number[]>` there is `number`. A
	// fallback is named at generic uses and expanded eagerly (exactly, from
	// the resolved type) at closed ones. Built-in helpers used generically
	// name their prelude twin; used with closed arguments they expand.
	assertLinesInOrder(t, got, []string{
		"local function patch(config: Config, partial: Partial2<Config>): Config",
		"local function first(values: { number }): number",
		`local function handlers(): Handlers<"click" | "hover">`,
		"local function describe(value: Stringy<string>): string",
		"local function useResult(value: Returned<() -> number>): number",
		`local function greet(value: Greeting<"world">): string`,
		"local function required(config: Required2<Config>, frozen: Readonly2<Config>): number",
		`local function lookup(key: Keys<Config>, value: Lookup<Config, "port">): Values<Config>`,
		"local function box(value: { host: Box<string>, port: Box<number> }): Box<string>",
		"local function unwrap<T>(value: Unwrap<T>): Unwrap<T>",
		"local function fill<T>(partial: __Partial<T>, defaults: T): T",
		"local function freeze<T>(value: T): __Readonly<T>",
		"local function needAll<T>(value: __Required<T>): __Required<T>",
		"local function only<T, K>(value: __Pick<T, K>): __Pick<T, K>",
		"local function without<T, K>(value: __Omit<T, K>): __Omit<T, K>",
		"local function record<K, V>(value: __Record<K, V>): __Record<K, V>",
		"local function exclude<T, U>(value: __Exclude<T, U>): __Exclude<T, U>",
		"local function extract<T, U>(value: __Extract<T, U>): __Extract<T, U>",
		"local function defined<T>(value: __NonNullable<T>): __NonNullable<T>",
		"local function returned<T>(value: __ReturnType<T>): __ReturnType<T>",
		"local function parameters<T>(value: __Parameters<T>): __Parameters<T>",
		"local function partialConfig(value: { host: string?, port: number? }): { host: string }",
	})
}

func TestLuauTypesTypeFunctionPrelude(t *testing.T) {
	got, _ := renderLuauTypesLosses(t, "src/typefunctions.ts")
	blocks := typeFunctionBlocks(got)

	// Every helper used generically has one prelude twin, hoisted after the
	// header (before the file's own leading comment) and never exported.
	names := []string{"Partial", "Required", "Readonly", "Pick", "Omit", "Record", "Exclude", "Extract", "NonNullable", "ReturnType", "Parameters"}
	comment := strings.Index(got, "-- Type-level aliases:")
	for _, name := range names {
		block, ok := blocks["__"+name]
		if !ok {
			t.Errorf("prelude __%s is missing:\n%s", name, got)
			continue
		}
		if strings.HasPrefix(block, "export ") {
			t.Errorf("prelude __%s must not be exported", name)
		}
		if count := strings.Count(got, "type function __"+name+"("); count != 1 {
			t.Errorf("prelude __%s must be declared once, got %d", name, count)
		}
		if index := strings.Index(got, block); comment < 0 || index > comment {
			t.Errorf("prelude __%s must be hoisted before the file comment", name)
		}
	}

	// The prelude twin of Partial is the worked Partial2 body under its
	// own name.
	want := strings.Replace(strings.TrimPrefix(partial2Body, "export "), "Partial2", "__Partial", 1)
	if blocks["__Partial"] != want {
		t.Errorf("__Partial rendered:\n%s\nwant:\n%s", blocks["__Partial"], want)
	}
}

func TestLuauTypesTypeFunctionsAnalyzeClean(t *testing.T) {
	got, _ := renderLuauTypesLosses(t, "src/typefunctions.ts")
	findings := analyzeLuauStrict(t, findLuauAnalyzer(t), got, true)
	if len(findings) != 0 {
		t.Errorf("typefunctions.ts does not analyze clean:\n%s\n--- rendered ---\n%s", strings.Join(findings, "\n"), got)
	}
}

// The analyzer evaluates a type function at each use, so the emitted bodies
// are judged by what they accept: a probe of the worked Partial2 body with
// one right and one wrong table must fail on the wrong one alone.
func TestLuauTypesTypeFunctionPartialProbe(t *testing.T) {
	source := partial2Body + strings.Join([]string{
		"type Config = { host: string, port: number }",
		"local ok: Partial2<Config> = {}",
		`local partial: Partial2<Config> = { host = "x" }`,
		"local primitive: Partial2<number> = 1",
		"local bad: Partial2<Config> = { host = 1 }",
		`local bad2: Partial2<Config> = { port = "1" }`,
		"return nil",
		"",
	}, "\n")
	findings := analyzeLuauStrict(t, findLuauAnalyzer(t), source, false)
	bodyLines := strings.Count(partial2Body, "\n")
	if got, want := luauErrorLines(t, findings), []int{bodyLines + 5, bodyLines + 6}; !slices.Equal(got, want) {
		t.Errorf("error lines: got %v, want %v\n%s", got, want, strings.Join(findings, "\n"))
	}
}

// Every prelude body, taken from the rendered fixture, accepts the tables
// its TS helper would and rejects exactly the deliberate misuses.
func TestLuauTypesTypeFunctionPreludeProbe(t *testing.T) {
	got, _ := renderLuauTypesLosses(t, "src/typefunctions.ts")
	blocks := typeFunctionBlocks(got)
	var prelude strings.Builder
	for _, name := range []string{"Partial", "Required", "Readonly", "Pick", "Omit", "Record", "Exclude", "Extract", "NonNullable", "ReturnType", "Parameters"} {
		block, ok := blocks["__"+name]
		if !ok {
			t.Fatalf("prelude __%s is missing:\n%s", name, got)
		}
		prelude.WriteString(block)
	}
	uses := []string{
		"type Config = { host: string, port: number, read secret: string }",
		`local a: __Pick<Config, "host"> = { host = "x" }`,
		`local b: __Omit<Config, "host" | "secret"> = { port = 1 }`,
		`local c: __Record<"a" | "b", number> = { a = 1, b = 2 }`,
		`local d: __Exclude<"a" | "b", "a"> = "b"`,
		`local e: __Extract<"a" | "b", "a"> = "a"`,
		`local f: __NonNullable<string?> = "s"`,
		"local g: __ReturnType<() -> number> = 1",
		`local h: __Parameters<(number, string) -> ()> = { 1, "s" }`,
		`local i: __Required<{ host: string? }> = { host = "x" }`,
		`local j: __Readonly<Config> = { host = "x", port = 1, secret = "s" }`,
		"local k: __Partial<Config> = {}",
		"local l: __Record<string, boolean> = { anything = true }",
		// Deliberate misuses, one per line, from here on.
		`local bad1: __Pick<Config, "host"> = { port = 1 }`,
		`local bad2: __Record<"a", number> = { a = "s" }`,
		`local bad3: __Exclude<"a" | "b", "a"> = "a"`,
		`local bad4: __ReturnType<() -> number> = "s"`,
		"local bad5: __Required<{ host: string? }> = {}",
		`j.host = "y"`,
		"local bad7: __Partial<Config> = { host = 1 }",
		"return nil",
		"",
	}
	source := prelude.String() + strings.Join(uses, "\n")
	findings := analyzeLuauStrict(t, findLuauAnalyzer(t), source, false)
	first := strings.Count(prelude.String(), "\n") + slices.Index(uses, `local bad1: __Pick<Config, "host"> = { port = 1 }`) + 1
	want := []int{first, first + 1, first + 2, first + 3, first + 4, first + 5, first + 6}
	if got := luauErrorLines(t, findings); !slices.Equal(got, want) {
		t.Errorf("error lines: got %v, want %v\n%s", got, want, strings.Join(findings, "\n"))
	}
}
