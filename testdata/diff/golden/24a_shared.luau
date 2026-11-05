-- Compiled with sloptor v3.0.0-beta.3
local BASE = 10
local function scale(n)
	return n * BASE
end
return {
	scale = scale,
	BASE = BASE,
}
