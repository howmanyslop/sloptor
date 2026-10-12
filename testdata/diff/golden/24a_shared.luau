-- Compiled with sloptor v2.8.1
local BASE = 10
local function scale(n)
	return n * BASE
end
return {
	scale = scale,
	BASE = BASE,
}
