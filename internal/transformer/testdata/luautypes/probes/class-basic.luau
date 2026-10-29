--!strict
--!nolint
export type Foo = { x: number, get: (self: Foo) -> number }
local Foo = setmetatable({}, { __tostring = function() return "Foo" end })
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
end
local f = Foo.new(1)
local n: number = f:get()
local bad1: string = f:get()
local bad2 = f.y
local bad3 = Foo.new("s")
return nil
