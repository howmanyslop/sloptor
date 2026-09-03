-- Compiled with sloptor v2.4.0
local MAX = 100
local function clamp(x)
	return if x > MAX then MAX else x
end
local function describe(x)
	return "v" .. tostring(x)
end
return {
	clamp = clamp,
	default = describe,
	MAX = MAX,
}
