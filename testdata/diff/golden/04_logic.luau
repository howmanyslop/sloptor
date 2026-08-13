-- Compiled with sloptor v2.3.0
local t = true
local f = false
local n = 5
local s = "hi"
local maybe = nil
print(t and f, t or f, not t)
print(n and s)
print(maybe or "default")
maybe = "set"
local _condition = maybe
if _condition == nil then
	_condition = "default"
end
print(_condition)
if n then
	print("n truthy")
end
if s then
	print("s truthy")
end
return nil
