--!strict
--!nolint
export type Direction = "up" | "down"
local Direction: { read Up: "up", read Down: "down" } = { Up = "up", Down = "down" }
local d: Direction = Direction.Up
local bad: Direction = "sideways"
return nil
