-- Compiled with sloptor v2.3.1
return function()
	it("should support set constructor", function()
		local set = {
			["foo"] = true,
			["bar"] = true,
			["baz"] = true,
		}
		expect(set.foo ~= nil).to.equal(true)
		expect(set.bar ~= nil).to.equal(true)
		expect(set.baz ~= nil).to.equal(true)
		do
			local _ = {
				[1] = true,
				[2] = true,
				[3] = true,
			}
			({
				[1] = true,
				[2] = true,
				[3] = true,
			})[4] = true
			local _exp = {
				[1] = true,
				[2] = true,
				[3] = true,
			}
			_exp[4] = true
			_exp[5] = true
			local u = {
				[1] = true,
				[2] = true,
				[3] = true,
			}
			local _exp_1 = {
				[1] = true,
				[2] = true,
				[3] = true,
			}
			_exp_1[4] = true
			local v = _exp_1
			local _exp_2 = {
				[1] = true,
				[2] = true,
				[3] = true,
			}
			_exp_2[4] = true
			_exp_2[5] = true
			local w = _exp_2
			local x = function()
				return {
					[1] = true,
					[2] = true,
					[3] = true,
				}
			end
			local y = function()
				local _exp_3 = {
					[1] = true,
					[2] = true,
					[3] = true,
				}
				_exp_3[4] = true
				return _exp_3
			end
			local z = function()
				local _exp_3 = {
					[1] = true,
					[2] = true,
					[3] = true,
				}
				_exp_3[4] = true
				_exp_3[5] = true
				return _exp_3
			end
			local i = 0
			local _1 = {
				[1] = true,
				[2] = true,
				[(function()
					local _original = i
					i += 1
					return _original
				end)()] = true,
			}
			({
				[1] = true,
				[2] = true,
				[(function()
					local _original = i
					i += 1
					return _original
				end)()] = true,
			})[4] = true
			local _exp_3 = {
				[1] = true,
				[2] = true,
				[(function()
					local _original = i
					i += 1
					return _original
				end)()] = true,
			}
			_exp_3[4] = true
			_exp_3[5] = true
			local a = {
				[1] = true,
				[2] = true,
				[(function()
					local _original = i
					i += 1
					return _original
				end)()] = true,
			}
			local _exp_4 = {
				[1] = true,
				[2] = true,
				[(function()
					local _original = i
					i += 1
					return _original
				end)()] = true,
			}
			_exp_4[4] = true
			local b = _exp_4
			local _exp_5 = {
				[1] = true,
				[2] = true,
				[(function()
					local _original = i
					i += 1
					return _original
				end)()] = true,
			}
			_exp_5[4] = true
			_exp_5[5] = true
			local c = _exp_5
			local d = function()
				return {
					[1] = true,
					[2] = true,
					[(function()
						local _original = i
						i += 1
						return _original
					end)()] = true,
				}
			end
			local e = function()
				local _exp_6 = {
					[1] = true,
					[2] = true,
					[(function()
						local _original = i
						i += 1
						return _original
					end)()] = true,
				}
				_exp_6[4] = true
				return _exp_6
			end
			local f = function()
				local _exp_6 = {
					[1] = true,
					[2] = true,
					[(function()
						local _original = i
						i += 1
						return _original
					end)()] = true,
				}
				_exp_6[4] = true
				_exp_6[5] = true
				return _exp_6
			end
			expect(i).to.equal(6)
			d()
			expect(i).to.equal(7)
			e()
			expect(i).to.equal(8)
			f()
			expect(i).to.equal(9)
		end
		do
			local _set = {}
			local _array = {}
			local _length = #_array
			local _array_1 = { 1, 2, 3 }
			table.move(_array_1, 1, #_array_1, _length + 1, _array)
			for _, _v in _array do
				_set[_v] = true
			end
			local _set_1 = {}
			local _array_2 = {}
			local _length_1 = #_array_2
			local _array_3 = { 1, 2, 3 }
			table.move(_array_3, 1, #_array_3, _length_1 + 1, _array_2)
			for _, _v in _array_2 do
				_set_1[_v] = true
			end
			_set_1[4] = true
			local _set_2 = {}
			local _array_4 = {}
			local _length_2 = #_array_4
			local _array_5 = { 1, 2, 3 }
			table.move(_array_5, 1, #_array_5, _length_2 + 1, _array_4)
			for _, _v in _array_4 do
				_set_2[_v] = true
			end
			_set_2[4] = true
			_set_2[5] = true
			local _set_3 = {}
			local _array_6 = {}
			local _length_3 = #_array_6
			local _array_7 = { 1, 2, 3 }
			table.move(_array_7, 1, #_array_7, _length_3 + 1, _array_6)
			for _, _v in _array_6 do
				_set_3[_v] = true
			end
			local u = _set_3
			local _set_4 = {}
			local _array_8 = {}
			local _length_4 = #_array_8
			local _array_9 = { 1, 2, 3 }
			table.move(_array_9, 1, #_array_9, _length_4 + 1, _array_8)
			for _, _v in _array_8 do
				_set_4[_v] = true
			end
			_set_4[4] = true
			local v = _set_4
			local _set_5 = {}
			local _array_10 = {}
			local _length_5 = #_array_10
			local _array_11 = { 1, 2, 3 }
			table.move(_array_11, 1, #_array_11, _length_5 + 1, _array_10)
			for _, _v in _array_10 do
				_set_5[_v] = true
			end
			_set_5[4] = true
			_set_5[5] = true
			local w = _set_5
			local x = function()
				local _set_6 = {}
				local _array_12 = {}
				local _length_6 = #_array_12
				local _array_13 = { 1, 2, 3 }
				table.move(_array_13, 1, #_array_13, _length_6 + 1, _array_12)
				for _, _v in _array_12 do
					_set_6[_v] = true
				end
				return _set_6
			end
			local y = function()
				local _set_6 = {}
				local _array_12 = {}
				local _length_6 = #_array_12
				local _array_13 = { 1, 2, 3 }
				table.move(_array_13, 1, #_array_13, _length_6 + 1, _array_12)
				for _, _v in _array_12 do
					_set_6[_v] = true
				end
				_set_6[4] = true
				return _set_6
			end
			local z = function()
				local _set_6 = {}
				local _array_12 = {}
				local _length_6 = #_array_12
				local _array_13 = { 1, 2, 3 }
				table.move(_array_13, 1, #_array_13, _length_6 + 1, _array_12)
				for _, _v in _array_12 do
					_set_6[_v] = true
				end
				_set_6[4] = true
				_set_6[5] = true
				return _set_6
			end
			local i = 0
			local _set_6 = {}
			local _array_12 = {}
			local _length_6 = #_array_12
			local _array_13 = { 1, 2, (function()
				local _original = i
				i += 1
				return _original
			end)() }
			table.move(_array_13, 1, #_array_13, _length_6 + 1, _array_12)
			for _, _v in _array_12 do
				_set_6[_v] = true
			end
			local _set_7 = {}
			local _array_14 = {}
			local _length_7 = #_array_14
			local _array_15 = { 1, 2, (function()
				local _original = i
				i += 1
				return _original
			end)() }
			table.move(_array_15, 1, #_array_15, _length_7 + 1, _array_14)
			for _, _v in _array_14 do
				_set_7[_v] = true
			end
			_set_7[4] = true
			local _set_8 = {}
			local _array_16 = {}
			local _length_8 = #_array_16
			local _array_17 = { 1, 2, (function()
				local _original = i
				i += 1
				return _original
			end)() }
			table.move(_array_17, 1, #_array_17, _length_8 + 1, _array_16)
			for _, _v in _array_16 do
				_set_8[_v] = true
			end
			_set_8[4] = true
			_set_8[5] = true
			local _set_9 = {}
			local _array_18 = {}
			local _length_9 = #_array_18
			local _array_19 = { 1, 2, (function()
				local _original = i
				i += 1
				return _original
			end)() }
			table.move(_array_19, 1, #_array_19, _length_9 + 1, _array_18)
			for _, _v in _array_18 do
				_set_9[_v] = true
			end
			local a = _set_9
			local _set_10 = {}
			local _array_20 = {}
			local _length_10 = #_array_20
			local _array_21 = { 1, 2, (function()
				local _original = i
				i += 1
				return _original
			end)() }
			table.move(_array_21, 1, #_array_21, _length_10 + 1, _array_20)
			for _, _v in _array_20 do
				_set_10[_v] = true
			end
			_set_10[4] = true
			local b = _set_10
			local _set_11 = {}
			local _array_22 = {}
			local _length_11 = #_array_22
			local _array_23 = { 1, 2, (function()
				local _original = i
				i += 1
				return _original
			end)() }
			table.move(_array_23, 1, #_array_23, _length_11 + 1, _array_22)
			for _, _v in _array_22 do
				_set_11[_v] = true
			end
			_set_11[4] = true
			_set_11[5] = true
			local c = _set_11
			local d = function()
				local _set_12 = {}
				local _array_24 = {}
				local _length_12 = #_array_24
				local _array_25 = { 1, 2, (function()
					local _original = i
					i += 1
					return _original
				end)() }
				table.move(_array_25, 1, #_array_25, _length_12 + 1, _array_24)
				for _, _v in _array_24 do
					_set_12[_v] = true
				end
				return _set_12
			end
			local e = function()
				local _set_12 = {}
				local _array_24 = {}
				local _length_12 = #_array_24
				local _array_25 = { 1, 2, (function()
					local _original = i
					i += 1
					return _original
				end)() }
				table.move(_array_25, 1, #_array_25, _length_12 + 1, _array_24)
				for _, _v in _array_24 do
					_set_12[_v] = true
				end
				_set_12[4] = true
				return _set_12
			end
			local f = function()
				local _set_12 = {}
				local _array_24 = {}
				local _length_12 = #_array_24
				local _array_25 = { 1, 2, (function()
					local _original = i
					i += 1
					return _original
				end)() }
				table.move(_array_25, 1, #_array_25, _length_12 + 1, _array_24)
				for _, _v in _array_24 do
					_set_12[_v] = true
				end
				_set_12[4] = true
				_set_12[5] = true
				return _set_12
			end
			expect(i).to.equal(6)
			d()
			expect(i).to.equal(7)
			e()
			expect(i).to.equal(8)
			f()
			expect(i).to.equal(9)
		end
	end)
	it("should support weak sets", function()
		local set = setmetatable({}, {
			__mode = "k",
		})
		local f = Instance.new("Frame")
		set[f] = true
		expect(set[f] ~= nil).to.equal(true)
		local i = 0
		local _object = {}
		local _left = "x"
		local _original = i
		i += 1
		_object[_left] = _original
		local k = _object
		setmetatable({}, {
			__mode = "k",
		})
		setmetatable({}, {
			__mode = "k",
		})
		local _set = {}
		local _array = {}
		local _length = #_array
		local _array_1 = { {} }
		table.move(_array_1, 1, #_array_1, _length + 1, _array)
		for _, _v in _array do
			_set[_v] = true
		end
		setmetatable(_set, {
			__mode = "k",
		})
		setmetatable({
			[{}] = true,
		}, {
			__mode = "k",
		})
		local _object_1 = {}
		local _left_1 = "x"
		local _original_1 = i
		i += 1
		_object_1[_left_1] = _original_1
		setmetatable({
			[_object_1] = true,
		}, {
			__mode = "k",
		})
		local _object_2 = table.clone(k)
		setmetatable(_object_2, nil)
		setmetatable({
			[_object_2] = true,
		}, {
			__mode = "k",
		})
		expect(getmetatable(setmetatable({}, {
			__mode = "k",
		})).__mode).to.equal("k")
		expect(getmetatable(setmetatable({}, {
			__mode = "k",
		})).__mode).to.equal("k")
		local _set_1 = {}
		local _array_2 = {}
		local _length_1 = #_array_2
		local _array_3 = { {} }
		table.move(_array_3, 1, #_array_3, _length_1 + 1, _array_2)
		for _, _v in _array_2 do
			_set_1[_v] = true
		end
		expect(getmetatable(setmetatable(_set_1, {
			__mode = "k",
		})).__mode).to.equal("k")
		expect(getmetatable(setmetatable({
			[{}] = true,
		}, {
			__mode = "k",
		})).__mode).to.equal("k")
		local _object_3 = {}
		local _left_2 = "x"
		local _original_2 = i
		i += 1
		_object_3[_left_2] = _original_2
		expect(getmetatable(setmetatable({
			[_object_3] = true,
		}, {
			__mode = "k",
		})).__mode).to.equal("k")
		local _object_4 = table.clone(k)
		setmetatable(_object_4, nil)
		expect(getmetatable(setmetatable({
			[_object_4] = true,
		}, {
			__mode = "k",
		})).__mode).to.equal("k")
		local u = setmetatable({}, {
			__mode = "k",
		})
		local v = setmetatable({}, {
			__mode = "k",
		})
		local _set_2 = {}
		local _array_4 = {}
		local _length_2 = #_array_4
		local _array_5 = { {} }
		table.move(_array_5, 1, #_array_5, _length_2 + 1, _array_4)
		for _, _v in _array_4 do
			_set_2[_v] = true
		end
		local w = setmetatable(_set_2, {
			__mode = "k",
		})
		local x = setmetatable({
			[{}] = true,
		}, {
			__mode = "k",
		})
		local _object_5 = {}
		local _left_3 = "x"
		local _original_3 = i
		i += 1
		_object_5[_left_3] = _original_3
		local y = setmetatable({
			[_object_5] = true,
		}, {
			__mode = "k",
		})
		local _object_6 = table.clone(k)
		setmetatable(_object_6, nil)
		local z = setmetatable({
			[_object_6] = true,
		}, {
			__mode = "k",
		})
		expect(getmetatable(u).__mode).to.equal("k")
		expect(getmetatable(v).__mode).to.equal("k")
		expect(getmetatable(w).__mode).to.equal("k")
		expect(getmetatable(x).__mode).to.equal("k")
		expect(getmetatable(y).__mode).to.equal("k")
		expect(getmetatable(z).__mode).to.equal("k")
	end)
	it("should support add", function()
		local set = {}
		set.foo = true
		set.test = true
		expect(set).to.equal(set)
		expect(set.foo ~= nil).to.equal(true)
	end)
	it("should support has", function()
		local _exp = {}
		_exp.a = true
		_exp.b = true
		_exp.c = true
		local set = _exp
		local _ = set.a ~= nil
		expect(set.a ~= nil).to.equal(true)
		expect(set.b ~= nil).to.equal(true)
		expect(set.c ~= nil).to.equal(true)
		expect(set.d ~= nil).to.equal(false)
	end)
	it("should support clear", function()
		local _exp = {}
		_exp.a = true
		_exp.b = true
		_exp.c = true
		local set = _exp
		table.clear(set)
		-- ▼ ReadonlySet.size ▼
		local _size = 0
		for _ in set do
			_size += 1
		end
		-- ▲ ReadonlySet.size ▲
		expect(_size).to.equal(0)
		expect(set.a ~= nil).to.equal(false)
		expect(set.b ~= nil).to.equal(false)
		expect(set.c ~= nil).to.equal(false)
	end)
	it("should support delete", function()
		local _exp = {}
		_exp.a = true
		_exp.b = true
		_exp.c = true
		local set = _exp
		-- ▼ ReadonlySet.size ▼
		local _size = 0
		for _ in set do
			_size += 1
		end
		-- ▲ ReadonlySet.size ▲
		expect(_size).to.equal(3)
		expect(set.b ~= nil).to.equal(true)
		-- ▼ Set.delete ▼
		local _valueExisted = set.b ~= nil
		set.b = nil
		-- ▲ Set.delete ▲
		local hadB = _valueExisted
		-- ▼ Set.delete ▼
		local _valueExisted_1 = set.d ~= nil
		set.d = nil
		-- ▲ Set.delete ▲
		local hadD = _valueExisted_1
		expect(hadB).to.equal(true)
		expect(hadD).to.equal(false)
		-- ▼ ReadonlySet.size ▼
		local _size_1 = 0
		for _ in set do
			_size_1 += 1
		end
		-- ▲ ReadonlySet.size ▲
		expect(_size_1).to.equal(2)
		expect(set.b ~= nil).to.equal(false)
		expect(set.a ~= nil).to.equal(true)
		set.a = nil
		expect(set.a ~= nil).to.equal(false)
	end)
	it("should support forEach", function()
		local hitA = 0
		local hitB = 0
		local hitC = 0
		local _exp = {}
		_exp.a = true
		_exp.b = true
		_exp.c = true
		local set = _exp
		-- ▼ ReadonlySet.forEach ▼
		local _callback = function(value, value2, obj)
			expect(value).to.equal(value2)
			expect(obj).to.equal(set)
			if value == "a" then
				hitA += 1
			elseif value == "b" then
				hitB += 1
			elseif value == "c" then
				hitC += 1
			end
		end
		for _v in set do
			_callback(_v, _v, set)
		end
		-- ▲ ReadonlySet.forEach ▲
		expect(hitA).to.equal(1)
		expect(hitB).to.equal(1)
		expect(hitC).to.equal(1)
	end)
	it("should support size", function()
		local _exp = {}
		_exp.a = true
		_exp.b = true
		_exp.c = true
		local set = _exp
		-- ▼ ReadonlySet.size ▼
		local _size = 0
		for _ in set do
			_size += 1
		end
		-- ▲ ReadonlySet.size ▲
		expect(_size).to.equal(3)
		set.d = true
		-- ▼ ReadonlySet.size ▼
		local _size_1 = 0
		for _ in set do
			_size_1 += 1
		end
		-- ▲ ReadonlySet.size ▲
		expect(_size_1).to.equal(4)
	end)
	it("should support isEmpty", function()
		local _ = next({}) == nil
		local v = next({}) == nil
		local set = {}
		local _1 = next(set) == nil
		local x = next(set) == nil
		expect(v).to.equal(true)
		set.Nope = true
		expect(next(set) == nil).to.equal(false)
	end)
	it("should support creating ReadonlySets", function()
		local set = {
			["foo"] = true,
			["bar"] = true,
			["baz"] = true,
		}
		expect(set.foo ~= nil).to.equal(true)
		expect(set.bar ~= nil).to.equal(true)
		expect(set.baz ~= nil).to.equal(true)
	end)
	it("should support delete with with Set constructor", function()
		local count = 0
		local function foo(input)
			count += 1
			return input
		end
		local _exp = {
			[foo("a")] = true,
			[foo("b")] = true,
		}
		-- ▼ Set.delete ▼
		local _valueExisted = _exp.a ~= nil
		_exp.a = nil
		-- ▲ Set.delete ▲
		local wasRemoved = _valueExisted
		expect(wasRemoved).to.equal(true)
		expect(count).to.equal(2)
	end)
end
