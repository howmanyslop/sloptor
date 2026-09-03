-- Compiled with sloptor v2.3.3
local s = "Hello, World"
print(#s, #"abc")
print(string.upper(s), string.lower(s), string.reverse(s))
print(string.rep(s, 2), string.sub(s, 1, 5), string.sub(s, 8))
print(string.sub("literal", 1, 3), string.lower("AbC"), string.rep("hi", 3))
print(string.split(s, ", "))
local i1, j1 = string.find(s, "World")
print(i1, j1)
local found = { string.find(s, "o", 2, true) }
print(found)
local replaced, count = string.gsub(s, "o", "0")
print(replaced, count)
local once = { string.gsub(s, "l", "L", 1) }
print(once)
print(string.format("%d/%s", 5, "x"))
local m = { string.match(s, "(%a+)") }
print(m)
local iter = string.gmatch(s, "%a+")
print(iter)
local b = { string.byte(s) }
print(b)
local b1, b2 = string.byte(s, 1, 2)
print(b1, b2)
print(string.upper(("a" .. s)), #("z" .. s))
local mut = "abcdef"
local i = 1
local _mut = mut
local _original = i
i += 1
print(string.sub(_mut, _original, 4))
print(string.upper(s))
local arr = { 1, 2, 3 }
print(#arr)
local total = #arr + #s
print(total)
for k = 0, #arr - 1 do
	print(arr[k + 1])
end
return nil
