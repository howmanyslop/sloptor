-- Compiled with sloptor v2.3.3
local TS = require(script.Parent.include.RuntimeLib)
local _24a_shared = TS.import(script, script.Parent, "24a_shared")
local BASE = _24a_shared.BASE
local scale = _24a_shared.scale
local _24b_middle = TS.import(script, script.Parent, "24b_middle")
local DOUBLE = _24b_middle.DOUBLE
local sum = _24b_middle.sum
local rescale = _24b_middle.rescale
local p = {
	first = 3,
	second = 4,
}
print(BASE, scale(2), DOUBLE)
print(sum(p), rescale(p.second))
local fns = {}
for i = 0, 2 do
	fns[i + 1] = function()
		local m = {}
		print("make", i)
		return m
	end
end
print(fns[1](), fns[3]())
return nil
