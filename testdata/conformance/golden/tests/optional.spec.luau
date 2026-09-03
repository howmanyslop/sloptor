-- Compiled with sloptor v2.4.0
return function()
	describe("should support optional function calls", function()
		local foo = (function()
			return "bar"
		end)
		local _result = foo
		if _result ~= nil then
			_result = _result()
		end
		expect(_result).to.equal("bar")
	end)
end
