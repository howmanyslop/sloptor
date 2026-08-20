-- Compiled with sloptor v2.3.2
return function()
	it("should properly fetch types with parenthesis and nonNull assertions", function()
		local function loop(array)
			local i = 0
			-- prettier-ignore
			for _, value in array do
				local _to = expect(value).to
				local _original = i
				i += 1
				_to.equal(_original)
			end
		end
		loop({ 0, 1, 2, 3 })
	end)
	it("should support instantiated expressions", function()
		local function wow(str)
			return str
		end
		local foo = wow
		expect(foo("foo")).to.equal("foo")
	end)
end
