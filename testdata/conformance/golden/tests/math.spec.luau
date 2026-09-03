-- Compiled with sloptor v2.4.0
return function()
	describe("should support basic math operators", function()
		it("should add numbers", function()
			expect(1 + 5).to.equal(6)
			expect(22 + 44).to.equal(66)
		end)
		it("should subtract numbers", function()
			expect(10 - 3).to.equal(7)
			expect(20 - 37).to.equal(-17)
		end)
		it("should multiply numbers", function()
			expect(5 * 7).to.equal(35)
			expect(8 * 12).to.equal(96)
		end)
		it("should divide numbers", function()
			expect(6 / 3).to.equal(2)
			expect(22 / 44).to.equal(0.5)
		end)
		it("should modulus numbers", function()
			expect(5 % 2).to.equal(1)
			expect(100 % 17).to.equal(15)
		end)
		it("should exponent numbers", function()
			expect(2 ^ 3).to.equal(8)
			expect(4 ^ 5).to.equal(1024)
		end)
	end)
	describe("should support math unary operators", function()
		it("should post increment", function()
			local x = 10
			x += 1
			expect(x).to.equal(11)
			local _original = x
			x += 1
			expect(_original).to.equal(11)
			expect(x).to.equal(12)
		end)
		it("should pre increment", function()
			local x = 10
			x += 1
			expect(x).to.equal(11)
			x += 1
			expect(x).to.equal(12)
			expect(x).to.equal(12)
		end)
		it("should post decrement", function()
			local x = 10
			x -= 1
			expect(x).to.equal(9)
			local _original = x
			x -= 1
			expect(_original).to.equal(9)
			expect(x).to.equal(8)
		end)
		it("should pre decrement", function()
			local x = 10
			x -= 1
			expect(x).to.equal(9)
			x -= 1
			expect(x).to.equal(8)
			expect(x).to.equal(8)
		end)
	end)
	describe("should support compound assignment", function()
		it("should compound assign addition", function()
			local x = 10
			x += 2
			expect(x).to.equal(12)
			x += 5
			expect(x).to.equal(17)
		end)
		it("should compound assign subtraction", function()
			local x = 10
			x -= 2
			expect(x).to.equal(8)
			x -= 5
			expect(x).to.equal(3)
		end)
		it("should compound assign multiplication", function()
			local x = 10
			x *= 2
			expect(x).to.equal(20)
			x *= 5
			expect(x).to.equal(100)
		end)
		it("should compound assign division", function()
			local x = 10
			x /= 2
			expect(x).to.equal(5)
			x /= 5
			expect(x).to.equal(1)
		end)
		it("should compound assign modulus", function()
			local x = 10
			x %= 2
			expect(x).to.equal(0)
			x %= 5
			expect(x).to.equal(0)
		end)
		it("should compound assign exponents", function()
			local x = 10
			x ^= 2
			expect(x).to.equal(100)
			x ^= 5
			expect(x).to.equal(100e8)
		end)
	end)
end
