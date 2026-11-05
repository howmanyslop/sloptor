-- Compiled with sloptor v3.0.0-beta.1
-- Placeholder for WP6: Roblox data types and an enum item as parameters and
-- returns. Every global used here is declared in ../sloptor.d.luau.
local function offset(position: Vector3, delta: Vector3): Vector3
	local _position = position
	local _delta = delta
	return _position + _delta
end
local function anchor(part: Part, material: Enum.Material): Part
	part.Anchored = true
	part.Material = material
	return part
end
local function plastic(part: Part): Part
	return anchor(part, Enum.Material.Plastic)
end
local function magnitude(part: Part): number
	return part.Position.Magnitude
end
return {
	offset = offset,
	anchor = anchor,
	plastic = plastic,
	magnitude = magnitude,
}
