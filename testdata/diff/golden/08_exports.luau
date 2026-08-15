-- Compiled with sloptor v2.3.1
local exports = {}
local constant = 42
exports.mutable = "start"
exports.mutable = "changed"
local internal = constant + 1
print(internal, exports.mutable)
exports.constant = constant
return exports
