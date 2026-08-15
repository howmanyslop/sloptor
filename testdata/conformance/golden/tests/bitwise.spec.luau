-- Compiled with sloptor v2.3.1
return function()
	it("should support bitwise operations", function()
		local a = 0b101
		local b = 0b110
		expect(bit32.bor(a, b)).to.equal(0b111)
		expect(bit32.band(a, b)).to.equal(0b100)
		expect(bit32.bxor(a, b)).to.equal(0b011)
		expect(bit32.lshift(a, 1)).to.equal(0b1010)
		expect(bit32.arshift(a, 1)).to.equal(0b10)
		expect(bit32.rshift(a, 1)).to.equal(0b10)
		expect(bit32.bor(a, 0)).to.equal(0b101)
		expect(bit32.bnot(a)).to.equal(4294967290)
		expect(bit32.bnot(b)).to.equal(4294967289)
	end)
	it("should support bitwise assignment", function()
		local a = 0b101
		a = bit32.bor(a, 0b110)
		expect(a).to.equal(0b111)
		local b = 0b101
		b = bit32.band(b, 0b110)
		expect(b).to.equal(0b100)
		local c = 0b101
		c = bit32.bxor(c, 0b110)
		expect(c).to.equal(0b011)
		local d = 0b101
		d = bit32.lshift(d, 1)
		expect(d).to.equal(0b1010)
		local e = 0b101
		e = bit32.arshift(e, 1)
		expect(e).to.equal(0b10)
		local f = -1
		f = bit32.rshift(f, 0)
		expect(f).to.equal(math.pow(2, 32) - 1)
	end)
	it("should support bitwise assignment expressions", function()
		local a = 0b101
		a = bit32.bor(a, 0b110)
		expect(a).to.equal(0b111)
		local b = 0b101
		b = bit32.band(b, 0b110)
		expect(b).to.equal(0b100)
		local c = 0b101
		c = bit32.bxor(c, 0b110)
		expect(c).to.equal(0b011)
		local d = 0b101
		d = bit32.lshift(d, 1)
		expect(d).to.equal(0b1010)
		local e = 0b101
		e = bit32.arshift(e, 1)
		expect(e).to.equal(0b10)
		local f = -1
		f = bit32.rshift(f, 0)
		expect(f).to.equal(math.pow(2, 32) - 1)
	end)
	it("should support bitwise with negative numbers", function()
		local x = -5
		x = bit32.arshift(x, 1)
		expect(x).to.equal(4294967293)
	end)
end
