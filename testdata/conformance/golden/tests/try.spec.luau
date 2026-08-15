-- Compiled with sloptor v2.3.1
local TS = require(script.Parent.Parent.include.RuntimeLib)
return function()
	it("should support try/catch", function()
		local x = 123
		TS.try(function()
			x = 456
		end, function(e) end)
		expect(x).to.equal(456)
	end)
	it("should support try/catch with throwing strings", function()
		local function foo()
			local exception
			TS.try(function()
				error("bar")
			end, function(e)
				exception = e
			end)
			return exception
		end
		expect(foo()).to.be.a("string")
		expect((string.find(tostring((foo())), "bar"))).to.be.ok()
	end)
	it("should support try/catch with throwing objects", function()
		local function foo()
			local exception
			TS.try(function()
				error({})
			end, function(e)
				exception = e
			end)
			return exception
		end
		expect(foo()).to.be.a("table")
	end)
	it("should support try/catch with return in try block", function()
		local function foo()
			local _exitType, _returns = TS.try(function()
				return TS.TRY_RETURN, { "foo" }
			end, function(e) end)
			if _exitType then
				return unpack(_returns)
			end
		end
		expect(foo()).to.equal("foo")
	end)
	it("should support try/catch with return in catch block", function()
		local function foo()
			local _exitType, _returns = TS.try(function()
				error(nil)
			end, function()
				return TS.TRY_RETURN, { "foo" }
			end)
			if _exitType then
				return unpack(_returns)
			end
		end
		expect(foo()).to.equal("foo")
	end)
	it("should support try/catch with return in finally block", function()
		local function foo()
			local _exitType, _returns = TS.try(function()
				return TS.TRY_RETURN, { 1 }
			end, nil, function()
				return TS.TRY_RETURN, { 2 }
			end)
			if _exitType then
				return unpack(_returns)
			end
		end
		expect(foo()).to.equal(2)
	end)
	it("should support try/catch with break", function()
		local x = 0
		for i = 0, 1 do
			x += 1
			local _exitType, _returns = TS.try(function()
				return TS.TRY_BREAK
			end, function(e) end)
			if _exitType then
				break
			end
		end
		expect(x).to.equal(1)
	end)
	it("should support try/catch with continue", function()
		local x = 0
		for i = 0, 1 do
			if i % 2 == 0 then
				local _exitType, _returns = TS.try(function()
					return TS.TRY_CONTINUE
				end, function(e) end)
				if _exitType then
					continue
				end
			end
			x += 1
		end
		expect(x).to.equal(1)
	end)
	it("should support try/catch without a catch variable", function()
		local function foo()
			local exception
			TS.try(function()
				error("bar")
			end, function()
				exception = "abc"
			end)
			return exception
		end
		expect(foo()).to.equal("abc")
	end)
	it("should support try/catch with finally", function()
		local values = {}
		TS.try(function()
			table.insert(values, "A")
			error("bar")
		end, function()
			table.insert(values, "B")
		end, function()
			table.insert(values, "C")
		end)
		expect(values[1]).to.equal("A")
		expect(values[2]).to.equal("B")
		expect(values[3]).to.equal("C")
	end)
	it("should support try with finally", function()
		local values = {}
		TS.try(function()
			table.insert(values, "A")
		end, nil, function()
			table.insert(values, "C")
		end)
		expect(values[1]).to.equal("A")
		expect(values[2]).to.equal("C")
	end)
	it("should support nested try/catch with return", function()
		local function foo()
			local _exitType, _returns = TS.try(function()
				local _exitType_1, _returns_1 = TS.try(function()
					return TS.TRY_RETURN, { "foo" }
				end, function(e) end)
				if _exitType_1 then
					return _exitType_1, _returns_1
				end
			end, function(e) end)
			if _exitType then
				return unpack(_returns)
			end
		end
		expect(foo()).to.be.a("string")
	end)
	it("should support nested try/catch with break", function()
		local x = 0
		for i = 0, 1 do
			x += 1
			local _exitType, _returns = TS.try(function()
				local _exitType_1, _returns_1 = TS.try(function()
					return TS.TRY_BREAK
				end, function(e) end)
				if _exitType_1 then
					return _exitType_1
				end
			end, function(e) end)
			if _exitType then
				break
			end
		end
		expect(x).to.equal(1)
	end)
	it("should support nested try/catch with continue", function()
		local x = 0
		for i = 0, 1 do
			if i % 2 == 0 then
				local _exitType, _returns = TS.try(function()
					local _exitType_1, _returns_1 = TS.try(function()
						return TS.TRY_CONTINUE
					end, function(e) end)
					if _exitType_1 then
						return _exitType_1
					end
				end, function(e) end)
				if _exitType then
					continue
				end
			end
			x += 1
		end
		expect(x).to.equal(1)
	end)
	it("should support try/catch with multiple flow control cases", function()
		local function foo()
			local x = 0
			for i = 0, 9 do
				local _exitType, _returns = TS.try(function()
					if i == 5 then
						return TS.TRY_RETURN, { x }
					elseif i % 2 == 0 then
						return TS.TRY_CONTINUE
					end
					x += 1
				end, function() end)
				if _exitType == TS.TRY_RETURN then
					return unpack(_returns)
				elseif _exitType then
					continue
				end
			end
		end
		expect(foo()).to.equal(2)
	end)
	it("should support loops between nested try/catch with break", function()
		local function foo()
			local x = 0
			local _exitType, _returns = TS.try(function()
				for i = 0, 9 do
					local _exitType_1, _returns_1 = TS.try(function()
						if i == 5 then
							return TS.TRY_RETURN, { x }
						elseif i % 2 == 0 then
							return TS.TRY_CONTINUE
						end
						x += 1
					end, function() end)
					if _exitType_1 == TS.TRY_RETURN then
						return _exitType_1, _returns_1
					elseif _exitType_1 then
						continue
					end
				end
			end, function() end)
			if _exitType then
				return unpack(_returns)
			end
		end
		expect(foo()).to.equal(2)
	end)
	it("should rethrow the error if there's no catch block", function()
		local function foo()
			TS.try(function()
				error("bar")
			end, nil, function() end)
		end
		expect(function()
			return foo()
		end).to:throw("bar")
	end)
	it("should run try -> catch -> finally in order", function()
		local array = {}
		TS.try(function()
			local condition = true
			TS.try(function()
				table.insert(array, 1)
				if condition then
					error("error")
				end
				table.insert(array, 999)
			end, function()
				table.insert(array, 2)
			end, function()
				table.insert(array, 3)
			end)
		end, function() end)
		expect(array[1]).to.equal(1)
		expect(array[2]).to.equal(2)
		expect(array[3]).to.equal(3)
	end)
	it("should run try -> finally in order", function()
		local array = {}
		TS.try(function()
			local condition = true
			TS.try(function()
				table.insert(array, 1)
				if condition then
					error("error")
				end
				table.insert(array, 999)
			end, nil, function()
				table.insert(array, 2)
			end)
		end, function() end)
		expect(array[1]).to.equal(1)
		expect(array[2]).to.equal(2)
	end)
	it("should run finally even if catch throws", function()
		local ranFinally = false
		TS.try(function()
			TS.try(function()
				error("try error")
			end, function()
				error("catch error")
			end, function()
				ranFinally = true
			end)
		end, function() end)
		expect(ranFinally).to.equal(true)
	end)
	it("should throw if finally throws", function()
		local function foo()
			TS.try(function() end, nil, function()
				error("bar")
			end)
		end
		expect(function()
			return foo()
		end).to:throw("bar")
	end)
	it("should discard errors if finally has a control flow statement", function()
		local function tryErrorReturn()
			local _exitType, _returns = TS.try(function()
				error("try error")
			end, nil, function()
				return TS.TRY_RETURN, { true }
			end)
			if _exitType then
				return unpack(_returns)
			end
			return false
		end
		expect(tryErrorReturn()).to.equal(true)
		local function catchErrorReturn()
			local _exitType, _returns = TS.try(function()
				error("try error")
			end, function()
				error("catch error")
			end, function()
				return TS.TRY_RETURN, { true }
			end)
			if _exitType then
				return unpack(_returns)
			end
			return false
		end
		expect(catchErrorReturn()).to.equal(true)
		local function tryErrorBreak()
			for i = 0, 9 do
				local _exitType, _returns = TS.try(function()
					error("try error")
				end, nil, function()
					return TS.TRY_BREAK
				end)
				if _exitType then
					break
				end
				return false
			end
			return true
		end
		expect(tryErrorBreak()).to.equal(true)
		local function catchErrorBreak()
			for i = 0, 0 do
				local _exitType, _returns = TS.try(function()
					error("try error")
				end, function()
					error("catch error")
				end, function()
					return TS.TRY_BREAK
				end)
				if _exitType then
					break
				end
				return false
			end
			return true
		end
		expect(catchErrorBreak()).to.equal(true)
		local function tryErrorContinue()
			for i = 0, 0 do
				local _exitType, _returns = TS.try(function()
					error("try error")
				end, nil, function()
					return TS.TRY_CONTINUE
				end)
				if _exitType then
					continue
				end
				return false
			end
			return true
		end
		expect(tryErrorContinue()).to.equal(true)
		local function catchErrorContinue()
			for i = 0, 0 do
				local _exitType, _returns = TS.try(function()
					error("try error")
				end, function()
					error("catch error")
				end, function()
					return TS.TRY_CONTINUE
				end)
				if _exitType then
					continue
				end
				return false
			end
			return true
		end
		expect(catchErrorContinue()).to.equal(true)
	end)
end
