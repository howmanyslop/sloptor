-- Compiled with sloptor v2.3.1
return function()
	it("should support map constructor", function()
		local map = {
			foo = 1,
			bar = 2,
			baz = 3,
		}
		expect(map.foo).to.equal(1)
		expect(map.bar).to.equal(2)
		expect(map.baz).to.equal(3)
	end)
	it("should support weak maps", function()
		local map = setmetatable({}, {
			__mode = "k",
		})
		local f = Instance.new("Frame")
		map[f] = "foo"
		expect(map[f]).to.equal("foo")
	end)
	it("should support get and set", function()
		local _exp = {}
		_exp.a = 1
		_exp.b = 2
		_exp.c = 3
		local map = _exp
		local _ = map.a
		expect(map.a).to.equal(1)
		expect(map.b).to.equal(2)
		expect(map.c).to.equal(3)
		map.d = 4
		expect(map).to.equal(map)
		map.e = 5
		expect(map.e).to.equal(5)
	end)
	it("should support has", function()
		local _exp = {}
		_exp.a = 1
		_exp.b = 2
		_exp.c = 3
		local map = _exp
		local _ = map.a ~= nil
		expect(map.a ~= nil).to.equal(true)
		expect(map.b ~= nil).to.equal(true)
		expect(map.c ~= nil).to.equal(true)
		expect(map.d ~= nil).to.equal(false)
	end)
	it("should support delete", function()
		local _exp = {}
		_exp.a = 1
		_exp.b = 2
		_exp.c = 3
		local map = _exp
		-- ▼ Map.delete ▼
		local _valueExisted = map.a ~= nil
		map.a = nil
		-- ▲ Map.delete ▲
		local hadA = _valueExisted
		expect(hadA).to.equal(true)
		expect(map.a).never.to.be.ok()
	end)
	it("should support size", function()
		local _exp = {}
		_exp.a = 1
		_exp.b = 2
		_exp.c = 3
		local map = _exp
		-- ▼ ReadonlyMap.size ▼
		local _size = 0
		for _ in map do
			_size += 1
		end
		-- ▲ ReadonlyMap.size ▲
		expect(_size).to.equal(3)
		map.b = nil
		-- ▼ ReadonlyMap.size ▼
		local _size_1 = 0
		for _ in map do
			_size_1 += 1
		end
		-- ▲ ReadonlyMap.size ▲
		expect(_size_1).to.equal(2)
	end)
	it("should support clear", function()
		local _exp = {}
		_exp.a = 1
		_exp.b = 2
		_exp.c = 3
		local map = _exp
		table.clear(map)
		expect(map.a ~= nil).to.equal(false)
		expect(map.b ~= nil).to.equal(false)
		expect(map.c ~= nil).to.equal(false)
		expect(map.a).never.to.be.ok()
		expect(map.b).never.to.be.ok()
		expect(map.c).never.to.be.ok()
		-- ▼ ReadonlyMap.size ▼
		local _size = 0
		for _ in map do
			_size += 1
		end
		-- ▲ ReadonlyMap.size ▲
		expect(_size).to.equal(0)
	end)
	it("should support forEach", function()
		local hitA = 0
		local hitB = 0
		local hitC = 0
		local _exp = {}
		_exp.a = 1
		_exp.b = 2
		_exp.c = 3
		local map = _exp
		-- ▼ ReadonlyMap.forEach ▼
		local _callback = function(value, key, obj)
			if key == "a" and value == 1 then
				hitA += 1
			elseif key == "b" and value == 2 then
				hitB += 1
			elseif key == "c" and value == 3 then
				hitC += 1
			end
			expect(obj).to.equal(map)
		end
		for _k, _v in map do
			_callback(_v, _k, map)
		end
		-- ▲ ReadonlyMap.forEach ▲
		expect(hitA).to.equal(1)
		expect(hitB).to.equal(1)
		expect(hitC).to.equal(1)
	end)
	it("should support constructor parameters", function()
		local arr = { { "a", 1 }, { "b", 2 }, { "c", 3 } }
		local map = {
			a = 1,
			b = 2,
			c = 3,
		}
		local _map = {}
		for _, _v in arr do
			_map[_v[1]] = _v[2]
		end
		local map2 = _map
		local function values(map)
			local result = {}
			for _, value in map do
				table.insert(result, value)
			end
			return result
		end
		do
			local a = values(map)
			-- ▼ ReadonlyArray.some ▼
			local _result = false
			local _callback = function(v)
				return v == 1
			end
			for _k, _v in a do
				if _callback(_v, _k - 1, a) then
					_result = true
					break
				end
			end
			-- ▲ ReadonlyArray.some ▲
			expect(_result).to.equal(true)
			-- ▼ ReadonlyArray.some ▼
			local _result_1 = false
			local _callback_1 = function(v)
				return v == 2
			end
			for _k, _v in a do
				if _callback_1(_v, _k - 1, a) then
					_result_1 = true
					break
				end
			end
			-- ▲ ReadonlyArray.some ▲
			expect(_result_1).to.equal(true)
			-- ▼ ReadonlyArray.some ▼
			local _result_2 = false
			local _callback_2 = function(v)
				return v == 3
			end
			for _k, _v in a do
				if _callback_2(_v, _k - 1, a) then
					_result_2 = true
					break
				end
			end
			-- ▲ ReadonlyArray.some ▲
			expect(_result_2).to.equal(true)
		end
		do
			local a = values(map2)
			-- ▼ ReadonlyArray.some ▼
			local _result = false
			local _callback = function(v)
				return v == 1
			end
			for _k, _v in a do
				if _callback(_v, _k - 1, a) then
					_result = true
					break
				end
			end
			-- ▲ ReadonlyArray.some ▲
			expect(_result).to.equal(true)
			-- ▼ ReadonlyArray.some ▼
			local _result_1 = false
			local _callback_1 = function(v)
				return v == 2
			end
			for _k, _v in a do
				if _callback_1(_v, _k - 1, a) then
					_result_1 = true
					break
				end
			end
			-- ▲ ReadonlyArray.some ▲
			expect(_result_1).to.equal(true)
			-- ▼ ReadonlyArray.some ▼
			local _result_2 = false
			local _callback_2 = function(v)
				return v == 3
			end
			for _k, _v in a do
				if _callback_2(_v, _k - 1, a) then
					_result_2 = true
					break
				end
			end
			-- ▲ ReadonlyArray.some ▲
			expect(_result_2).to.equal(true)
		end
	end)
	it("should support isEmpty", function()
		local _ = next({}) == nil
		local v = next({}) == nil
		local map = {}
		local _1 = next(map) == nil
		local x = next(map) == nil
		expect(v).to.equal(true)
		map.Nope = 1
		expect(next(map) == nil).to.equal(false)
	end)
	-- it("should support the spread operator on maps", () => {
	-- 	expect(
	-- 		[
	-- 			...new Map([
	-- 				["a", 97],
	-- 				["b", 98],
	-- 			]),
	-- 			...new Map([
	-- 				["c", 99],
	-- 				["d", 100],
	-- 			]),
	-- 		].every(([l, n]) => l === string.char(n)),
	-- 	).to.equal(true);
	-- });
	it("should support the non-null assertion operator on maps", function()
		local a = {}
		local c
		c = a
		c[1] = 2
		expect(a[1]).to.equal(2)
		local b = {
			a = { 123 },
		}
		expect(b.a[1]).to.equal(123)
	end)
	it("should support creating ReadonlyMaps", function()
		local map = {
			foo = 1,
			bar = 2,
			baz = 3,
		}
		expect(map.foo).to.equal(1)
		expect(map.bar).to.equal(2)
		expect(map.baz).to.equal(3)
	end)
	it("should support creating Maps with tuple calls", function()
		local function foo()
			return { 123, "abc" }
		end
		local _map = {}
		for _, _v in { foo() } do
			_map[_v[1]] = _v[2]
		end
		local map = _map
		expect(map[123]).to.equal("abc")
	end)
	it("should support delete with with Map constructor", function()
		local count = 0
		local function foo(input)
			count += 1
			return input
		end
		local _exp = {
			[foo("a")] = foo("b"),
		}
		-- ▼ Map.delete ▼
		local _valueExisted = _exp.a ~= nil
		_exp.a = nil
		-- ▲ Map.delete ▲
		local wasRemoved = _valueExisted
		expect(wasRemoved).to.equal(true)
		expect(count).to.equal(2)
	end)
end
