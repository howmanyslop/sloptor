--!strict
--!nolint
export type Foo = { x: number, constructor: (self: Foo, x: number) -> (), get: (self: Foo) -> number }
local Foo = setmetatable({}, { __tostring = function() return "Foo" end })
do
	Foo.__index = Foo
	function Foo.new(x: number): Foo
		local self = (setmetatable({}, Foo) :: any) :: Foo
		return self:constructor(x) or self
	end
	function Foo.constructor(self: Foo, x: number)
		self.x = x
	end
	function Foo.get(self: Foo): number
		return self.x
	end
	function Foo.make(self: typeof(Foo), x: number): Foo
		return Foo.new(x)
	end
	function Foo.twice(x: number): Foo
		return Foo.new(x * 2)
	end
end
local f = Foo.new(1)
local m = Foo.make(Foo, 2)
local t = Foo.twice(3)
local n: number = m:get() + t:get()
local bad1: string = f:get()
local bad2 = f.y
local bad3 = Foo.new("s")
local bad4 = Foo.make(f, 2)
return nil
