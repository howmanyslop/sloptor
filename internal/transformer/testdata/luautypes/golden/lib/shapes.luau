-- Compiled with sloptor v3.0.0-beta.1
-- Cross-file fixture for imports.ts: one interface, one string-literal alias,
-- one generic interface, one class and one function, so type-only, renamed
-- and value imports all resolve.
export type Shape = { kind: ShapeKind, area: (self: Shape) -> number }
export type ShapeKind = "circle" | "square"
export type Box<T> = { item: T }
export type Circle = { kind: ShapeKind, radius: number, area: (self: Shape) -> number }
local Circle = setmetatable({}, {
	__tostring = function()
		return "Circle"
	end,
})
do
	Circle.__index = Circle
	function Circle.new(radius: number): Circle
		local self = (setmetatable({}, Circle) :: any) :: Circle
		return Circle.constructor(self, radius) or self
	end
	function Circle.constructor(self: Circle, radius: number)
		self.radius = radius
		self.kind = "circle"
	end
	function Circle.area(self: Circle): number
		return math.pi * self.radius * self.radius
	end
end
local function circle(radius: number): Shape
	return Circle.new(radius)
end
local function describe(shape: Shape): string
	return `{shape.kind}:{shape:area()}`
end
return {
	circle = circle,
	describe = describe,
	Circle = Circle,
}
