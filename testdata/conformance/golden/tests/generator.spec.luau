-- Compiled with sloptor v2.3.0
local TS = require(script.Parent.Parent.include.RuntimeLib)
return function()
	it("should support generator function declarations", function()
		local function foo()
			return TS.generator(function()
				coroutine.yield(1)
				return 2
			end)
		end
		expect(foo()).to.be.ok()
	end)
	it("should support no return value", function()
		local function foo()
			return TS.generator(function()
				coroutine.yield(1)
			end)
		end
		for _result in foo().next do
			if _result.done then
				break
			end
			local result = _result.value
			expect(result).to.equal(1)
		end
	end)
	it("should support multiple yields", function()
		local function foo()
			return TS.generator(function()
				coroutine.yield(10)
				coroutine.yield(20)
			end)
		end
		local _array = {}
		local _length = #_array
		for _result in foo().next do
			if _result.done then
				break
			end
			_length += 1
			_array[_length] = _result.value
		end
		local result = _array
		expect(#result).to.equal(2)
		expect(result[1]).to.equal(10)
		expect(result[2]).to.equal(20)
	end)
	it("should support yield with asterisk token", function()
		local function foo()
			return TS.generator(function()
				coroutine.yield(1)
			end)
		end
		local function bar()
			return TS.generator(function()
				for _result in foo().next do
					if _result.done then
						break
					end
					coroutine.yield(_result.value)
				end
			end)
		end
		local _array = {}
		local _length = #_array
		for _result in bar().next do
			if _result.done then
				break
			end
			_length += 1
			_array[_length] = _result.value
		end
		local result = _array
		expect(#result).to.equal(1)
		expect(result[1]).to.equal(1)
	end)
	it("should not resume finished generator", function()
		local function foo()
			return TS.generator(function()
				coroutine.yield(1)
			end)
		end
		local function bar()
			return TS.generator(function()
				local generated = foo()
				for _result in generated.next do
					if _result.done then
						break
					end
					coroutine.yield(_result.value)
				end
				for _result in generated.next do
					if _result.done then
						break
					end
					coroutine.yield(_result.value)
				end
			end)
		end
		local _array = {}
		local _length = #_array
		for _result in bar().next do
			if _result.done then
				break
			end
			_length += 1
			_array[_length] = _result.value
		end
		local result = _array
		expect(#result).to.equal(1)
		expect(result[1]).to.equal(1)
	end)
	it("should properly define yield vs return", function()
		local function a()
			return TS.generator(function()
				coroutine.yield(1)
				coroutine.yield(2)
				return 3
			end)
		end
		local function c()
			return TS.generator(function()
				return 61
			end)
		end
		local function b()
			return TS.generator(function()
				coroutine.yield(0)
				for _result in c().next do
					if _result.done then
						break
					end
					coroutine.yield(_result.value)
				end
				local _returnValue
				for _result in a().next do
					if _result.done then
						_returnValue = _result.value
						break
					end
					coroutine.yield(_result.value)
				end
				local output = _returnValue
				coroutine.yield(output)
				coroutine.yield(4)
				return 5
			end)
		end
		local _array = {}
		local _length = #_array
		for _result in b().next do
			if _result.done then
				break
			end
			_length += 1
			_array[_length] = _result.value
		end
		local result = _array
		expect(#result).to.equal(5)
		expect(result[1]).to.equal(0)
		expect(result[2]).to.equal(1)
		expect(result[3]).to.equal(2)
		expect(result[4]).to.equal(3)
		expect(result[5]).to.equal(4)
	end)
end
