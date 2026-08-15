-- Compiled with sloptor v2.3.1
local TS = require(script.Parent.include.RuntimeLib)
local function getSet()
	return {
		["a"] = true,
		["b"] = true,
	}
end
local function getMap()
	return {
		k = 1,
	}
end
-- Set: READONLY_SET_MAP_SHARED (size loop-count, has, isEmpty)
local set = {
	[1] = true,
	[2] = true,
	[3] = true,
}
-- ▼ ReadonlySet.size ▼
local _size = 0
for _ in set do
	_size += 1
end
-- ▲ ReadonlySet.size ▲
print(_size, next(set) == nil, set[2] ~= nil)
-- ▼ ReadonlySet.size ▼
local _size_1 = 0
for _ in getSet() do
	_size_1 += 1
end
-- ▲ ReadonlySet.size ▲
print(_size_1, next(getSet()) == nil, getSet().a ~= nil)
-- Set.add: statement vs value (chaining) position
set[4] = true
set[5] = true
set[6] = true
local chained = set
print(chained)
-- SET_MAP_SHARED delete: statement vs value position
set[1] = nil
-- ▼ Set.delete ▼
local _valueExisted = set[2] ~= nil
set[2] = nil
-- ▲ Set.delete ▲
local removed = _valueExisted
print(removed)
local _exp = getSet()
-- ▼ Set.delete ▼
local _valueExisted_1 = _exp.a ~= nil
_exp.a = nil
-- ▲ Set.delete ▲
print(_valueExisted_1)
-- Set.forEach: callback receives (value, value2, set) — the value TWICE
-- ▼ ReadonlySet.forEach ▼
local _callback = function(value, value2, container)
	local _exp_1 = value
	local _exp_2 = value2
	-- ▼ ReadonlySet.size ▼
	local _size_2 = 0
	for _ in container do
		_size_2 += 1
	end
	-- ▲ ReadonlySet.size ▲
	return print(_exp_1, _exp_2, _size_2)
end
for _v in set do
	_callback(_v, _v, set)
end
-- ▲ ReadonlySet.forEach ▲
-- ▼ ReadonlySet.forEach ▼
local _callback_1 = function(v)
	return print(v)
end
for _v in set do
	_callback_1(_v, _v, set)
end
-- ▲ ReadonlySet.forEach ▲
local _exp_1 = getSet()
-- ▼ ReadonlySet.forEach ▼
local _callback_2 = function(v)
	return print(v)
end
for _v in _exp_1 do
	_callback_2(_v, _v, _exp_1)
end
-- ▲ ReadonlySet.forEach ▲
-- ReadonlySet
local rs = {
	["x"] = true,
	["y"] = true,
}
-- ▼ ReadonlySet.size ▼
local _size_2 = 0
for _ in rs do
	_size_2 += 1
end
-- ▲ ReadonlySet.size ▲
print(_size_2, next(rs) == nil, rs.x ~= nil)
-- ▼ ReadonlySet.forEach ▼
local _callback_3 = function(a, b)
	return print(a, b)
end
for _v in rs do
	_callback_3(_v, _v, rs)
end
-- ▲ ReadonlySet.forEach ▲
-- SET_MAP_SHARED clear
table.clear(set)
-- Map: shared methods + get/set
local map = {
	a = 1,
	b = 2,
}
-- ▼ ReadonlyMap.size ▼
local _size_3 = 0
for _ in map do
	_size_3 += 1
end
-- ▲ ReadonlyMap.size ▲
print(_size_3, next(map) == nil, map.a ~= nil, (map.a))
-- ▼ ReadonlyMap.size ▼
local _size_4 = 0
for _ in getMap() do
	_size_4 += 1
