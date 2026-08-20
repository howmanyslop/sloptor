-- Compiled with sloptor v2.3.2
return function()
	it("should support JS truthiness", function()
		local function isTruthy(x)
			local y = not not (x ~= 0 and x == x and x ~= "" and x)
			-- make sure that the expression is equivalent to an if statement
			if not not (x ~= 0 and x == x and x ~= "" and x) then
				expect(y).to.equal(true)
			else
				expect(y).to.equal(false)
			end
			return y
		end
		expect(isTruthy()).to.equal(false)
		expect(isTruthy(nil)).to.equal(false)
		expect(isTruthy("")).to.equal(false)
		expect(isTruthy("Hello")).to.equal(true)
		expect(isTruthy(0)).to.equal(false)
		expect(isTruthy(1)).to.equal(true)
		expect(isTruthy(0 / 0)).to.equal(false)
		expect(isTruthy({})).to.equal(true)
		expect(isTruthy({ 1 })).to.equal(true)
		expect(isTruthy({})).to.equal(true)
		expect(isTruthy({
			a = 0,
		})).to.equal(true)
	end)
	it("should support truthiness in && expressions", function()
		local i = 0
		local function f(s)
			local _condition = s
			if _condition ~= "" and _condition then
				_condition = "Go"
			end
			local x = _condition
			local y = if s == "" then "" else "Go"
			expect(x).to.equal(y)
			local _condition_1 = s
			if _condition_1 ~= "" and _condition_1 then
				_condition_1 = "Go"
			end
			expect(_condition_1).to.equal(y)
			local _condition_2 = s
			if _condition_2 ~= "" and _condition_2 then
				local _original = i
				i += 1
				_condition_2 = _original
			end
			if _condition_2 ~= 0 and _condition_2 == _condition_2 and _condition_2 ~= "" and _condition_2 then
				expect(s).to.equal("thing")
				expect(i).to.equal(2)
			end
		end
		f("")
		f("stuff")
		f("thing")
	end)
	it("should support truthiness in || expressions", function()
		local i = 0
		local function f(s)
			local _condition = s
			if not (_condition ~= "" and _condition) then
				_condition = "default"
			end
			local x = _condition
			local y = if s == "" then "default" else s
			expect(x).to.equal(y)
			local _condition_1 = s
			if not (_condition_1 ~= "" and _condition_1) then
				_condition_1 = "default"
			end
			expect(_condition_1).to.equal(y)
			local _condition_2 = s
			if not (_condition_2 ~= "" and _condition_2) then
				local _original = i
				i += 1
				_condition_2 = _original
			end
			if _condition_2 ~= 0 and _condition_2 == _condition_2 and _condition_2 ~= "" and _condition_2 then
				expect(s).to.equal("stuff")
				expect(i).to.equal(1)
			end
		end
		f("")
		f("stuff")
	end)
	it("should support binary expressions", function()
		local function test(x, answers)
			local i = -1
			local _condition = x
			if _condition ~= "" and _condition then
				_condition = x == "Soup"
			end
			local _to = expect(_condition).to
			local _exp = answers
			i += 1
			_to.equal(_exp[i + 1])
			local _condition_1 = x
			if not (_condition_1 ~= "" and _condition_1) then
				_condition_1 = x == "Soup"
			end
			local _to_1 = expect(_condition_1).to
			local _exp_1 = answers
			i += 1
			_to_1.equal(_exp_1[i + 1])
			local _to_2 = expect(not (x ~= "" and x) and x == "Soup").to
			local _exp_2 = answers
			i += 1
			_to_2.equal(_exp_2[i + 1])
			local _to_3 = expect(not (x ~= "" and x) or x == "Soup").to
			local _exp_3 = answers
			i += 1
			_to_3.equal(_exp_3[i + 1])
			local _to_4 = expect(not not (x ~= "" and x) and x == "Soup").to
			local _exp_4 = answers
			i += 1
			_to_4.equal(_exp_4[i + 1])
			local _to_5 = expect(not not (x ~= "" and x) or x == "Soup").to
			local _exp_5 = answers
			i += 1
			_to_5.equal(_exp_5[i + 1])
		end
		test(nil, { nil, false, false, true, false, false })
		test("", { "", false, false, true, false, false })
		test("a", { false, "a", false, false, false, true })
		test("Soup", { true, "Soup", false, true, true, true })
	end)
	it("should support returning binary expressions", function()
		local function f(s)
			local _condition = s
			if not (_condition ~= "" and _condition) then
				_condition = "default"
			end
			return _condition
		end
		local function g(s)
			local _condition = s
			if _condition ~= "" and _condition then
				_condition = "default"
			end
			return _condition
		end
		expect(f("")).to.equal("default")
		expect(f("a")).to.equal("a")
		expect(g("")).to.equal("")
		expect(g("a")).to.equal("default")
	end)
end
