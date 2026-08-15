-- Compiled with sloptor v2.3.1
local counter = 0
local arr = { 1, 2, 3 }
local obj = {
	a = 1,
	b = 2,
}
local _original = counter
counter += 1
local _condition = _original > 0
if _condition then
	_condition = arr[counter + 1] == 2
end
local _condition_1 = _condition
if not _condition_1 then
	_condition_1 = obj.a == 1
end
local r1 = _condition_1
local _exp = arr
local _original_1 = counter
counter += 1
local _exp_1 = _exp[_original_1 + 1]
local _exp_2 = arr
local _original_2 = counter
counter += 1
local r2 = _exp_1 + _exp_2[_original_2 + 1]
local s = "x"
local _exp_3 = s
s ..= "y"
local r3 = `{_exp_3} {s} {s}`
local cond = if counter > 2 then arr[1] else obj.b
counter += 1
local _condition_2 = counter > 3
if _condition_2 then
	counter -= 1
	_condition_2 = counter < 10
end
if _condition_2 then
	print("both sides ran")
end
print(r1, r2, r3, cond, counter)
return nil
