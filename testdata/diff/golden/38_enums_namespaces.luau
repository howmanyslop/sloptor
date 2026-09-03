-- Compiled with sloptor v2.3.3
local Fruit
do
	local _inverse = {}
	Fruit = setmetatable({}, {
		__index = _inverse,
	})
	Fruit.Apple = 0
	_inverse[0] = "Apple"
	Fruit.Banana = 1
	_inverse[1] = "Banana"
	Fruit.Cherry = 2
	_inverse[2] = "Cherry"
end
local Mixed
do
	local _inverse = {}
	Mixed = setmetatable({}, {
		__index = _inverse,
	})
	Mixed.A = 5
	_inverse[5] = "A"
	Mixed.B = 6
	_inverse[6] = "B"
	Mixed.C = 10
	_inverse[10] = "C"
end
local Color = {
	Red = "red",
	Green = "green",
}
local Hetero
do
	local _inverse = {}
	Hetero = setmetatable({}, {
		__index = _inverse,
	})
	Hetero.Num = 1
	_inverse[1] = "Num"
	Hetero.Str = "str"
end
local base = 10
local Computed
do
	local _inverse = {}
	Computed = setmetatable({}, {
		__index = _inverse,
	})
	Computed.X = 20
	_inverse[20] = "X"
	local _value = #"y"
	Computed.Y = _value
	_inverse[_value] = "Y"
end
local Exported
do
	local _inverse = {}
	Exported = setmetatable({}, {
		__index = _inverse,
	})
	Exported.E1 = 0
	_inverse[0] = "E1"
	Exported.E2 = 1
	_inverse[1] = "E2"
end
local Weird
do
	local _inverse = {}
	Weird = setmetatable({}, {
		__index = _inverse,
	})
	Weird["a b"] = 1
	_inverse[1] = "a b"
	Weird.ok = 2
	_inverse[2] = "ok"
end
local E
local function early()
	return E.A
end
do
	local _inverse = {}
	E = setmetatable({}, {
		__index = _inverse,
	})
	E.A = 0
	_inverse[0] = "A"
end
local SE
local function earlyStr()
	return SE.S
end
SE = {
	S = "s",
}
local function uses()
	print(Fruit.Apple, Fruit[1], Mixed.B, Color.Red, Hetero.Str, Computed.X, Computed.Y, Weird.ok)
	print(0, 1)
	print(Exported.E2)
end
local Outer = {}
do
	local _container = Outer
	local value = 1
	_container.value = value
	_container.mut = 2
	local function fn()
		return value
	end
	_container.fn = fn
	local function bump()
		_container.mut += 1
		return _container.mut
	end
	_container.bump = bump
	local hidden = 3
	local Inner = {}
	do
		local _container_1 = Inner
		local deep = hidden
		_container_1.deep = deep
	end
	_container.Inner = Inner
end
local function useNs()
	print(Outer.value, Outer.fn(), Outer.Inner.deep, Outer.mut, Outer.bump())
	Outer.mut = 5
end
local NoExports = {}
do
	local x = 1
	print(x)
end
local AB = {}
do
	local _container = AB
	local CD = {}
	do
		local _container_1 = CD
		local v = 1
		_container_1.v = v
	end
	_container.CD = CD
end
local function useAB()
	print(AB.CD.v)
end
local NS
local function earlyNs()
	return NS.v
end
NS = {}
do
	local _container = NS
	local v = 7
	_container.v = v
end
local HasEnum = {}
do
	local _container = HasEnum
	local Inner2
	do
		local _inverse = {}
		Inner2 = setmetatable({}, {
			__index = _inverse,
		})
		Inner2.X = 0
		_inverse[0] = "X"
	end
	_container.Inner2 = Inner2
	local c = Inner2.X
	_container.c = c
end
local function useHasEnum()
	print(HasEnum.c)
end
local ExpNs = {}
do
	local _container = ExpNs
	local e = 1
	_container.e = e
	local p = 1
	local q = 2
	_container.p = p
	_container.q = q
end
return {
	early = early,
	earlyStr = earlyStr,
	uses = uses,
	useNs = useNs,
	useAB = useAB,
	earlyNs = earlyNs,
	useHasEnum = useHasEnum,
	Exported = Exported,
	ExpNs = ExpNs,
}
