-- Compiled with sloptor v2.3.1
return function()
	it("should understand string templates", function()
		local value = "hello"
		expect(`"{value} world"`).to.equal('"hello world"')
		expect(`"{value}" world`).to.equal('"hello" world')
		expect(`{value} "world"`).to.equal('hello "world"')
		expect(`a{"b"}c{"d"}e`).to.equal("abcde")
		expect(`1 {`2 {`3 {4}`}`}`).to.equal("1 2 3 4")
		-- prettier-ignore
		expect(`{value} \{\}`).to.equal("hello \{\}")
		-- prettier-ignore
		expect(`{value} \{world\}`).to.equal("hello \{world\}")
		expect(`{value}\nworld`).to.equal("hello\nworld")
		expect(`{value}\
world`).to.equal("hello\nworld")
		expect(function()
			return `{value} {({})}`
		end).to.never:throw()
		expect(#`{value} {0} {1}`).to.equal(9)
		local tableStrLength = #tostring({})
		expect(#`{value} {({})}`).to.equal(6 + tableStrLength)
		expect(#`{value} {({})}`).to.equal(6 + tableStrLength)
		expect(#`{value} {({})}`).to.equal(6 + tableStrLength)
		expect(#`{value} {({})}`).to.equal(6 + tableStrLength)
		local function returnsTuple()
			return "Osyris", "rimuy", "Dionysusnu"
		end
		expect(#`{value} {({ returnsTuple() })}`).to.equal(6 + tableStrLength)
	end)
	it("should support tagged TemplateExpression", function()
		local OPERATIONS = {
			["*"] = function(a, b)
				local _a = a
				local _b = b
				return _a * _b
			end,
			["/"] = function(a, b)
				local _a = a
				local _b = b
				return _a / _b
			end,
			["+"] = function(a, b)
				local _a = a
				local _b = b
				return _a + _b
			end,
			["-"] = function(a, b)
				local _a = a
				local _b = b
				return _a - _b
			end,
		}
		local function trim(s)
			return (string.match(s, "^%s*(.-)%s*$"))
		end
		local function m(strings, ...)
			local operands = { ... }
			-- ▼ ReadonlyArray.map ▼
			local _newValue = table.create(#strings)
			for _k, _v in strings do
				_newValue[_k] = trim(_v, _k - 1, strings)
			end
			-- ▲ ReadonlyArray.map ▲
			local operators = _newValue
			local value = table.remove(operands, 1)
			table.remove(operators, 1)
			for i = 0, #operands - 1 do
				local operator = trim(operators[i + 1])
				if OPERATIONS[operator] ~= nil then
					local operation = OPERATIONS[operator]
					value = operation(value, operands[i + 1])
				end
			end
			return value
		end
		local a = Vector3.new(1, 2, 3)
		local b = Vector3.new(4, 5, 6)
		local pos = m({ "", " * ", " - ", "" }, a, b, Vector3.new(1, 2, 3))
		expect(pos.X).to.equal(3)
		expect(pos.Y).to.equal(8)
		expect(pos.Z).to.equal(15)
	end)
	it("should support tagged NoSubstitutionTemplateLiteral", function()
		local function foo(strings)
			return "baz"
		end
		expect(foo({ "bar" })).to.equal("baz")
	end)
	it("should support functions which might return void", function()
		local function foo()
			if math.random() > 1 then
				-- impossible condition, math.random will always be 0-1
				-- but this generates an optional return type for TS
				return Instance.new("Model")
			end
		end
		tonumber((foo()))
		expect(`value = {foo()}`).to.equal("value = nil")
	end)
	it("should support defined expressions", function()
		local value = 123
		expect(`value = {value}`).to.equal("value = 123")
	end)
	it("should support unknown expressions", function()
		local value = 456
		expect(`value = {value}`).to.equal("value = 456")
	end)
	it("should not escape unicode sequences in template literals", function()
		expect(`\u{E001}`).to.equal("\u{E001}")
	end)
end
