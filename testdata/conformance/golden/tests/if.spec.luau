-- Compiled with sloptor v2.3.1
return function()
	it("should support if statements", function()
		local function foo(n)
			if n < 5 then
				return "bar"
			elseif n < 10 then
				return "foo"
			else
				return "baz"
			end
		end
		expect(foo(9)).to.equal("foo")
		expect(foo(4)).to.equal("bar")
		expect(foo(11)).to.equal("baz")
	end)
	it("should support prereqs in elseif", function()
		local array = { 1 }
		if true then
		else
			-- ▼ Array.pop ▼
			local _length = #array
			local _result = array[_length]
			array[_length] = nil
			-- ▲ Array.pop ▲
			if _result == 1 then
			end
		end
		expect(#array).to.equal(1)
	end)
end
