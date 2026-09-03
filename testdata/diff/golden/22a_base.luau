-- Compiled with sloptor v2.4.0
local alpha = 1
local beta = 2
local function gamma(n)
	return n + alpha
end
return {
	gamma = gamma,
	alpha = alpha,
	beta = beta,
}
