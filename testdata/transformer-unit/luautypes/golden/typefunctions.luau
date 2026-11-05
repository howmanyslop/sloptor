-- Compiled with sloptor v3.0.0-beta.1
type function __Partial(T: type)
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
type function __Readonly(T: type)
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
type function __Required(T: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	local function withoutnil(value: type): type
		local parts = {}
		for _, component in distribute(value) do
			if not component:is("nil") then
				table.insert(parts, component)
			end
		end
		return join(parts)
	end
	if not T:is("table") then
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
type function __Pick(T: type, K: type)
	if not T:is("table") then
		error("Pick<T, K>: expected a table type")
	end
	local result = types.newtable()
	for key, prop in T:properties() do
		if key:issubtypeof(K) then
			if prop.read then
				result:setreadproperty(key, prop.read)
			end
			if prop.write then
				result:setwriteproperty(key, prop.write)
			end
		end
	end
	return result
end
type function __Omit(T: type, K: type)
	if not T:is("table") then
		error("Omit<T, K>: expected a table type")
	end
	local result = types.newtable()
	for key, prop in T:properties() do
		if not key:issubtypeof(K) then
			if prop.read then
				result:setreadproperty(key, prop.read)
			end
			if prop.write then
				result:setwriteproperty(key, prop.write)
			end
		end
	end
	return result
end
type function __Record(K: type, T: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local result = types.newtable()
	for _, key in distribute(K) do
		if key:is("singleton") then
			result:setproperty(key, T)
		elseif key:is("string") or key:is("number") then
			result:setindexer(key, T)
		elseif not key:is("never") then
			error("Record<K, T>: a mapped key must be a string literal, string or number")
		end
	end
	return result
end
type function __Exclude(T: type, U: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	local parts = {}
	for _, item in distribute(T) do
		if not item:issubtypeof(U) then
			table.insert(parts, item)
		end
	end
	return join(parts)
end
type function __Extract(T: type, U: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	local parts = {}
	for _, item in distribute(T) do
		if item:issubtypeof(U) then
			table.insert(parts, item)
		end
	end
	return join(parts)
end
type function __NonNullable(T: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	local function withoutnil(value: type): type
		local parts = {}
		for _, component in distribute(value) do
			if not component:is("nil") then
				table.insert(parts, component)
			end
		end
		return join(parts)
	end
	return withoutnil(T)
end
type function __ReturnType(T: type)
	local function returnof(fn: type): type
		local returns = fn:returns()
		if returns.head and returns.head[1] then
			return returns.head[1]
		end
		if returns.tail then
			return returns.tail
		end
		return types.singleton(nil)
	end
	if T:is("any") then
		return types.any
	end
	if not T:is("function") then
		error("ReturnType<T>: expected a function type")
	end
	return returnof(T)
end
type function __Parameters(T: type)
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	if not T:is("function") then
		error("Parameters<T>: expected a function type")
	end
	local parameters = T:parameters()
	local parts = {}
	for _, parameter in parameters.head or {} do
		table.insert(parts, parameter)
	end
	if parameters.tail then
		table.insert(parts, parameters.tail)
	end
	local element = join(parts)
	return types.newtable(nil, {
		index = types.number,
		readresult = element,
		writeresult = element,
	})
end
-- Type-level aliases: a generic alias whose body is a mapped, conditional or
-- template literal type becomes a Luau type function; one the translator
-- cannot express becomes `any` with a loss comment.
export type function Partial2(T: type)
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
export type function Required2(T: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	local function withoutnil(value: type): type
		local parts = {}
		for _, component in distribute(value) do
			if not component:is("nil") then
				table.insert(parts, component)
			end
		end
		return join(parts)
	end
	if not T:is("table") then
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
export type function Readonly2(T: type)
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
export type function Elem(T: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	local parts = {}
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
export type function Handlers(T: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function template(parts: { string | type }): type
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
	local result = types.newtable()
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
export type function Stringy(T: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	local parts = {}
	for _, item in distribute(T) do
		if item:issubtypeof(types.string) then
			table.insert(parts, types.singleton("yes"))
		else
			table.insert(parts, types.singleton("no"))
		end
	end
	return join(parts)
end
export type function Returned(T: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	local function returnof(fn: type): type
		local returns = fn:returns()
		if returns.head and returns.head[1] then
			return returns.head[1]
		end
		if returns.tail then
			return returns.tail
		end
		return types.singleton(nil)
	end
	local parts = {}
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
export type function Greeting(T: type)
	local function template(parts: { string | type }): type
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
	return template({ "hello ", T })
end
export type function Keys(T: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	local function keys(value: type): type
		if not value:is("table") then
			error("keyof: expected a table type")
		end
		local parts = {}
		for key in value:properties() do
			table.insert(parts, key)
		end
		return join(parts)
	end
	local parts = {}
	for _, item in distribute(T) do
		if item:issubtypeof(types.string) then
			table.insert(parts, types.never)
		else
			table.insert(parts, keys(item))
		end
	end
	return join(parts)
end
export type function Lookup(T: type, K: type)
	local function distribute(value: type): { type }
		if value:is("union") then
			return value:components()
		end
		if value:is("never") then
			return {}
		end
		return { value }
	end
	local function join(parts: { type }): type
		if #parts == 0 then
			return types.never
		end
		if #parts == 1 then
			return parts[1]
		end
		return types.unionof(table.unpack(parts))
	end
	local function indexof(object: type, key: type): type
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
	local parts = {}
	for _, item in distribute(T) do
		if item:issubtypeof(types.string) then
			table.insert(parts, types.never)
		else
			table.insert(parts, indexof(item, K))
		end
	end
	return join(parts)
end
-- A body that is only `keyof T` / `T[K]` stays an alias over Luau's built-in
-- type functions.
export type Values<T> = index<T, keyof<T>>
export type Box<V> = { value: V }
-- Fallbacks: an `infer` outside the two translated shapes, an `as` clause
-- over another alias, a mapped value and an extends clause naming another
-- type.
-- luauTypes: Unwrap<T> spelled as any: `infer` is only translated in `T extends (infer U)[]` and `T extends (...args: any) => infer R`
export type Unwrap<T> = any
-- luauTypes: Renamed<T> spelled as any: `Uppercase` names a type outside the type function; type-function bodies see no outer names
export type Renamed<T> = any
-- luauTypes: Boxed<T> spelled as any: `Box` names a type outside the type function; type-function bodies see no outer names
export type Boxed<T> = any
-- luauTypes: IsBox<T> spelled as any: `Box` names a type outside the type function; type-function bodies see no outer names
export type IsBox<T> = any
export type Config = { host: string, port: number }
-- Instantiated uses of complete translations keep the call-site spelling.
local function patch(config: Config, partial: Partial2<Config>): Config
	local _object = {}
	local _condition = partial.host
	if _condition == nil then
		_condition = config.host
	end
	local _condition = _condition :: string
	_object.host = _condition
	local _condition_1 = partial.port
	if _condition_1 == nil then
		_condition_1 = config.port
	end
	local _condition_1 = _condition_1 :: number
	_object.port = _condition_1
	return _object
end
local function first(values: { number }): number
	return values[1]
end
local function handlers(): Handlers<"click" | "hover">
	return {
		onclick = function(): () end,
		onhover = function(): () end,
	}
end
local function describe(value: Stringy<string>): string
	return value
end
local function useResult(value: Returned<() -> number>): number
	return value
end
local function greet(value: Greeting<"world">): string
	return value
end
local function required(config: Required2<Config>, frozen: Readonly2<Config>): number
	return config.port + frozen.port
end
local function lookup(key: Keys<Config>, value: Lookup<Config, "port">): Values<Config>
	return if key == "host" then value else value + 1
end
-- Instantiated uses of a fallback expand eagerly; generic uses name it.
local function box(value: { host: Box<string>, port: Box<number> }): Box<string>
	return value.host
end
local function unwrap<T>(value: Unwrap<T>): Unwrap<T>
	return value
end
-- Generic uses of the built-in helpers go through the prelude.
local function fill<T>(partial: __Partial<T>, defaults: T): T
	return defaults
end
local function freeze<T>(value: T): __Readonly<T>
	return value
end
local function needAll<T>(value: __Required<T>): __Required<T>
	return value
end
local function only<T, K>(value: __Pick<T, K>): __Pick<T, K>
	return value
end
local function without<T, K>(value: __Omit<T, K>): __Omit<T, K>
	return value
end
local function record<K, V>(value: __Record<K, V>): __Record<K, V>
	return value
end
local function exclude<T, U>(value: __Exclude<T, U>): __Exclude<T, U>
	return value
end
local function extract<T, U>(value: __Extract<T, U>): __Extract<T, U>
	return value
end
local function defined<T>(value: __NonNullable<T>): __NonNullable<T>
	return value
end
local function returned<T>(value: __ReturnType<T>): __ReturnType<T>
	return value
end
local function parameters<T>(value: __Parameters<T>): __Parameters<T>
	return value
end
-- An instantiated built-in helper expands eagerly without a prelude.
local function partialConfig(value: { host: string?, port: number? }): { host: string }
	local _object = {}
	local _condition = value.host
	if _condition == nil then
		_condition = ""
	end
	local _condition = _condition :: string
	_object.host = _condition
	return _object
end
return {
	patch = patch,
	first = first,
	handlers = handlers,
	describe = describe,
	useResult = useResult,
	greet = greet,
	required = required,
	lookup = lookup,
	box = box,
	unwrap = unwrap,
	fill = fill,
	freeze = freeze,
	needAll = needAll,
	only = only,
	without = without,
	record = record,
	exclude = exclude,
	extract = extract,
	defined = defined,
	returned = returned,
	parameters = parameters,
	partialConfig = partialConfig,
}
