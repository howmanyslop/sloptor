-- Compiled with sloptor v2.3.1
local function getArr()
	return { 3, 1, 2 }
end
local arr = { 1, 2, 3 }
-- ReadonlyArray
print(#arr == 0, #({}) == 0)
-- join: number elements (no tostring pass), default + custom separators
print(table.concat(arr, ", "), table.concat(arr, "-"))
local sep = ":"
-- ▼ ReadonlyArray.join ▼
local _sep = sep
if _sep == nil then
	_sep = ", "
end
-- ▲ ReadonlyArray.join ▲
print(table.concat(arr, _sep))
-- join with the tostring pre-pass (boolean elements)
local flags = { true, false }
-- ▼ ReadonlyArray.join ▼
local _result = table.create(#flags)
for _k, _v in flags do
	_result[_k] = tostring(_v)
end
-- ▲ ReadonlyArray.join ▲
print(table.concat(_result, ", "))
local _exp = getArr()
-- ▼ ReadonlyArray.map ▼
local _newValue = table.create(#_exp)
local _callback = function(v)
	return v > 1
end
for _k, _v in _exp do
	_newValue[_k] = _callback(_v, _k - 1, _exp)
end
-- ▲ ReadonlyArray.map ▲
-- ▼ ReadonlyArray.join ▼
local _result_1 = table.create(#_newValue)
for _k, _v in _newValue do
	_result_1[_k] = tostring(_v)
end
-- ▲ ReadonlyArray.join ▲
print(table.concat(_result_1, "|"))
-- move / includes / indexOf
local dest = { 0, 0, 0, 0 }
table.move(arr, 1, 3, 2, dest)
print(dest)
print(table.find(arr, 2) ~= nil, table.find(arr, 2, 2) ~= nil)
print((table.find(arr, 3) or 0) - 1, (table.find(arr, 3, 2) or 0) - 1, (table.find(getArr(), 9) or 0) - 1)
-- every / some
-- ▼ ReadonlyArray.every ▼
local _result_2 = true
local _callback_1 = function(v, i)
	return v > i
end
for _k, _v in arr do
	if not _callback_1(_v, _k - 1, arr) then
		_result_2 = false
		break
	end
end
-- ▲ ReadonlyArray.every ▲
print(_result_2)
-- ▼ ReadonlyArray.some ▼
local _result_3 = false
local _callback_2 = function(v, i, a)
	return v == #a - i
end
for _k, _v in arr do
	if _callback_2(_v, _k - 1, arr) then
		_result_3 = true
		break
	end
end
-- ▲ ReadonlyArray.some ▲
print(_result_3)
-- forEach: statement vs value position
-- ▼ ReadonlyArray.forEach ▼
local _callback_3 = function(v)
	return print(v)
end
for _k, _v in arr do
	_callback_3(_v, _k - 1, arr)
end
-- ▲ ReadonlyArray.forEach ▲
-- ▼ ReadonlyArray.forEach ▼
local _callback_4 = function(v, i)
	return print(v, i)
end
for _k, _v in arr do
	_callback_4(_v, _k - 1, arr)
end
-- ▲ ReadonlyArray.forEach ▲
local fe = nil
print(fe)
-- map / mapFiltered / filterUndefined / filter
-- ▼ ReadonlyArray.map ▼
local _newValue_1 = table.create(#arr)
local _callback_5 = function(v, i)
	return v * 2 + i
end
for _k, _v in arr do
	_newValue_1[_k] = _callback_5(_v, _k - 1, arr)
end
-- ▲ ReadonlyArray.map ▲
local doubled = _newValue_1
print(doubled)
-- ▼ ReadonlyArray.mapFiltered ▼
local _newValue_2 = {}
local _callback_6 = function(v)
	return if v > 1 then v * 10 else nil
end
local _length = 0
for _k, _v in arr do
	local _result_4 = _callback_6(_v, _k - 1, arr)
	if _result_4 ~= nil then
		_length += 1
		_newValue_2[_length] = _result_4
	end
end
-- ▲ ReadonlyArray.mapFiltered ▲
local mf = _newValue_2
print(mf)
local holes = { 1, nil, 3 }
-- ▼ ReadonlyArray.filterUndefined ▼
local _length_1 = 0
for _i in holes do
	if _i > _length_1 then
		_length_1 = _i
	end
end
local _result_4 = {}
local _resultLength = 0
for _i = 1, _length_1 do
	local _v = holes[_i]
	if _v ~= nil then
		_resultLength += 1
		_result_4[_resultLength] = _v
	end
end
-- ▲ ReadonlyArray.filterUndefined ▲
print(_result_4)
-- ▼ ReadonlyArray.filter ▼
local _newValue_3 = {}
local _callback_7 = function(v)
	return v % 2 == 1
end
local _length_2 = 0
for _k, _v in arr do
	if _callback_7(_v, _k - 1, arr) == true then
		_length_2 += 1
		_newValue_3[_length_2] = _v
	end
end
-- ▲ ReadonlyArray.filter ▲
print(_newValue_3)
-- reduce: no initialValue vs initialValue
-- ▼ ReadonlyArray.reduce ▼
if #arr == 0 then
	error("Attempted to call `ReadonlyArray.reduce()` on an empty array without an initialValue.")
end
local _result_5 = arr[1]
local _callback_8 = function(a, b)
	return a + b
end
for _i = 2, #arr do
	_result_5 = _callback_8(_result_5, arr[_i], _i - 1, arr)
end
-- ▲ ReadonlyArray.reduce ▲
print(_result_5)
-- ▼ ReadonlyArray.reduce ▼
local _result_6 = 10
local _callback_9 = function(acc, v, i)
	return acc + v * i
end
for _i = 1, #arr do
	_result_6 = _callback_9(_result_6, arr[_i], _i - 1, arr)
end
-- ▲ ReadonlyArray.reduce ▲
print(_result_6)
-- find / findIndex
-- ▼ ReadonlyArray.find ▼
local _callback_10 = function(v)
	return v == 2
end
local _result_7
for _i, _v in arr do
	if _callback_10(_v, _i - 1, arr) == true then
		_result_7 = _v
		break
	end
end
-- ▲ ReadonlyArray.find ▲
print(_result_7)
-- ▼ ReadonlyArray.findIndex ▼
local _callback_11 = function(v)
	return v == 2
end
local _result_8 = -1
for _i, _v in arr do
	if _callback_11(_v, _i - 1, arr) == true then
		_result_8 = _i - 1
		break
	end
end
-- ▲ ReadonlyArray.findIndex ▲
print(_result_8)
-- complex base: header-exempt `local _exp = ...` push
local _exp_1 = getArr()
-- ▼ ReadonlyArray.map ▼
local _newValue_4 = table.create(#_exp_1)
local _callback_12 = function(v)
	return v + 1
end
for _k, _v in _exp_1 do
	_newValue_4[_k] = _callback_12(_v, _k - 1, _exp_1)
end
-- ▲ ReadonlyArray.map ▲
print(_newValue_4)
-- Array
local list = { 5, 6 }
table.insert(list, 7)
-- ▼ Array.push ▼
table.insert(list, 8)
table.insert(list, 9)
-- ▲ Array.push ▲
table.insert(list, 10)
local newLen = #list
print(newLen, #list)
local _ = #list
list[#list] = nil
-- ▼ Array.pop ▼
local _length_3 = #list
local _result_9 = list[_length_3]
list[_length_3] = nil
-- ▲ Array.pop ▲
local popped = _result_9
print(popped)
table.remove(list, 1)
local shifted = table.remove(list, 1)
print(shifted, list)
table.insert(list, 1, 4)
-- ▼ Array.unshift ▼
table.insert(list, 1, 3)
table.insert(list, 1, 2)
-- ▲ Array.unshift ▲
local ulen = #list
print(ulen)
table.insert(list, 2, 99)
table.remove(list, 1)
local removed = table.remove(list, 2)
print(removed)
-- ▼ Array.unorderedRemove ▼
local _length_4 = #list
local _value = list[1]
if _value ~= nil then
	list[1] = list[_length_4]
	list[_length_4] = nil
end
-- ▲ Array.unorderedRemove ▲
-- ▼ Array.unorderedRemove ▼
local _length_5 = #list
local _value_1 = list[2]
if _value_1 ~= nil then
	list[2] = list[_length_5]
	list[_length_5] = nil
end
-- ▲ Array.unorderedRemove ▲
local ur = _value_1
print(ur)
table.sort(list)
table.sort(list, function(a, b)
	return a > b
end)
table.sort(list, function(a, b)
	return a < b
end)
local sorted = list
print(sorted)
table.clear(list)
print(#list == 0)
return nil
