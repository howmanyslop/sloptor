-- Compiled with sloptor v2.3.1
local arr = { 10, 20, 30 }
local first = arr[1]
local second = arr[2]
local idx = 2
local dynamic = arr[idx + 1]
local obj = {
	alpha = 1,
	["beta key"] = 2,
}
local byProp = obj.alpha
local byString = obj["beta key"]
local nested = {
	inner = {
		deep = "value",
	},
}
print(first, second, dynamic, byProp, byString, nested.inner.deep)
print(math.floor(3.7), math.max(1, 2, 3), tostring(42), (tonumber("7")))
return nil
