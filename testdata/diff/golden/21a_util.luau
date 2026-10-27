-- Compiled with sloptor v3.0.0-beta.1-beta.1
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
