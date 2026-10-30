-- Compiled with sloptor v3.0.0-beta.1
local TS = _G[script]
type Circle = { kind: ShapeKind, radius: number, area: (self: Shape) -> number }
type ShapeKind = "circle" | "square"
type Shape = { kind: ShapeKind, area: (self: Shape) -> number }
type Inner = { id: number, twice: (self: Inner) -> number }
type Anonymous = { label: string }
-- Classes (plan WP7): a base class with a static and a toString, a derived
-- class with an override and a super call, an abstract base with a concrete
-- subclass, a generic class with a generic method and a generic subclass, a
-- class referenced before its declaration (hoisted), class expressions, and
-- an imported class constructed through its mirrored alias.
local Circle: { new: (radius: number) -> Circle } = TS.import(script, script.Parent, "lib", "shapes").Circle
export type Animal = {
	secret: number,
	tag: string?,
	name: string,
	speak: (self: Animal) -> string,
	toString: (self: Animal) -> string,
}
local Animal = setmetatable({}, {
	__tostring = function()
		return "Animal"
	end,
})
do
	Animal.__index = Animal
	function Animal.new(name: string): Animal
		local self = (setmetatable({}, Animal) :: any) :: Animal
		return Animal.constructor(self, name) or self
	end
	function Animal.constructor(self: Animal, name: string)
		self.name = name
		self.secret = 1
		Animal.count += 1
	end
	function Animal.speak(self: Animal): string
		return self.name
	end
	function Animal.toString(self: Animal): string
		return `Animal({self.name})`
	end
	function Animal.create(self: typeof(Animal), name: string): Animal
		return Animal.new(name)
	end
	function Animal.__tostring(self: Animal): string
		return self:toString()
	end
	Animal.count = 0
end
export type Dog = {
	tricks: number,
	speak: (self: Animal) -> string,
	both: (self: Dog) -> string,
	secret: number,
	tag: string?,
	name: string,
	toString: (self: Animal) -> string,
}
local super = Animal
local Dog = setmetatable({}, {
	__tostring = function()
		return "Dog"
	end,
	__index = super,
})
do
	Dog.__index = Dog
	function Dog.new(name: string, tricks: number): Dog
		local self = (setmetatable({}, Dog) :: any) :: Dog
		return Dog.constructor(self, name, tricks) or self
	end
	function Dog.constructor(self: Dog, name: string, tricks: number)
		super.constructor(self, name)
		self.tricks = tricks
	end
	function Dog.speak(self: Dog): string
		return `{self.name}!`
	end
	function Dog.both(self: Dog): string
		return `{self:speak()} {self.tricks}`
	end
	function Dog.__tostring(self: Dog): string
		return self:toString()
	end
end
export type Puppy = {
	secret: number,
	tag: string?,
	name: string,
	toString: (self: Animal) -> string,
	tricks: number,
	speak: (self: Animal) -> string,
	both: (self: Dog) -> string,
}
local super = Dog
local Puppy = setmetatable({}, {
	__tostring = function()
		return "Puppy"
	end,
	__index = super,
})
do
	Puppy.__index = Puppy
	function Puppy.new(name: string, tricks: number): Puppy
		local self = (setmetatable({}, Puppy) :: any) :: Puppy
		return Puppy.constructor(self, name, tricks) or self
	end
	function Puppy.constructor(self: Puppy, name: string, tricks: number)
		super.constructor(self, name, tricks)
	end
	function Puppy.__tostring(self: Puppy): string
		return self:toString()
	end
end
export type Figure = { area: (self: Figure) -> number, describe: (self: Figure) -> string }
local Figure = {}
do
	function Figure.constructor(self: Figure)
	end
	function Figure.describe(self: Figure): string
		return `{self:area()}`
	end
end
export type Square = {
	side: number,
	area: (self: Figure) -> number,
	describe: (self: Figure) -> string,
}
local super = Figure
local Square = setmetatable({}, {
	__tostring = function()
		return "Square"
	end,
	__index = super,
})
do
	Square.__index = Square
	function Square.new(side: number): Square
		local self = (setmetatable({}, Square) :: any) :: Square
		return Square.constructor(self, side) or self
	end
	function Square.constructor(self: Square, side: number)
		super.constructor(self)
		self.side = side
	end
	function Square.area(self: Square): number
		return self.side * self.side
	end
