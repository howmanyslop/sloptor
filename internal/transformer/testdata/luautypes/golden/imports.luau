-- Compiled with sloptor v3.0.0-beta.1
local TS = _G[script]
type Shape = { kind: ShapeKind, area: (self: Shape) -> number }
type ShapeKind = "circle" | "square"
type Container<T> = { item: T }
-- luauTypes: Promise<TResult1>, Promise<TResult2>, Promise<TResult1 | TResult2>, Promise<TResult>, Promise<T | TResult>, Promise<R>, Promise<U> spelled as any: a recursive alias must be referenced with its own type arguments
type Promise<T> = {
	["then"]: <TResult1, TResult2>(self: Promise<T>, onResolved: ((value: T) -> any)?, onRejected: ((reason: any) -> any)?) -> any,
	andThen: <TResult1, TResult2>(self: Promise<T>, onResolved: ((value: T) -> any)?, onRejected: ((reason: any) -> any)?) -> any,
	catch: <TResult>(self: Promise<T>, onRejected: ((reason: any) -> any)?) -> any,
	tap: (self: Promise<T>, tapHandler: (value: T) -> ()) -> Promise<T>,
	finally: <TResult>(self: Promise<T>, onSettled: (() -> any)?) -> any,
	andThenCall: <P, R>(self: Promise<T>, callback: (...any) -> R, ...any) -> any,
	finallyCall: <P, R>(self: Promise<T>, callback: (...any) -> R, ...any) -> any,
	andThenReturn: <U>(self: Promise<T>, value: U) -> any,
	finallyReturn: <U>(self: Promise<T>, value: U) -> any,
	timeout: (self: Promise<T>, seconds: number, rejectionValue: any) -> Promise<T>,
	cancel: (self: Promise<T>) -> (),
	now: (self: Promise<T>, rejectionValue: any) -> Promise<T>,
	await: (self: Promise<T>) -> ...any,
	awaitStatus: (self: Promise<T>) -> (Promise_Status, unknown),
	expect: (self: Promise<T>) -> T,
	getStatus: (self: Promise<T>) -> Promise_Status,
}
type Promise_Status = "Cancelled" | "Rejected" | "Resolved" | "Started"
-- Cross-file types: a type-only import, a renamed import of a generic
-- interface, value imports from the same sibling module, and a lib type
-- (`Promise<number>`). Every imported type is mirrored as a hoisted,
-- non-exported alias under its local name, because `TS.import` is opaque to
-- Luau's checker.
local _shapes = TS.import(script, script.Parent, "lib", "shapes")
local circle = _shapes.circle
local describe = _shapes.describe
export type Wrapped = {
	shape: Shape,
	kind: ShapeKind,
	container: Container<Shape>,
	pending: Promise<number>,
}
local function unit(): Shape
	return circle(1)
end
local function kindOf(shape: Shape): ShapeKind
	return if shape.kind == "circle" then "circle" else "square"
end
local function wrap(shape: Shape): Container<Shape>
	return {
		item = shape,
	}
end
local function label(): string
	return describe(unit())
end
return {
	unit = unit,
	kindOf = kindOf,
	wrap = wrap,
	label = label,
}
