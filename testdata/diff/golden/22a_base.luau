-- Compiled with sloptor v3.0.0-beta.1-beta.1
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
