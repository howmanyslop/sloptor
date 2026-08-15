-- Compiled with sloptor v2.3.1
-- declared before the exported function: rbxtsc still returns the function
-- first (binder binds FunctionDeclarations before other statements)
local VERSION = 3
local function makeAdder(base)
	return function(x)
		return base + x
	end
end
local function makeCounter(start)
	local count = start
	return function()
		count += 1
		return count
	end
end
local function classify(items)
	local evens = 0
	local odds = 0
	for _, item in items do
		local _exp = item % 2
		repeat
			if _exp == 0 then
				evens += 1
				break
			end
			odds += 1
		until true
	end
	return {
		evens = evens,
		odds = odds,
	}
end
local function bindAll(...)
	local nums = { ... }
	return function(extra)
		local total = extra
		for _, n in nums do
			total += n
		end
		return total
	end
end
local function withDefaults(a, b, f)
	if b == nil then
		b = a + 1
	end
	if f == nil then
		f = function(n)
			return n * a
		end
	end
	return f(b)
end
local function pickLabel(kind)
	local _binding = {
		label = if kind == "x" then "extra" else "normal",
	}
	local label = _binding.label
	return function()
		return label
	end
end
local _binding = classify({ 1, 2, 3, 4, 5 })
local evens = _binding.evens
local odds = _binding.odds
local add5 = makeAdder(5)
local tick = makeCounter(10)
local pairList = { { 1, 10 }, { 2, 20 }, { 3, 30 } }
local fns = {}
local slot = 0
for _, _binding_1 in pairList do
	local k = _binding_1[1]
	local v = _binding_1[2]
	fns[slot + 1] = function()
		return k * 100 + v
	end
	slot += 1
end
local sum3 = bindAll(1, 2, 3)
print(add5(10), evens, odds, withDefaults(2), withDefaults(3, 7), sum3(4))
print(fns[1](), fns[2](), fns[3](), tick(), tick(), pickLabel("x")(), pickLabel("y")())
return {
	classify = classify,
	VERSION = VERSION,
}
