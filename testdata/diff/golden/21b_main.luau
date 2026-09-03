-- Compiled with sloptor v2.3.3
local TS = require(script.Parent.include.RuntimeLib)
local _21a_util = TS.import(script, script.Parent, "21a_util")
local describe = _21a_util.default
local MAX = _21a_util.MAX
local clamp = _21a_util.clamp
local util = TS.import(script, script.Parent, "21a_util")
print(MAX, clamp(150))
print(describe(7))
print(util.MAX, util.clamp(5))
print(util.default(2))
return nil
