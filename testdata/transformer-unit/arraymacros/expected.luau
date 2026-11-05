local function isBig(v)
	return v > 15
end
local arr = { 10, 20, 30 }
print(#arr == 0)
print(table.concat(arr, ", "))
table.move(arr, 1, 2, 4)
print(table.find(arr, 20) ~= nil)
print((table.find(arr, 20) or 0) - 1)
-- ▼ ReadonlyArray.every ▼
local _result = true
local _callback = function(v)
	return v > 0
end
for _k, _v in arr do
	if not _callback(_v, _k - 1, arr) then
		_result = false
		break
	end
end
-- ▲ ReadonlyArray.every ▲
print(_result)
-- ▼ ReadonlyArray.some ▼
local _result_1 = false
local _callback_1 = function(v)
	return v > 25
end
for _k, _v in arr do
	if _callback_1(_v, _k - 1, arr) then
		_result_1 = true
		break
	end
end
-- ▲ ReadonlyArray.some ▲
print(_result_1)
-- ▼ ReadonlyArray.forEach ▼
local _callback_2 = function(v, i)
	return print(v, i)
end
for _k, _v in arr do
	_callback_2(_v, _k - 1, arr)
end
-- ▲ ReadonlyArray.forEach ▲
-- ▼ ReadonlyArray.map ▼
local _newValue = table.create(#arr)
local _callback_3 = function(v, i)
	return v + i
end
for _k, _v in arr do
	_newValue[_k] = _callback_3(_v, _k - 1, arr)
end
-- ▲ ReadonlyArray.map ▲
print(_newValue)
-- ▼ ReadonlyArray.mapFiltered ▼
local _newValue_1 = {}
local _callback_4 = function(v)
	return if v > 10 then v else nil
end
local _length = 0
for _k, _v in arr do
	local _result_2 = _callback_4(_v, _k - 1, arr)
	if _result_2 ~= nil then
		_length += 1
		_newValue_1[_length] = _result_2
	end
end
-- ▲ ReadonlyArray.mapFiltered ▲
print(_newValue_1)
local sparse = { 10, nil }
-- ▼ ReadonlyArray.filterUndefined ▼
local _length_1 = 0
for _i in sparse do
	if _i > _length_1 then
		_length_1 = _i
	end
end
local _result_2 = {}
local _resultLength = 0
for _i = 1, _length_1 do
	local _v = sparse[_i]
	if _v ~= nil then
		_resultLength += 1
		_result_2[_resultLength] = _v
	end
end
-- ▲ ReadonlyArray.filterUndefined ▲
print(_result_2)
-- ▼ ReadonlyArray.filter ▼
local _newValue_2 = {}
local _callback_5 = function(v)
	return v > 10
end
local _length_2 = 0
for _k, _v in arr do
	if _callback_5(_v, _k - 1, arr) == true then
		_length_2 += 1
		_newValue_2[_length_2] = _v
	end
end
-- ▲ ReadonlyArray.filter ▲
print(_newValue_2)
-- ▼ ReadonlyArray.filter ▼
local _newValue_3 = {}
local _length_3 = 0
for _k, _v in arr do
	if isBig(_v, _k - 1, arr) == true then
		_length_3 += 1
		_newValue_3[_length_3] = _v
	end
end
-- ▲ ReadonlyArray.filter ▲
print(_newValue_3)
-- ▼ ReadonlyArray.reduce ▼
if #arr == 0 then
	error("Attempted to call `ReadonlyArray.reduce()` on an empty array without an initialValue.")
end
local _result_3 = arr[1]
local _callback_6 = function(a, b)
	return a + b
end
for _i = 2, #arr do
	_result_3 = _callback_6(_result_3, arr[_i], _i - 1, arr)
end
-- ▲ ReadonlyArray.reduce ▲
print(_result_3)
-- ▼ ReadonlyArray.reduce ▼
local _result_4 = 0
local _callback_7 = function(a, b)
	return a + b
end
for _i = 1, #arr do
	_result_4 = _callback_7(_result_4, arr[_i], _i - 1, arr)
end
-- ▲ ReadonlyArray.reduce ▲
print(_result_4)
-- ▼ ReadonlyArray.find ▼
local _callback_8 = function(v)
	return v == 20
end
local _result_5
for _i, _v in arr do
	if _callback_8(_v, _i - 1, arr) == true then
		_result_5 = _v
		break
	end
end
-- ▲ ReadonlyArray.find ▲
print(_result_5)
-- ▼ ReadonlyArray.findIndex ▼
local _callback_9 = function(v)
	return v == 20
end
local _result_6 = -1
for _i, _v in arr do
	if _callback_9(_v, _i - 1, arr) == true then
		_result_6 = _i - 1
		break
	end
end
-- ▲ ReadonlyArray.findIndex ▲
print(_result_6)
local list = { 1, 2 }
table.insert(list, 3)
-- ▼ Array.push ▼
table.insert(list, 4)
table.insert(list, 5)
-- ▲ Array.push ▲
local len = #list
print(len)
list[#list] = nil
-- ▼ Array.pop ▼
local _length_4 = #list
local _result_7 = list[_length_4]
list[_length_4] = nil
-- ▲ Array.pop ▲
print(_result_7)
print((table.remove(list, 1)))
table.insert(list, 1, 0)
table.insert(list, 1, 9)
table.remove(list, 1)
-- ▼ Array.unorderedRemove ▼
local _length_5 = #list
local _value = list[1]
if _value ~= nil then
	list[1] = list[_length_5]
	list[_length_5] = nil
end
-- ▲ Array.unorderedRemove ▲
table.sort(list)
table.clear(list)
local i = 2
local _arg0 = i - 1
table.insert(list, _arg0 + 1, 7)
print(i)
