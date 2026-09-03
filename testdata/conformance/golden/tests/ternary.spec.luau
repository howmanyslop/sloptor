-- Compiled with sloptor v2.4.0
return function()
	it("should support ternary expressions", function()
		expect(if true then 1 else 0).to.equal(1)
		expect(if false then 1 else 0).to.equal(0)
		expect(if true then false else true).to.equal(false)
		expect(if false then false else true).to.equal(true)
	end)
	it("should support JS truthiness", function()
		local value = 123
		local zero = 0
		local emptyStr = ""
		local NaN = 0 / 0
		expect(if value ~= 0 and value == value and value then "PASS" else "FAIL").to.equal("PASS")
		expect(if zero ~= 0 and zero == zero and zero then "PASS" else "FAIL").to.equal("FAIL")
		expect(if emptyStr ~= "" and emptyStr then "PASS" else "FAIL").to.equal("FAIL")
		expect(if NaN ~= 0 and NaN == NaN and NaN then "PASS" else "FAIL").to.equal("FAIL")
	end)
	it("should support JS truthiness with prereqs", function()
		local value = 123
		local zero = 0
		local emptyStr = ""
		local NaN = 0 / 0
		local strA = "A"
		local strB = ""
		local _result
		if value ~= 0 and value == value and value then
			strA ..= "B"
			_result = strA
		else
			_result = "FAIL"
		end
		expect(_result).to.equal("AB")
		local _result_1
		if zero ~= 0 and zero == zero and zero then
			_result_1 = "FAIL"
		else
			strB ..= "X"
			_result_1 = strB
		end
		expect(_result_1).to.equal("X")
		local _result_2
		if emptyStr ~= "" and emptyStr then
			_result_2 = "FAIL"
		else
			strB ..= "Y"
			_result_2 = strB
		end
		expect(_result_2).to.equal("XY")
		local _result_3
		if NaN ~= 0 and NaN == NaN and NaN then
			_result_3 = "FAIL"
		else
			strB ..= "Z"
			_result_3 = strB
		end
		expect(_result_3).to.equal("XYZ")
	end)
	it("should correctly wrap if-expressions in parentheses where needed", function()
		local function getColorStr(on)
			return `{if on then "on" else "off"}Color`
		end
		expect(getColorStr(true)).to.equal("onColor")
		expect(getColorStr(false)).to.equal("offColor")
	end)
end
