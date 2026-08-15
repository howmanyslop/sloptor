-- Compiled with sloptor v2.3.1
local TS = require(script.Parent.include.RuntimeLib)
local fetchValue
fetchValue = TS.async(function(x)
	local a = TS.await(TS.Promise.resolve(x))
	local b = TS.await(fetchValue(a))
	return a + b
end)
local asyncArrow = TS.async(function(n)
	return n * 2
end)
local exported = TS.async(function()
	TS.await(asyncArrow(1))
end)
local function gen()
	return TS.generator(function()
		coroutine.yield(1)
		coroutine.yield(2)
		return "done"
	end)
end
local function gen2()
	return TS.generator(function()
		coroutine.yield()
		local _returnValue
		for _result in gen().next do
			if _result.done then
				_returnValue = _result.value
				break
			end
			coroutine.yield(_result.value)
		end
		local v = _returnValue
		print(v)
	end)
end
local function useGen()
	for _result in gen2().next do
		if _result.done then
			break
		end
		local x = _result.value
		print(x)
	end
end
local lateAsync
local function early()
	return lateAsync()
end
lateAsync = TS.async(function()
	return 1
end)
local Foo
do
	Foo = setmetatable({}, {
		__tostring = function()
			return "Foo"
		end,
	})
	Foo.__index = Foo
	function Foo.new(...)
		local self = setmetatable({}, Foo)
		return self:constructor(...) or self
	end
	function Foo:constructor()
	end
	Foo.work = TS.async(function(self, n)
		return (TS.await(TS.Promise.resolve(n))) + 1
	end)
	Foo.make = TS.async(function(self)
		return Foo.new()
	end)
	function Foo:counter()
		return TS.generator(function()
			coroutine.yield(1)
			coroutine.yield(2)
		end)
	end
	function Foo:names()
		return TS.generator(function()
			coroutine.yield("a")
		end)
	end
	Foo["computed " .. "key"] = TS.async(function(self)
		return 1
	end)
end
local obj = {
	work = TS.async(function(self, n)
		return (TS.await(TS.Promise.resolve(n))) + 1
	end),
	counter = function(self)
		return TS.generator(function()
			coroutine.yield(1)
		end)
	end,
}
local fexpr = TS.async(function(x)
	return x
end)
local genExpr = function()
	return TS.generator(function()
		coroutine.yield(5)
	end)
end
local function outerStmt()
	return TS.generator(function()
		for _result in gen().next do
			if _result.done then
				break
			end
			coroutine.yield(_result.value)
		end
		coroutine.yield()
	end)
end
local chainBin = TS.async(function(a)
	local x = 1 + (TS.await(a()))
	if (TS.await(a())) > 0 then
		TS.await(a())
	end
	TS.await(a())
	return x
end)
local arrowExprBody = TS.async(function(n)
	return n + (TS.await(TS.Promise.resolve(1)))
end)
local function resume()
	local g = (function()
		return TS.generator(function()
			local got = coroutine.yield(1)
			print(got)
		end)
	end)()
	g.next()
	g.next(5)
end
local function use()
	local f = Foo.new()
	print(f:work(1), Foo:make(), f:counter(), Foo:names())
	print(fexpr(2), genExpr(), outerStmt(), arrowExprBody(1))
end
return {
	exported = exported,
	useGen = useGen,
	early = early,
	chainBin = chainBin,
	resume = resume,
	use = use,
	obj = obj,
}
