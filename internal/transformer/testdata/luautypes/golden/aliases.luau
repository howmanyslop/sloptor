-- Compiled with sloptor v3.0.0-beta.1
export type Geometry_Size = { width: number, height: number }
-- Aliases: exported and local interfaces, generic aliases with defaults,
-- recursion with identical and with different arguments, a merged interface,
-- aliases of a union, a literal and a callable, a nested namespace, a name
-- that collides with a Roblox type, an `export { X }` specifier and a
-- LuaTuple alias.
export type Point = { x: number, y: number }
type Hidden = { secret: number }
export type Listed = { id: string }
export type Boxed<T = number> = { value: T }
export type Pair<A, B = A> = { first: A, second: B }
-- `until` is a Luau keyword, so the parameter is left out of the alias and
-- every reference to it (T's default included) widens to any.
export type Labeled<T = any> = { label: any, value: T }
export type Tree = { value: number, children: { Tree } }
export type Chain<T> = { value: T, next: Chain<T>? }
-- luauTypes: Weird<T[]> spelled as any: a recursive alias must be referenced with its own type arguments
export type Weird<T> = { value: T, nested: any }
export type Merged = { a: number, b: string }
export type Mode = "r" | "w"
export type Literal = "x"
export type Handler = (value: number) -> ()
export type Part_2 = { id: number }
export type Holder = {
	point: Point,
	hidden: Hidden,
	listed: Listed,
	boxed: Boxed<number>,
	pair: Pair<number, number>,
	labeled: Labeled<number>,
	tree: Tree,
	chain: Chain<number>,
	weird: Weird<number>,
	merged: Merged,
	mode: Mode,
	literal: "x",
	handler: Handler,
	part: Part_2,
	size: Geometry_Size,
}
local function origin(): Point
	return {
		x = 0,
		y = 0,
	}
end
local function box<T>(value: T): Boxed<T>
	return {
		value = value,
	}
end
local function leaf(value: number): Tree
	return {
		value = value,
		children = {},
	}
end
local function merged(): Merged
	return {
		a = 1,
		b = "b",
	}
end
local function size(): Geometry_Size
	return {
		width = 1,
		height = 2,
	}
end
local function tuple(): (number, string)
	return 1, "a"
end
return {
	origin = origin,
	box = box,
	leaf = leaf,
	merged = merged,
	size = size,
	tuple = tuple,
}
