-- Compiled with sloptor v3.0.0-beta.1
local exports = {}
-- Tables: interfaces, aliases, index signatures, arrays, tuples, maps, sets,
-- classes, enums and `typeof`.
export type Point = { x: number, y: number }
export type Shape = {
	read kind: string,
	area: (self: Shape) -> number,
	optional: string?,
	["kebab-case"]: number,
	["1"]: boolean,
	["end"]: string,
	onEvent: (value: number) -> (),
	maybeRun: ((self: Shape) -> ())?,
	explicitThis: (self: Point, n: number) -> number,
	clone: (self: Shape) -> Shape,
}
export type Dict = { [string]: number }
export type NumDict = { [number]: string }
export type Both = { [any]: number }
export type BothDiffer = { [any]: any }
export type ReadonlyDict = { read [string]: number }
export type Node = { value: number, next: Node? }
export type Tree<T> = { value: T, children: { Tree<T> } }
export type Pair<T> = { first: T, second: T }
export type Mode = "r" | "w"
export type Callback<T> = (value: T) -> ()
export type Empty = {}
export type Direction = "up" | "down"
local Direction: { read Up: "up", read Down: "down" } = {
	Up = "up",
	Down = "down",
}
export type Codes = number
local Codes: { A: number, B: number, [number]: string }
do
	local _inverse = {}
	Codes = setmetatable({}, {
		__index = _inverse,
	}) :: any
	Codes.A = 1
	_inverse[1] = "A"
	Codes.B = 2
	_inverse[2] = "B"
end
export type Foo = { x: number, get: (self: Foo) -> number }
local Foo = setmetatable({}, {
	__tostring = function()
		return "Foo"
	end,
})
do
	Foo.__index = Foo
	function Foo.new(x: number): Foo
		local self = (setmetatable({}, Foo) :: any) :: Foo
		return Foo.constructor(self, x) or self
	end
	function Foo.constructor(self: Foo, x: number)
		self.x = x
	end
	function Foo.get(self: Foo): number
		return self.x
	end
	function Foo.make(self: typeof(Foo)): Foo
		return Foo.new(1)
	end
end
export type Box<T> = { value: T }
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
end
local config = {
	depth = 1,
}
exports.counter = 0
local later = {
	x = 1,
}
exports.Direction = Direction
exports.Codes = Codes
exports.Foo = Foo
exports.Box = Box
exports.config = config
exports.later = later
return exports
