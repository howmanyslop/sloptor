-- Compiled with sloptor v2.3.0
local a = 10
local b = 3
print(a + b, a - b, a * b, a / b, a % b, a ^ b)
local s = "num: "
local joined = s .. tostring(a)
local both = s .. "and" .. s
print(joined, both)
print(a < b, a <= b, a > b, a >= b, a == b, a ~= b)
print(bit32.band(a, b), bit32.bor(a, b), bit32.bxor(a, b), bit32.lshift(a, b), bit32.rshift(a, b))
return nil
