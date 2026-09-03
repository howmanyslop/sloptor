-- Compiled with sloptor v2.4.0
return function()
	it("should support string methods", function()
		expect(string.sub("Hello, world", 1, 1)).to.equal("H")
	end)
	it("should support string methods on identifiers", function()
		local str = "Hello, world"
		expect(string.sub(str, 1, 1)).to.equal("H")
	end)
	it("should support string.split", function()
		local function checkLen(len, arr)
			expect(#arr).to.equal(len)
			return arr
		end
		local str = "Hello, world"
		local _exp = { string.byte(str, 0, -1) }
		-- ▼ ReadonlyArray.map ▼
		local _newValue = table.create(#_exp)
		local _callback = function(i)
			return string.char(i)
		end
		for _k, _v in _exp do
			_newValue[_k] = _callback(_v, _k - 1, _exp)
		end
		-- ▲ ReadonlyArray.map ▲
		local chars = _newValue
		local words = { "Hello", "world" }
		local hSplit = { "", "ello, world" }
		local _exp_1 = string.split(str, "")
		-- ▼ ReadonlyArray.every ▼
		local _result = true
		local _callback_1 = function(char, i)
			return char == chars[i + 1]
		end
		for _k, _v in _exp_1 do
			if not _callback_1(_v, _k - 1, _exp_1) then
				_result = false
				break
			end
		end
		-- ▲ ReadonlyArray.every ▲
		expect(_result).to.equal(true)
		local _exp_2 = string.split(str, ", ")
		-- ▼ ReadonlyArray.every ▼
		local _result_1 = true
		local _callback_2 = function(word, i)
			return word == words[i + 1]
		end
		for _k, _v in _exp_2 do
			if not _callback_2(_v, _k - 1, _exp_2) then
				_result_1 = false
				break
			end
		end
		-- ▲ ReadonlyArray.every ▲
		expect(_result_1).to.equal(true)
		local _exp_3 = string.split(str, "H")
		-- ▼ ReadonlyArray.every ▼
		local _result_2 = true
		local _callback_3 = function(word, i)
			return word == hSplit[i + 1]
		end
		for _k, _v in _exp_3 do
			if not _callback_3(_v, _k - 1, _exp_3) then
				_result_2 = false
				break
			end
		end
		-- ▲ ReadonlyArray.every ▲
		expect(_result_2).to.equal(true)
		expect(checkLen(1, string.split("", "a"))[1]).to.equal("")
		for i = 2, 9 do
			local _arg0 = i - 1
			local str = string.rep("d", _arg0)
			local str1 = string.split(str, "d")
			expect(#str1).to.equal(i)
			-- ▼ ReadonlyArray.every ▼
			local _result_3 = true
			local _callback_4 = function(c)
				return c == ""
			end
			for _k, _v in str1 do
				if not _callback_4(_v, _k - 1, str1) then
					_result_3 = false
					break
				end
			end
			-- ▲ ReadonlyArray.every ▲
			expect(_result_3).to.equal(true)
		end
		expect(#string.split("", "") == 0).to.equal(true)
		local slasher = { "", "Validark", "Osyris", "Vorlias", "" }
		local _exp_4 = checkLen(5, string.split("/Validark/Osyris/Vorlias/", "/"))
		-- ▼ ReadonlyArray.every ▼
		local _result_3 = true
		local _callback_4 = function(word, i)
			return word == slasher[i + 1]
		end
		for _k, _v in _exp_4 do
			if not _callback_4(_v, _k - 1, _exp_4) then
				_result_3 = false
				break
			end
		end
		-- ▲ ReadonlyArray.every ▲
		expect(_result_3).to.equal(true)
		local _exp_5 = checkLen(4, string.split("Validark/Osyris/Vorlias/", "/"))
		-- ▼ ReadonlyArray.every ▼
		local _result_4 = true
		local _callback_5 = function(word, i)
			return word == slasher[i + 2]
		end
		for _k, _v in _exp_5 do
			if not _callback_5(_v, _k - 1, _exp_5) then
				_result_4 = false
				break
			end
		end
		-- ▲ ReadonlyArray.every ▲
		expect(_result_4).to.equal(true)
		local _exp_6 = checkLen(3, string.split("Validark/Osyris/Vorlias", "/"))
		-- ▼ ReadonlyArray.every ▼
		local _result_5 = true
		local _callback_6 = function(word, i)
			return word == slasher[i + 2]
		end
		for _k, _v in _exp_6 do
			if not _callback_6(_v, _k - 1, _exp_6) then
				_result_5 = false
				break
			end
		end
		-- ▲ ReadonlyArray.every ▲
		expect(_result_5).to.equal(true)
	end)
	it("should support calling gmatch", function()
		expect((string.gmatch("Hello", ".")())).to.equal("H")
	end)
	it("should support the spread operator on strings", function()
		local array4 = { "H", "i", "y", "a" }
		local _array = {}
		local _length = #_array
		for _char in string.gmatch("Hiya", utf8.charpattern) do
			_length += 1
			_array[_length] = _char
		end
		-- ▼ ReadonlyArray.every ▼
		local _result = true
		local _callback = function(x, i)
			return x == array4[i + 1]
		end
		for _k, _v in _array do
			if not _callback(_v, _k - 1, _array) then
				_result = false
				break
			end
		end
		-- ▲ ReadonlyArray.every ▲
		expect(_result).to.equal(true)
	end)
	it("should support string.find", function()
		local data = { string.find("Hello", "H", 1, true) }
		local _value = data[1]
		if _value ~= 0 and _value == _value and _value then
			expect(data[1]).to.equal(1)
			expect(data[2]).to.equal(1)
		end
		local data2 = { string.find("Hello", "e", 2, true) }
		local _value_1 = data2[1]
		if _value_1 ~= 0 and _value_1 == _value_1 and _value_1 then
			expect(data2[1]).to.equal(2)
			expect(data2[2]).to.equal(2)
		end
	end)
	it("should support concatenating strings", function()
		expect("a" .. tostring(1) .. tostring(true) .. tostring(false)).to.equal("a1truefalse")
	end)
	it("should support variable string indices", function()
		local i = 1
		local j = 3
		local _i = i
		local _j = j
		expect(string.sub("foobar", _i, _j)).to.equal("foo")
	end)
	it("should support proper destructuring and iterating", function()
		local function compare(results, array2)
			for i = 0, #array2 - 1 do
				expect(results[i + 1]).to.equal(array2[i + 1])
			end
		end
		-- optimized destructuring
		local _array = {}
		local _length = #_array
		for _char in string.gmatch("𝟘𝟙𝟚𝟛", utf8.charpattern) do
			_length += 1
			_array[_length] = _char
		end
		compare(_array, { "𝟘", "𝟙", "𝟚", "𝟛" })
		local _array_1 = {}
		local _length_1 = #_array_1
		for _char in string.gmatch("யாமறிந்த", utf8.charpattern) do
			_length_1 += 1
			_array_1[_length_1] = _char
		end
		compare(_array_1, { "ய", "ா", "ம", "ற", "ி", "ந", "்", "த" })
		local spreadString = function(str)
			local _array_2 = {}
			local _length_2 = #_array_2
			for _char in string.gmatch(str, utf8.charpattern) do
				_length_2 += 1
				_array_2[_length_2] = _char
			end
			return _array_2
		end
		-- run-time destructuring
		compare(spreadString("𝟘𝟙𝟚𝟛"), { "𝟘", "𝟙", "𝟚", "𝟛" })
		compare(spreadString("யாமறிந்த"), { "ய", "ா", "ம", "ற", "ி", "ந", "்", "த" })
		local i = 0
		for substr in string.gmatch("𝟘𝟙𝟚𝟛", utf8.charpattern) do
			local _to = expect(substr).to
			local _exp = { "𝟘", "𝟙", "𝟚", "𝟛" }
			local _original = i
			i += 1
			_to.equal(_exp[_original + 1])
		end
		local j = 0
		local myStr = "யாமறிந்த"
		for substr in string.gmatch(myStr, utf8.charpattern) do
			local _to = expect(substr).to
			local _exp = { "ய", "ா", "ம", "ற", "ி", "ந", "்", "த" }
			local _original = j
			j += 1
			_to.equal(_exp[_original + 1])
		end
	end)
	it("should support multiline strings as an object index", function()
		local key = `str\
			ing`
		local obj = {
			[`str\
			ing`] = "foo",
		}
		-- prettier-ignore
		expect(obj[`str\
			ing`]).to.equal("foo")
		expect(obj[key]).to.equal("foo")
		-- prettier-ignore
		obj[`str\
			ing`] = "bar"
		-- prettier-ignore
		expect(obj[`str\
			ing`]).to.equal("bar")
		expect(obj[key]).to.equal("bar")
	end)
	-- issue #1467
	it([=[should support strings with \", ', and ending with ]]=], function()
		local str1 = `A string with a " and ' ending in a ]`
		expect(#str1).to.equal(37)
		local str2 = `A string with a " and ' ending in a ]] ]=`
		expect(#str2).to.equal(41)
	end)
end
