-- Compiled with sloptor v2.4.0
local a = 5
local b = 3.25
local c = 0xff
local d = 0b101
local e = 1_000_000
local s1 = "double"
local s2 = "single"
local s3 = 'escape \"quotes\" and \\ backslash'
local t = true
local f = false
local arr = { 1, 2, 3 }
local empty = {}
local obj = {
	x = 1,
	y = "two",
	["computed key"] = 3,
}
local tmpl = `value is {a} and {s1}`
print(a, b, c, d, e, s1, s2, s3, t, f, arr, empty, obj, tmpl)
return nil
