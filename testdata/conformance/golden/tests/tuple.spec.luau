-- Compiled with sloptor v2.4.0
return function()
	it("should unpack function return tuples", function()
		local function foo()
			return { 101, 203 }
		end
		local _binding = foo()
		local a = _binding[1]
		local b = _binding[2]
		expect(a).to.equal(101)
		expect(b).to.equal(203)
		local c = foo()
		expect(c[1]).to.equal(101)
		expect(c[2]).to.equal(203)
		expect(foo()[1]).to.equal(101)
		expect(foo()[2]).to.equal(203)
	end)
	it("should support wrapping function results", function()
		local function foo()
			return { 1, 2, 3 }
		end
		expect(foo()[1]).to.equal(1)
		expect(foo()[2]).to.equal(2)
		expect(foo()[3]).to.equal(3)
	end)
	it("should support functions returning tuple calls", function()
		local function foo()
			return { 1, "2" }
		end
		local function bar()
			return foo()
		end
		local _binding = bar()
		local a = _binding[1]
		local b = _binding[2]
		expect(a).to.equal(1)
		expect(b).to.equal("2")
	end)
	it("should support wrapping tuple returns in tuple", function()
		local function foo()
			return { 1, "2" }
		end
		local function bar()
			return { foo(), true }
		end
		local _binding = bar()
		local _binding_1 = _binding[1]
		local a = _binding_1[1]
		local b = _binding_1[2]
		local c = _binding[2]
		expect(a).to.equal(1)
		expect(b).to.equal("2")
		expect(c).to.equal(true)
	end)
	it("should support function calls with tuple returns as expression statements", function()
		local itWorked = false
		local function foo()
			itWorked = true
			return { 1, "2" }
		end
		foo()
		expect(itWorked).to.equal(true)
	end)
	it("should support indirect tuple returns", function()
		local function foo()
			local result = { 1, 2, 3 }
			return result
		end
		local _binding = foo()
		local x = _binding[1]
		local y = _binding[2]
		local z = _binding[3]
		expect(x).to.equal(1)
		expect(y).to.equal(2)
		expect(z).to.equal(3)
	end)
	it("should allow tuples access to array functions", function()
		local function foo()
			local result = { 1, 2, 3 }
			return result
		end
		local _exp = foo()
		-- ▼ Array.pop ▼
		local _length = #_exp
		local _result = _exp[_length]
		_exp[_length] = nil
		-- ▲ Array.pop ▲
		expect(_result).to.equal(3)
	end)
	it("should unpack function return tuples with LuaTuple<T>", function()
		local function foo()
			return 101, 203
		end
		local a, b = foo()
		expect(a).to.equal(101)
		expect(b).to.equal(203)
		local c = { foo() }
		expect(c[1]).to.equal(101)
		expect(c[2]).to.equal(203)
		expect((foo())).to.equal(101)
		expect((select(2, foo()))).to.equal(203)
	end)
	it("should support functions returning tuple calls with LuaTuple<T>", function()
		local function foo()
			return 1, "2"
		end
		local function bar()
			return foo()
		end
		local a, b = bar()
		expect(a).to.equal(1)
		expect(b).to.equal("2")
	end)
	it("should support wrapping tuple returns in tuple with LuaTuple<T>", function()
		local function foo()
			return 1, "2"
		end
		local function bar()
			return { foo() }, true
		end
		local _binding, c = bar()
		local a = _binding[1]
		local b = _binding[2]
		expect(a).to.equal(1)
		expect(b).to.equal("2")
		expect(c).to.equal(true)
	end)
	it("should support function calls with tuple returns as expression statements with LuaTuple<T>", function()
		local itWorked = false
		local function foo()
			itWorked = true
			return 1, "2"
		end
		foo()
		expect(itWorked).to.equal(true)
	end)
	it("should support indirect tuple returns with LuaTuple<T>", function()
		local function foo()
			local result = { 1, 2, 3 }
			return unpack(result)
		end
		local x, y, z = foo()
		expect(x).to.equal(1)
		expect(y).to.equal(2)
		expect(z).to.equal(3)
	end)
	it("should allow tuples access to array functions with LuaTuple<T>", function()
		local function foo()
			local result = { 1, 2, 3 }
			return unpack(result)
		end
		local _exp = { foo() }
		-- ▼ Array.pop ▼
		local _length = #_exp
		local _result = _exp[_length]
		_exp[_length] = nil
		-- ▲ Array.pop ▲
		expect(_result).to.equal(3)
	end)
	it("should allow LuaTuples to have Array<> inside", function()
		local function foo()
			return 1, 2, "3"
		end
		local _exp = { foo() }
		-- ▼ Array.pop ▼
		local _length = #_exp
		local _result = _exp[_length]
		_exp[_length] = nil
		-- ▲ Array.pop ▲
		expect(_result).to.equal("3")
	end)
	it("should support assigning from LuaTuples", function()
		local function foo()
			return 101, 203
		end
		local a = 0
		local b = 0
		a, b = foo()
		expect(a).to.equal(101)
		expect(b).to.equal(203)
	end)
	it("should support assigning from LuaTuples with omitted expressions", function()
		local function foo()
			return 101, 203, 345
		end
		local a = 0
		local b = 0
		a, _, b = foo()
		expect(a).to.equal(101)
		expect(b).to.equal(345)
	end)
	it("should support nested assigning from LuaTuples", function()
		local function foo()
			return 101, { 203, 345 }
		end
		local a = 0
		local b = 0
		local _binding
		a, _binding = foo()
		b = _binding[2]
		expect(a).to.equal(101)
		expect(b).to.equal(345)
	end)
	it("should support nested assigning from LuaTuples 2", function()
		local function foo()
			return 101, {
				a = 203,
				b = 345,
			}
		end
		local a = 0
		local b = 0
		local _binding
		a, _binding = foo()
		b = _binding.b
		expect(a).to.equal(101)
		expect(b).to.equal(345)
	end)
	it("should wrap LuaTuple returns in for loop conditions", function()
		local function luaTupleReturn()
			return false
		end
		local hasRun1 = false
		while { luaTupleReturn() } do
			hasRun1 = true
			break
		end
		expect(hasRun1).to.equal(true)
	end)
	it("should support $tuple macro", function()
		local function luaTupleMacroReturn()
			return 123, "abc", true
		end
		local tuple = { luaTupleMacroReturn() }
		expect(tuple[1]).to.equal(123)
		expect(tuple[2]).to.equal("abc")
		expect(tuple[3]).to.equal(true)
	end)
	it("should support $tuple macro with destructuring", function()
		local function luaTupleMacroReturn()
			return 123, "abc", true
		end
		local a, b, c = luaTupleMacroReturn()
		expect(a).to.equal(123)
		expect(b).to.equal("abc")
		expect(c).to.equal(true)
	end)
	it("should support $tuple macro in nested calls", function()
		local function luaTupleMacroReturn()
			return 123, "abc", true
		end
		local function wrapperFunction()
			return luaTupleMacroReturn()
		end
		local a, b, c = wrapperFunction()
		expect(a).to.equal(123)
		expect(b).to.equal("abc")
		expect(c).to.equal(true)
	end)
end
