-- Compiled with sloptor v2.4.0
-- classes: declarations, expressions, inheritance, statics, abstract,
-- parameter properties, computed names, hoisting, toString/__tostring
local Animal
do
	Animal = setmetatable({}, {
		__tostring = function()
			return "Animal"
		end,
	})
	Animal.__index = Animal
	function Animal.new(...)
		local self = setmetatable({}, Animal)
		return self:constructor(...) or self
	end
	function Animal:constructor(name)
		self.legs = 4
		self.name = name
	end
	function Animal:walk(dist)
		print(self.name, dist)
	end
	function Animal.describe()
		print("callback-style method")
	end
	Animal["with space"] = function(self)
		return 1
	end
	function Animal:create()
		return Animal.new("a")
	end
	Animal.VERSION = 1
end
local Point
do
	Point = setmetatable({}, {
		__tostring = function()
			return "Point"
		end,
	})
	Point.__index = Point
	function Point.new(...)
		local self = setmetatable({}, Point)
		return self:constructor(...) or self
	end
	function Point:constructor(x, y)
		if y == nil then
			y = 10
		end
		self.x = x
		self.y = y
	end
	function Point:toString()
		return `({self.x}, {self.y})`
	end
	function Point:lengthSquared()
		return self.x * self.x + self.y * self.y
	end
	function Point:__tostring()
		return self:toString()
	end
end
local Point3
do
	local super = Point
	Point3 = setmetatable({}, {
		__tostring = function()
			return "Point3"
		end,
		__index = super,
	})
	Point3.__index = Point3
	function Point3.new(...)
		local self = setmetatable({}, Point3)
		return self:constructor(...) or self
	end
	function Point3:constructor(x, z)
		print("pre-super")
		super.constructor(self, x)
		self.z = z
		print("post-super", self.z)
	end
	function Point3:lengthSquared()
		return super.lengthSquared(self) + self.z * self.z
	end
	function Point3:manhattan()
		return super.lengthSquared(self)
	end
	function Point3:__tostring()
		return self:toString()
	end
end
local Inherited
do
	local super = Point
	Inherited = setmetatable({}, {
		__tostring = function()
			return "Inherited"
		end,
		__index = super,
	})
	Inherited.__index = Inherited
	function Inherited.new(...)
		local self = setmetatable({}, Inherited)
		return self:constructor(...) or self
	end
	function Inherited:constructor(...)
		super.constructor(self, ...)
	end
	function Inherited:__tostring()
		return self:toString()
	end
end
local Shape
do
	Shape = {}
	function Shape:constructor()
	end
	function Shape:describe()
		return "shape area: " .. tostring(self:area())
	end
end
local NamedShape
do
	local super = Point
	NamedShape = setmetatable({}, {
		__tostring = function()
			return "NamedShape"
		end,
		__index = super,
	})
	NamedShape.__index = NamedShape
	function NamedShape:constructor(...)
		super.constructor(self, ...)
	end
	function NamedShape:__tostring()
		return self:toString()
	end
end
local Square
do
	local super = Shape
	Square = setmetatable({}, {
		__tostring = function()
			return "Square"
		end,
		__index = super,
	})
	Square.__index = Square
	function Square.new(...)
		local self = setmetatable({}, Square)
		return self:constructor(...) or self
	end
	function Square:constructor(side)
		super.constructor(self)
		self.side = side
	end
	function Square:area()
		return self.side * self.side
	end
end
local _class
do
	local Inner = setmetatable({}, {
		__tostring = function()
			return "Inner"
		end,
	})
	Inner.__index = Inner
	function Inner.new(...)
		local self = setmetatable({}, Inner)
		return self:constructor(...) or self
	end
	function Inner:constructor()
		self.tag = "inner"
	end
	function Inner:self()
		return Inner
	end
	_class = Inner
end
local Runtime = _class
local _class_1
do
	_class_1 = setmetatable({}, {
		__tostring = function()
			return "Anonymous"
		end,
	})
	_class_1.__index = _class_1
	function _class_1.new(...)
		local self = setmetatable({}, _class_1)
		return self:constructor(...) or self
	end
	function _class_1:constructor()
	end
	function _class_1:value()
		return 42
	end
end
local Anon = _class_1
local Registry
do
	Registry = setmetatable({}, {
		__tostring = function()
			return "Registry"
		end,
	})
	Registry.__index = Registry
	function Registry.new(...)
		local self = setmetatable({}, Registry)
		return self:constructor(...) or self
	end
	function Registry:constructor()
	end
	Registry.entries = 0
	do
		Registry.entries = 1
		print(Registry.entries)
	end
end
local KEY = "dynamic"
local Computed
do
	Computed = setmetatable({}, {
		__tostring = function()
			return "Computed"
		end,
	})
	Computed.__index = Computed
	function Computed.new(...)
		local self = setmetatable({}, Computed)
		return self:constructor(...) or self
	end
	function Computed:constructor()
	end
	Computed[KEY] = function(self)
		return "computed"
	end
end
local Hoisted
local function early()
	return Hoisted.new()
end
do
	Hoisted = setmetatable({}, {
		__tostring = function()
			return "Hoisted"
		end,
	})
	Hoisted.__index = Hoisted
	function Hoisted.new(...)
		local self = setmetatable({}, Hoisted)
		return self:constructor(...) or self
	end
	function Hoisted:constructor()
		self.when = "later"
	end
end
local default
do
	default = setmetatable({}, {
		__tostring = function()
			return "default"
		end,
	})
	default.__index = default
	function default.new(...)
		local self = setmetatable({}, default)
		return self:constructor(...) or self
	end
	function default:constructor()
	end
	function default:run()
		return "default"
	end
end
local animal = Animal:create()
animal:walk(Animal.VERSION)
animal.describe()
print(animal["with space"](animal))
local p3 = Point3.new(1, 2)
print(tostring(p3), p3:lengthSquared(), p3:manhattan(), Inherited.new(5))
print(Square.new(3):describe(), NamedShape)
print(Runtime.new():self(), Anon.new():value(), Registry.entries)
local _computed = Computed.new()
print(_computed[KEY](_computed), early().when)
return {
	Animal = Animal,
	default = default,
}
