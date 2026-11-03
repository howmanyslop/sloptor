--!strict
--!nolint
export type Box<T> = { value: T, get: (self: Box<T>) -> T }
export type Pair<T> = Box<T> & { other: T, swap: (self: Pair<T>) -> () }
local Box = setmetatable({}, { __tostring = function() return "Box" end })
do
	Box.__index = Box
	function Box.new<T>(value: T): Box<T>
		local self = (setmetatable({}, Box) :: any) :: Box<T>
		return Box.constructor(self, value) or self
	end
	function Box.constructor<T>(self: Box<T>, value: T) self.value = value end
	function Box.get<T>(self: Box<T>): T return self.value end
end
local Pair = setmetatable({}, { __tostring = function() return "Pair" end, __index = Box })
do
	local super = Box
	Pair.__index = Pair
	function Pair.new<T>(value: T, other: T): Pair<T>
		local self = (setmetatable({}, Pair) :: any) :: Pair<T>
		return Pair.constructor(self, value, other) or self
	end
	function Pair.constructor<T>(self: Pair<T>, value: T, other: T)
		super.constructor(self, value)
		self.other = other
	end
	function Pair.swap<T>(self: Pair<T>)
		self.value, self.other = self.other, self.value
	end
end
local p = Pair.new(1, 2)
local b: Box<number> = p
local n: number = p:get()
p:swap()
local bad: string = p:get()
local bad2: Box<string> = p
return nil
