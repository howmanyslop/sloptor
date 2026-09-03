-- Compiled with sloptor v2.3.3
return function()
	it("should support binary expressions on properties", function()
		local foo = {
			bar = 1,
		}
		foo.bar += 1
		expect(foo.bar).to.equal(2)
	end)
	it("should support binary assignment on properties", function()
		local foo = {
			bar = 1,
		}
		foo.bar += 1
		expect(foo.bar).to.equal(2)
	end)
	it("should support computedPropertyAccessExpressions requiring precedingStatements", function()
		local x = 0
		local arr = { 3, 4, 5 }
		local function f()
			x += 1
			return x
		end
		local _index = f() + 1
		x += 1
		arr[_index] *= x
		expect((arr[_index])).to.equal(8)
		expect(x).to.equal(2)
		local numCalls = 0
		local function g()
			local _object = {}
			local _left = 2
			numCalls += 1
			_object[_left] = numCalls
			return _object
		end
		local i = 2
		local _exp = g()
		local _original = i
		i += 1
		_exp[_original] *= i
		expect((_exp[_original])).to.equal(3)
		expect(numCalls).to.equal(1)
		i = 0
		local _object = {}
		i += 1
		local _left = i
		i += 1
		_object[_left] = i
		local _original_1 = i
		i += 1
		local _index_1 = _original_1 - 1
		i += 1
		_object[_index_1] *= i
		expect((_object[_index_1])).to.equal(8)
		do
			do
				do
					local i = 0
					local _object_1 = {}
					local _left_1 = "jk"
					local _object_2 = {
						o = 3,
					}
					local _left_2 = "b"
					local _original_2 = i
					i += 1
					_object_2[_left_2] = _original_2
					_object_2.a4 = {
						[2] = i,
						k = 4,
					}
					function _object_2:g()
						self.b += 1
					end
					_object_2.no = function(self)
						self[5] *= 7
						return self[5]
					end
					_object_2[5] = 3
					_object_1[_left_1] = _object_2
					expect(_object_1.jk:no()).to.equal(21)
					expect(({
						o = 8,
						a = function(self)
							self.o += 1
							return self
						end,
						e = function(self)
							self.o += 1
							return self.o
						end,
					}):a():e()).to.equal(10)
				end
			end
		end
	end)
	it("should push WritableOperandNames", function()
		local numItems = 0
		local _class
		do
			_class = setmetatable({}, {
				__tostring = function()
					return "Anonymous"
				end,
			})
			_class.__index = _class
			function _class.new(...)
				local self = setmetatable({}, _class)
				return self:constructor(...) or self
			end
			function _class:constructor()
				local _original = numItems
				numItems += 1
				self.id = _original
			end
		end
		_class.new().id += 1
		expect(numItems).to.equal(1)
	end)
	it("should support unary expressions on indexed parenthesized expressions", function()
		-- issue #470
		local array = { 0 }
		-- prettier-ignore
		array[1] += 1
		expect(array[1]).to.equal(1)
	end)
	it("should support unary expressions on indexed call expressions", function()
		local array = { 0 }
		local function getArray()
			return array
		end
		getArray()[1] += 1
		expect(array[1]).to.equal(1)
	end)
	it("should always put parentheses around unary expressions", function()
		local buttonSize = 10
		local a = -(48 + 1 * (buttonSize + 4) + 4)
		local b = -48 + 1 * (buttonSize + 4) + 4
		expect(a).to.equal(-66)
		expect(b).to.equal(-30)
	end)
	it("should support set expressions with parenthesis around the left-hand side", function()
		local x = 5
		-- prettier-ignore
		x *= 9
		expect(x).to.equal(45)
		local o = {
			x = 3,
		}
		-- prettier-ignore
		o.x = 5
		expect(o.x).to.equal(5)
		-- prettier-ignore
		o.x = 8
		expect(o.x).to.equal(8)
	end)
	it("should support comma operator", function()
		local x = 0
		local _exp = expect(x).to.equal(0)
		x = 1
		expect(x).to.equal(1)
		local _exp_1 = expect(x).to.equal(1)
		x = 3
		expect(x).to.equal(3)
		expect((x + 2)).to.equal(5)
		local function a()
			return 1, 2
		end
		x = 8
		local _ = x
		local b = ({ a() })
		expect(b[1]).to.equal(1)
		expect(b[2]).to.equal(2)
		expect(x).to.equal(8)
	end)
end
