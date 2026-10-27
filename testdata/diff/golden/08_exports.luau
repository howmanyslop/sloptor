-- Compiled with sloptor v3.0.0-beta.1-beta.1
local exports = {}
local constant = 42
exports.mutable = "start"
exports.mutable = "changed"
local internal = constant + 1
print(internal, exports.mutable)
exports.constant = constant
return exports
