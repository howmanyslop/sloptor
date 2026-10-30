-- Compiled with sloptor v3.0.0-beta.1
local TS = _G[script]
-- luauTypes: Promise<TResult1>, Promise<TResult2>, Promise<TResult1 | TResult2>, Promise<TResult>, Promise<T | TResult>, Promise<R>, Promise<U> spelled as any: a recursive alias must be referenced with its own type arguments
type Promise<T> = {
	["then"]: <TResult1, TResult2>(self: Promise<T>, onResolved: ((value: T) -> any)?, onRejected: ((reason: any) -> any)?) -> any,
	andThen: <TResult1, TResult2>(self: Promise<T>, onResolved: ((value: T) -> any)?, onRejected: ((reason: any) -> any)?) -> any,
	catch: <TResult>(self: Promise<T>, onRejected: ((reason: any) -> any)?) -> any,
	tap: (self: Promise<T>, tapHandler: (value: T) -> ()) -> Promise<T>,
	finally: <TResult>(self: Promise<T>, onSettled: (() -> any)?) -> any,
	andThenCall: <P, R>(self: Promise<T>, callback: (...any) -> R, ...any) -> any,
	finallyCall: <P, R>(self: Promise<T>, callback: (...any) -> R, ...any) -> any,
	andThenReturn: <U>(self: Promise<T>, value: U) -> any,
	finallyReturn: <U>(self: Promise<T>, value: U) -> any,
	timeout: (self: Promise<T>, seconds: number, rejectionValue: any) -> Promise<T>,
	cancel: (self: Promise<T>) -> (),
	now: (self: Promise<T>, rejectionValue: any) -> Promise<T>,
	await: (self: Promise<T>) -> ...any,
	awaitStatus: (self: Promise<T>) -> (Promise_Status, unknown),
	expect: (self: Promise<T>) -> T,
	getStatus: (self: Promise<T>) -> Promise_Status,
}
type Promise_Status = "Cancelled" | "Rejected" | "Resolved" | "Started"
type Generator<Yields = unknown, Returns = nil, Next = unknown> = { next: (...any) -> IteratorResult<Yields, Returns> }
type IteratorResult<Yields, Returns = nil> = IteratorReturnResult<Returns> | IteratorYieldResult<Yields>
type IteratorReturnResult<Returns> = { done: true, value: Returns }
type IteratorYieldResult<Yields> = { done: false, value: Yields }
-- Emission sites: parameters (plain, optional, defaulted, rest,
-- destructured, `this`), LuaTuple and void returns, async functions and
-- arrows, a generator, object literals with methods, and nested arrows with
-- generics.
local function plain(a: number, b: string): boolean
	return a > #b
end
local function optionalParams(a: number, b: string?, c: number?): number
	if c == nil then
		c = 2
	end
	local c = c :: number
	return a + (if b == nil then 0 else #b) + c
end
local function restParams(first: number, ...: string): number
	return first + select("#", ...)
end
local function restLoop(...: number): number
	local total = 0
	for _i = 1, select("#", ...) do
		local value = (select(_i, ...))
		total += value
	end
	return total
end
local function restArray(...: string): { string }
	local rest = { ... }
	return rest
end
local function fact(n: number): number
	return if n <= 1 then 1 else n * fact(n - 1)
end
local named = fact
local function destructured(_param: { x: number, y: number }, _param_1: { string }): string
	local x = _param.x
	local y = _param.y
	local first = _param_1[1]
	return `{x + y}{first}`
end
local function thisParam(self: { x: number }, n: number): number
	return self.x + n
end
local function luaTuple(): (number, string)
	return 1, "a"
end
-- Luau packs have no omittable elements: `(number, string?)` still wants two
-- values, so the short return passes `undefined` explicitly.
local function optionalTuple(flag: boolean): (number, string?)
	if flag then
		return 1, "a"
	end
	return 2, nil
end
local function nothing(): ()
	print("x")
end
local function bareReturn(flag: boolean): ...nil
	if flag then
		return nil
	end
	print("x")
end
local fetched: (n: number) -> Promise<number> = TS.async(function(n: number): number
	return n * 2
end)
local fetchArrow: (n: number) -> Promise<number> = TS.async(function(n: number): number
	return n + 1
end)
local awaited: () -> Promise<number> = TS.async(function(): number
	local value: number = TS.await(fetched(1))
	return value
end)
local pair: () -> Promise<{ number }> = TS.async(function(): { number }
	return { 1, 2 }
end)
local summed: () -> Promise<number> = TS.async(function(): number
	local _binding: { number } = TS.await(pair())
	local a = _binding[1]
	local b = _binding[2]
	return a + b
end)
local function counter(limit: number): Generator<number, nil, unknown>
	return TS.generator(function()
		do
			local i = 0
			local _shouldIncrement = false
			while true do
				if _shouldIncrement then
					i += 1
				else
					_shouldIncrement = true
				end
				if not (i < limit) then
					break
				end
				coroutine.yield(i)
			end
		end
	end)
end
export type Counter = { count: number, inc: (self: Counter) -> () }
local typed: Counter = {
	count = 0,
	inc = function(self: Counter): ()
		self.count += 1
	end,
}
local anonymous = {
	count = 0,
	inc = function(self: any): ()
		self.count += 1
	end,
}
local explicit = {
	count = 0,
	inc = function(self: Counter): ()
		self.count += 1
	end,
}
export type Options = { host: string, port: number }
-- `??` spills the literal to a temp; the temp keeps its shape and each
-- `_condition` is re-bound to the non-nil type TS gives the expression.
local function merged(defaults: Options, partial: { host: string?, port: number? }): Options
	local _object = {}
	local _condition = partial.host
	if _condition == nil then
		_condition = defaults.host
	end
	local _condition = _condition :: string
	_object.host = _condition
	local _condition_1 = partial.port
	if _condition_1 == nil then
		_condition_1 = defaults.port
	end
	local _condition_1 = _condition_1 :: number
	_object.port = _condition_1
	return _object
end
local function fallback(name: string?): string
	local _condition = name
	if not (_condition ~= "" and _condition) then
		_condition = "anonymous"
	end
	local _condition = _condition :: string
	return _condition
end
local identity = function<T>(value: T): T
	return value
end
local function nested(): (n: number) -> number
	local double = function(n: number): number
		return n * 2
	end
	return double
end
return {
	plain = plain,
	optionalParams = optionalParams,
	restParams = restParams,
	restLoop = restLoop,
	restArray = restArray,
	destructured = destructured,
	thisParam = thisParam,
	luaTuple = luaTuple,
	optionalTuple = optionalTuple,
	nothing = nothing,
	bareReturn = bareReturn,
	fetched = fetched,
	awaited = awaited,
	pair = pair,
	summed = summed,
	counter = counter,
	merged = merged,
	fallback = fallback,
	nested = nested,
	named = named,
	fetchArrow = fetchArrow,
	typed = typed,
	anonymous = anonymous,
	explicit = explicit,
	identity = identity,
}
