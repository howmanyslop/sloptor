-- Compiled with sloptor v3.0.0-beta.1
type Ambient = number
-- Enums (plan WP7): a string enum, a numeric enum, a mixed enum, a const enum
-- (alias only), an ambient enum (mirrored when referenced), and a string enum
-- referenced before its declaration (hoisted).
export type Direction = "up" | "down"
local Direction: { read Up: "up", read Down: "down" } = {
	Up = "up",
	Down = "down",
}
export type Codes = number
local Codes: { A: number, B: number, C: number, [number]: string }
do
	local _inverse = {}
	Codes = setmetatable({}, {
		__index = _inverse,
	}) :: any
	Codes.A = 0
	_inverse[0] = "A"
	Codes.B = 1
	_inverse[1] = "B"
	Codes.C = 10
	_inverse[10] = "C"
end
export type Mixed = number | "two"
local Mixed: { One: number, Two: "two", [number]: string }
do
	local _inverse = {}
	Mixed = setmetatable({}, {
		__index = _inverse,
	}) :: any
	Mixed.One = 1
	_inverse[1] = "One"
	Mixed.Two = "two"
end
export type Flags = number
local function flip(direction: Direction): Direction
	return if direction == Direction.Up then Direction.Down else Direction.Up
end
local function code(): Codes
	return Codes.B
end
local function codeName(): string
	return Codes[Codes.C]
end
local function mixed(): Mixed
	return Mixed.Two
end
local function flags(): number
	return 1 + 2
end
local function ambient(value: Ambient): number
	return value
end
local Hoisted: { read Yes: "yes", read No: "no" }
local function early(): Hoisted
	return Hoisted.Yes
end
export type Hoisted = "yes" | "no"
Hoisted = {
	Yes = "yes",
	No = "no",
}
return {
	flip = flip,
	code = code,
	codeName = codeName,
	mixed = mixed,
	flags = flags,
	ambient = ambient,
	early = early,
	Direction = Direction,
	Codes = Codes,
	Mixed = Mixed,
	Hoisted = Hoisted,
}
