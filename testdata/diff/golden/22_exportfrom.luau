-- Compiled with sloptor v2.3.3
local TS = require(script.Parent.include.RuntimeLib)
local exports = {}
for _k, _v in TS.import(script, script.Parent, "22a_base") or {} do
	exports[_k] = _v
end
exports.first = TS.import(script, script.Parent, "22a_base").alpha
local own = 1
exports.own = own
return exports
