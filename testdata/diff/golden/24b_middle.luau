-- Compiled with sloptor v2.3.3
local TS = require(script.Parent.include.RuntimeLib)
local exports = {}
local _24a_shared = TS.import(script, script.Parent, "24a_shared")
local BASE = _24a_shared.BASE
local scale = _24a_shared.scale
exports.rescale = TS.import(script, script.Parent, "24a_shared").scale
local DOUBLE = BASE * 2
local function sum(p)
	return scale(p.first) + p.second
end
exports.sum = sum
exports.DOUBLE = DOUBLE
return exports
