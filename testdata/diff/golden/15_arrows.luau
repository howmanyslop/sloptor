-- Compiled with sloptor v2.3.2
local square = function(x)
	return x * x
end
local clamp = function(x, lo, hi)
	if x < lo then
		return lo
	end
	return if x > hi then hi else x
end
local counter = 0
local bump = function()
	counter += 1
	return counter
end
local ops = {
	twice = function(x)
		return x * 2
	end,
	apply = function(f, v)
		return f(v)
	end,
}
print(square(5), clamp(15, 0, 10), bump(), bump(), ops.twice(4), ops.apply(square, 6))
return nil
