--!strict
--!nolint
export type Box<T> = { value: T, constructor: (self: Box<T>, value: T) -> (), get: (self: Box<T>) -> T }
local Box
do
	Box = setmetatable({}, { __tostring = function() return "Box" end })
	Box.__index = Box
	function Box.new<T>(value: T): Box<T>
		local self = (setmetatable({}, Box) :: any) :: Box<T>
		return self:constructor(value) or self
	end
	function Box.constructor<T>(self: Box<T>, value: T)
		self.value = value
	end
	function Box.get<T>(self: Box<T>): T
		return self.value
	end
end
local n: number = Box.new(1):get()
local bad: string = Box.new(1):get()
return nil
