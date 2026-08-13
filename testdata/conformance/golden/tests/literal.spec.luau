-- Compiled with sloptor v2.3.0
return function()
	it("should understand number literals", function()
		expect(1).to.equal(1)
		expect(6).to.equal(6)
		expect(0xf00d).to.equal(61453)
		expect(0b1010).to.equal(10)
		expect(484).to.equal(484)
		-- issue #213
		expect(0.0000000000001).never.to.equal(1)
		expect(100000000000000).never.to.equal(1)
		-- We should compare numbers, not strings
		expect(tonumber(tostring(0.0000000000001))).to.equal(tonumber("1e-13"))
		expect(tonumber(tostring(100000000000000))).to.equal(tonumber("1e+14"))
		expect(1_0_0_0_0).to.equal(10000)
	end)
	it("should add numbers", function()
		expect(1 + 1).to.equal(2)
		local a = 1
		local b = 1
		expect(a + b).to.equal(2)
	end)
	-- prettier-ignore
	it("should understand string literals", function()
		expect("foo").to.equal("foo")
		expect("foo").to.equal("foo")
		expect(`foo`).to.equal("foo")
		expect(#"foo").to.equal(3)
		expect(#"foo").to.equal(3)
		expect(#`foo`).to.equal(3)
		expect('\"').to.equal('\"')
		expect(`\"`).to.equal('\"')
		expect('\"').to.equal('\"')
		expect(`"`).to.equal('\"')
		expect('"').to.equal('\"')
	end)
	it("should add strings", function()
		expect("a" .. "b").to.equal("ab")
		local a = "a"
		local b = "b"
		expect(a .. b).to.equal("ab")
	end)
	it("should add numbers and strings", function()
		local a = "2"
		local b = 1
		expect(a .. tostring(b)).to.equal("21")
		expect(tostring(b) .. a).to.equal("12")
	end)
	it("should support ===", function()
		local a = (function()
			return false
		end)()
		local b = (function()
			return true
		end)()
		expect(a == a).to.equal(true)
		expect(a == b).to.equal(false)
		expect(b == b).to.equal(true)
		expect(b == a).to.equal(false)
	end)
	it("should support !==", function()
		local a = (function()
			return false
		end)()
		local b = (function()
			return true
		end)()
		expect(a ~= a).to.equal(false)
		expect(a ~= b).to.equal(true)
		expect(b ~= b).to.equal(false)
		expect(b ~= a).to.equal(true)
	end)
	it("should support &&", function()
		local a = false
		local b = true
		expect(a and a).to.equal(false)
		expect(a and b).to.equal(false)
		expect(b and b).to.equal(true)
		expect(b and a).to.equal(false)
	end)
	it("should support ||", function()
		local a = false
		local b = true
		expect(a or a).to.equal(false)
		expect(a or b).to.equal(true)
		expect(b or b).to.equal(true)
		expect(b or a).to.equal(true)
	end)
	it("should support `in`", function()
		local foo = {
			a = 1,
		}
		expect(foo.a ~= nil).to.equal(true)
		expect(foo.b ~= nil).to.equal(false)
	end)
	it("should support <", function()
		expect(1 < 2).to.equal(true)
		expect(2 < 1).to.equal(false)
		expect(2 < 2).to.equal(false)
	end)
	it("should support >", function()
		expect(1 > 2).to.equal(false)
		expect(2 > 1).to.equal(true)
		expect(2 > 2).to.equal(false)
	end)
	it("should support <=", function()
		expect(1 <= 2).to.equal(true)
		expect(2 <= 1).to.equal(false)
		expect(2 <= 2).to.equal(true)
	end)
	it("should support >=", function()
		expect(1 >= 2).to.equal(false)
		expect(2 >= 1).to.equal(true)
		expect(2 >= 2).to.equal(true)
	end)
	it("should support !", function()
		expect(not false).to.equal(true)
		expect(not true).to.equal(false)
	end)
	it("should allow postfix operators on properties", function()
		local obj = {
			x = 0,
		}
		expect(obj.x).to.equal(0)
		obj.x += 1
		expect(obj.x).to.equal(1)
		obj.x -= 1
		expect(obj.x).to.equal(0)
		local _original = obj.x
		obj.x += 1
		expect(_original).to.equal(0)
		expect(obj.x).to.equal(1)
		local _original_1 = obj.x
		obj.x -= 1
		expect(_original_1).to.equal(1)
		expect(obj.x).to.equal(0)
	end)
end