end
-- ▲ ReadonlyMap.size ▲
print(_size_4, (getMap().k))
-- Map.set: statement vs value (chaining) position
map.c = 3
map.d = 4
map.e = 5
local mchained = map
print(mchained)
-- Map delete / clear
map.a = nil
-- ▼ Map.delete ▼
local _valueExisted_2 = map.b ~= nil
map.b = nil
-- ▲ Map.delete ▲
local mremoved = _valueExisted_2
print(mremoved)
-- Map.forEach: callback receives (value, key, map) — VALUE first
-- ▼ ReadonlyMap.forEach ▼
local _callback_4 = function(value, key, container)
	local _exp_2 = key
	local _exp_3 = value
	-- ▼ ReadonlyMap.size ▼
	local _size_5 = 0
	for _ in container do
		_size_5 += 1
	end
	-- ▲ ReadonlyMap.size ▲
	return print(_exp_2, _exp_3, _size_5)
end
for _k, _v in map do
	_callback_4(_v, _k, map)
end
-- ▲ ReadonlyMap.forEach ▲
-- ▼ ReadonlyMap.forEach ▼
local _callback_5 = function(v)
	return print(v)
end
for _k, _v in map do
	_callback_5(_v, _k, map)
end
-- ▲ ReadonlyMap.forEach ▲
local _exp_2 = getMap()
-- ▼ ReadonlyMap.forEach ▼
local _callback_6 = function(v, k)
	return print(k, v)
end
for _k, _v in _exp_2 do
	_callback_6(_v, _k, _exp_2)
end
-- ▲ ReadonlyMap.forEach ▲
table.clear(map)
-- ReadonlyMap
local rm = {
	k = 9,
}
local _exp_3 = rm.k
local _exp_4 = rm.k ~= nil
-- ▼ ReadonlyMap.size ▼
local _size_5 = 0
for _ in rm do
	_size_5 += 1
end
-- ▲ ReadonlyMap.size ▲
print(_exp_3, _exp_4, _size_5, next(rm) == nil)
-- ▼ ReadonlyMap.forEach ▼
local _callback_7 = function(v, k)
	return print(k, v)
end
for _k, _v in rm do
	_callback_7(_v, _k, rm)
end
-- ▲ ReadonlyMap.forEach ▲
-- WeakSet/WeakMap reuse the Set/Map method symbols through inheritance
local ws = setmetatable({}, {
	__mode = "k",
})
local tag = {
	id = 1,
}
ws[tag] = true
print(ws[tag] ~= nil)
local wm = setmetatable({}, {
	__mode = "k",
})
wm[tag] = 5
print((wm[tag]))
-- Promise: identifier macro (TS.Promise) + then -> andThen; cancel is a REAL
-- runtime method (not macro-only), so it emits a plain method call
local promise = TS.Promise.new(function(resolve)
	return resolve(1)
end)
local p2 = promise:andThen(function(v)
	return v + 1
end)
print(p2)
promise:andThen(function(v)
	return print(v)
end)
promise:cancel()
local p3 = TS.Promise.resolve(7)
print(p3)
-- CALL_MACROS
-- assert: boolean condition (no truthiness expansion) + number|undefined
-- condition with message (full 0/NaN/nil expansion)
local _arg0 = set[3] ~= nil or true
assert(_arg0)
local function check(x)
	local _x = x
	assert(_x ~= 0 and _x == _x and _x, "no x")
	return x
end
print(check(3))
-- typeOf
print(typeof(5), typeof("x"), typeof(set))
-- typeIs: primitive Luau type string -> type(); Roblox type -> typeof()
local u = 4
if type(u) == "number" then
	print(u + 1)
end
if typeof(u) == "Vector3" then
	print(u.X)
end
-- classIs: ClassName comparison
local inst = Instance.new("Folder")
if inst.ClassName == "Part" then
	print(inst.Size)
end
-- identity compiles to its argument
print(7)
local idArr = { 1, 2 }
print(idArr)
-- $tuple: multi-value return
local function pair()
	return 1, "a"
end
local num, str = pair()
print(num, str)
-- $getModuleTree: { root, { "rest", "of", "path" } }
print({ script.Parent, { "24a_shared" } })
return nil
