--!strict
--!nolint
export type Base = { x: number, get: (self: Base) -> number }
export type Derived = { x: number, y: string, get: (self: Base) -> number, both: (self: Derived) -> string }
local Base = setmetatable({}, { __tostring = function() return "Base" end })
do
	Base.__index = Base
	function Base.new(x: number): Base
		local self = (setmetatable({}, Base) :: any) :: Base
		return Base.constructor(self, x) or self
	end
	function Base.constructor(self: Base, x: number) self.x = x end
	function Base.get(self: Base): number return self.x end
end
local Derived = setmetatable({}, { __tostring = function() return "Derived" end, __index = Base })
do
	local super = Base
	Derived.__index = Derived
	function Derived.new(x: number, y: string): Derived
		local self = (setmetatable({}, Derived) :: any) :: Derived
		return Derived.constructor(self, x, y) or self
	end
	function Derived.constructor(self: Derived, x: number, y: string)
		super.constructor(self, x)
		self.y = y
	end
	function Derived.get(self: Derived): number return super.get(self) + #self.y end
	function Derived.both(self: Derived): string return self.y .. tostring(self:get()) end
end
local d = Derived.new(1, "a")
local b: Base = d
local n: number = d:get() + b:get()
local s: string = d:both()
local bad: Derived = Base.new(1)
local bad2: string = d:get()
local bad3 = b:both()
return nil
