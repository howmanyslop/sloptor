--!strict
--!nolint
export type Foo = { x: number, constructor: (self: Foo, x: number) -> (), get: (self: Foo) -> number }
local Foo
do
	Foo = setmetatable({}, { __tostring = function() return "Foo" end })
	Foo.__index = Foo
	function Foo.new(x: number): Foo
		local self = (setmetatable({}, Foo) :: any) :: Foo
		return self:constructor(x) or self
	end
	function Foo.constructor(self: Foo, x: number)
		self.x = x
	end
	function Foo:get(): number
		return self.x
	end
	function Foo:oops(): number
		return self.nope
	end
	function Foo:callsGet(): number
		return self:get() + self:constructor(1)
	end
end
local f = Foo.new(1)
local n: number = f:get()
return nil
