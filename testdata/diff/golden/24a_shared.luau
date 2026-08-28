-- Compiled with sloptor v2.3.3
local BASE = 10
local function scale(n)
	return n * BASE
end
return {
	scale = scale,
	BASE = BASE,
}
