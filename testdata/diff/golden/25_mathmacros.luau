-- Compiled with sloptor v2.4.0
local a = Vector3.new(1, 2, 3)
local b = Vector3.new(4, 5, 6)
local c = Vector3.new(7, 8, 9)
print(a + b, a - b, a * b, a / b, a // b)
print(a * 2, a / 0.5, a // 2)
print((a + b) * c)
local _arg0 = b + c
print(a * _arg0)
local _exp = a + b
local _arg0_1 = c - a
print(_exp * _arg0_1)
local _exp_1 = a - b - c
local _arg0_2 = b - c
print(_exp_1, a - _arg0_2)
local cf = CFrame.new(a)
local cf2 = CFrame.new(b)
print(cf + b, cf - b, cf * cf2, cf * b)
print(cf * cf2 * cf2)
local u = UDim.new(0.5, 10)
local u2 = UDim2.new(u, u)
print(u + u, u - u, u2 + u2, u2 - u2)
local v2 = Vector2.new(1, 2)
print(v2 + v2, v2 - v2, v2 * 3, v2 / v2, v2 // 2)
local v2i = Vector2int16.new(1, 2)
print(v2i + v2i, v2i - v2i, v2i * v2i, v2i / v2i)
local v3i = Vector3int16.new(1, 2, 3)
print(v3i + v3i, v3i - v3i, v3i * v3i, v3i / v3i)
local n = 7
print(n // 2)
local function getOffset()
	return Vector3.new(1, 1, 1)
end
local _arg0_3 = getOffset()
print(a + _arg0_3)
local _x = b.X
print(a * _x)
return nil