end
export type Box<T> = {
	value: T,
	get: (self: Box<T>) -> T,
	map: <U>(self: Box<T>, transform: (value: T) -> U) -> U,
}
local Box = setmetatable({}, {
	__tostring = function()
		return "Box"
	end,
})
do
	Box.__index = Box
	function Box.new<T>(value: T): Box<T>
		local self = (setmetatable({}, Box) :: any) :: Box<T>
		return Box.constructor(self, value) or self
	end
	function Box.constructor<T>(self: Box<T>, value: T)
		self.value = value
	end
	function Box.get<T>(self: Box<T>): T
		return self.value
	end
	function Box.map<T, U>(self: Box<T>, transform: (value: T) -> U): U
		return transform(self.value)
	end
end
export type Pair<T> = {
	other: T,
	swap: (self: Pair<T>) -> (),
	value: T,
	get: (self: Box<T>) -> T,
	map: <U>(self: Box<T>, transform: (value: T) -> U) -> U,
}
local super = Box
local Pair = setmetatable({}, {
	__tostring = function()
		return "Pair"
	end,
	__index = super,
})
do
	Pair.__index = Pair
	function Pair.new<T>(value: T, other: T): Pair<T>
		local self = (setmetatable({}, Pair) :: any) :: Pair<T>
		return Pair.constructor(self, value, other) or self
	end
	function Pair.constructor<T>(self: Pair<T>, value: T, other: T)
		super.constructor(self, value)
		self.other = other
	end
	function Pair.swap<T>(self: Pair<T>): ()
		local value = self.value
		self.value = self.other
		self.other = value
	end
end
export type NumberBox = {
	double: (self: NumberBox) -> number,
	value: number,
	get: (self: Box<number>) -> number,
	map: <U>(self: Box<number>, transform: (value: number) -> U) -> U,
}
local super = Box
local NumberBox = setmetatable({}, {
	__tostring = function()
		return "NumberBox"
	end,
	__index = super,
})
do
	NumberBox.__index = NumberBox
	function NumberBox.new(value: number): NumberBox
		local self = (setmetatable({}, NumberBox) :: any) :: NumberBox
		return NumberBox.constructor(self, value) or self
	end
	function NumberBox.constructor(self: NumberBox, value: number)
		super.constructor(self, value)
	end
	function NumberBox.double(self: NumberBox): number
		return self:get() * 2
	end
end
local Late: any
local function makeLate(): Late
	return Late.new(1)
end
export type Late = { n: number }
Late = setmetatable({}, {
	__tostring = function()
		return "Late"
	end,
})
do
	Late.__index = Late
	function Late.new(n: number): Late
		local self = (setmetatable({}, Late) :: any) :: Late
		return Late.constructor(self, n) or self
	end
	function Late.constructor(self: Late, n: number)
		self.n = n
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
	function Inner.new(id: number): Inner
		local self = (setmetatable({}, Inner) :: any) :: Inner
		return Inner.constructor(self, id) or self
	end
	function Inner.constructor(self: Inner, id: number)
		self.id = id
	end
	function Inner.twice(self: Inner): number
		return self.id * 2
	end
	_class = Inner
end
local Named = _class
local _class_1 = setmetatable({}, {
	__tostring = function()
		return "Anonymous"
	end,
})
do
	_class_1.__index = _class_1
	function _class_1.new(): Anonymous
		local self = (setmetatable({}, _class_1) :: any) :: Anonymous
		return _class_1.constructor(self) or self
	end
	function _class_1.constructor(self: Anonymous)
		self.label = "anonymous"
	end
end
local Anonymous = _class_1
local function unitCircle(): Circle
	return Circle.new(1)
end
local function useAll(): number
	local dog = Dog.new("rex", 2)
	local animal: Animal = dog
	local square: Figure = Square.new(3)
	local pair = Pair.new(1, 2)
	pair:swap()
	-- A generic method inherited by a derived class is called through the
	-- base type: Luau's new solver cannot instantiate `map`'s `U` when the
	-- receiver is a strict subtype of the `self` the alias declares.
	local box: Box<number> = pair
	local named = Named.new(4)
	local anonymous = Anonymous.new()
	return #animal:speak() + #dog:both() + #square:describe() + #Animal:create("a").name + box:get() + box:map(function(value: number): number
		return value + 1
	end) + NumberBox.new(2):double() + makeLate().n + named:twice() + #anonymous.label + Puppy.count + unitCircle().radius
end
return {
	makeLate = makeLate,
	unitCircle = unitCircle,
	useAll = useAll,
	Animal = Animal,
	Dog = Dog,
	Puppy = Puppy,
	Figure = Figure,
	Square = Square,
	Box = Box,
	Pair = Pair,
	NumberBox = NumberBox,
	Late = Late,
	Named = Named,
	Anonymous = Anonymous,
}
