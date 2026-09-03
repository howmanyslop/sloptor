-- Compiled with sloptor v2.3.3
local TS = require(script.Parent.Parent.include.RuntimeLib)
return function()
	it("should allow async function declarations", function()
		local foo = TS.async(function()
			return "foo"
		end)
		local bar = TS.async(function()
			return (TS.await(foo())) .. "bar"
		end)
		local success, value = bar():await()
		expect(success).to.equal(true)
		expect(value).to.equal("foobar")
	end)
	it("should allow async function expressions", function()
		local foo = TS.async(function()
			return "foo"
		end)
		local bar = TS.async(function()
			return (TS.await(foo())) .. "bar"
		end)
		local success, value = bar():await()
		expect(success).to.equal(true)
		expect(value).to.equal("foobar")
	end)
	it("should allow async arrow function expressions", function()
		local foo = TS.async(function()
			return "foo"
		end)
		local bar = TS.async(function()
			return (TS.await(foo())) .. "bar"
		end)
		local success, value = bar():await()
		expect(success).to.equal(true)
		expect(value).to.equal("foobar")
	end)
	it("should allow async static class methods", function()
		local X
		do
			X = setmetatable({}, {
				__tostring = function()
					return "X"
				end,
			})
			X.__index = X
			function X.new(...)
				local self = setmetatable({}, X)
				return self:constructor(...) or self
			end
			function X:constructor()
			end
			X.foo = TS.async(function(self)
				return "foo"
			end)
			X.bar = TS.async(function(self)
				return (TS.await(self:foo())) .. "bar"
			end)
		end
		local success, value = X:bar():await()
		expect(success).to.equal(true)
		expect(value).to.equal("foobar")
	end)
	it("should allow async class methods", function()
		local X
		do
			X = setmetatable({}, {
				__tostring = function()
					return "X"
				end,
			})
			X.__index = X
			function X.new(...)
				local self = setmetatable({}, X)
				return self:constructor(...) or self
			end
			function X:constructor()
			end
			X.foo = TS.async(function(self)
				return "foo"
			end)
			X.bar = TS.async(function(self)
				return (TS.await(self:foo())) .. "bar"
			end)
		end
		local success, value = X.new():bar():await()
		expect(success).to.equal(true)
		expect(value).to.equal("foobar")
	end)
end
