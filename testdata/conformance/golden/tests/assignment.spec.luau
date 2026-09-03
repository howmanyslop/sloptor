-- Compiled with sloptor v2.3.3
return function()
	it("should support logical null coalescing assignment statement", function()
		local x
		if x == nil then
			x = true
		end
		expect(x).to.equal(true)
	end)
	it("should support logical or assignment statement", function()
		local x = false
		local _condition = x
		if not x then
			_condition = true
		end
		x = _condition
		expect(x).to.equal(true)
	end)
	it("should support logical and assignment statement", function()
		local x = true
		local _condition = x
		if x then
			_condition = false
		end
		x = _condition
		expect(x).to.equal(false)
	end)
	it("should support logical null coalescing assignment expression", function()
		local x
		if x == nil then
			x = true
		end
		expect(x).to.equal(true)
		expect(x).to.equal(true)
	end)
	it("should support logical or assignment expression", function()
		local x = false
		local _condition = x
		if not x then
			_condition = true
		end
		x = _condition
		expect(x).to.equal(true)
		expect(x).to.equal(true)
	end)
	it("should support logical and assignment expression", function()
		local x = true
		local _condition = x
		if x then
			_condition = false
		end
		x = _condition
		expect(x).to.equal(false)
		expect(x).to.equal(false)
	end)
end
