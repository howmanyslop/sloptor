-- Compiled with sloptor v2.3.2
return function()
	it("should support static parameter decorators", function()
		local buzz
		local function foobar(object, methodKey, paramNum)
			buzz = `{object}{methodKey}{paramNum}`
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
			end
			function Foo:bar(baz)
			end
			foobar(Foo, "bar", 0)
		end
		expect(buzz).to.equal("Foobar0")
	end)
	it("should support non-static parameter decorators", function()
		local buzz
		local function foobar(object, methodKey, paramNum)
			buzz = `{object}{methodKey}{paramNum}`
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
			end
			function Foo:bar(baz)
			end
			foobar(Foo, "bar", 0)
		end
		expect(buzz).to.equal("Foobar0")
	end)
	it("should support class decorators", function()
		local buzz
		local function foobar(object)
			buzz = object
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
			end
			Foo = foobar(Foo) or Foo
		end
		expect(buzz).to.equal(Foo)
	end)
	it("should support static method decorators", function()
		local buzz
		local function foobar(object, methodKey)
			buzz = `{object}{methodKey}`
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
			end
			function Foo:bar()
			end
			local _descriptor = foobar(Foo, "bar", {
				value = Foo.bar,
			})
			if _descriptor then
				Foo.bar = _descriptor.value
			end
		end
		expect(buzz).to.equal("Foobar")
	end)
	it("should support non-static method decorators", function()
		local buzz
		local function foobar(object, methodKey)
			buzz = `{tostring(object)}{methodKey}`
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
			end
			function Foo:bar()
			end
			local _descriptor = foobar(Foo, "bar", {
				value = Foo.bar,
			})
			if _descriptor then
				Foo.bar = _descriptor.value
			end
		end
		expect(buzz).to.equal("Foobar")
	end)
	it("should support computed method decorators", function()
		local barName = "bar"
		local buzz
		local function foobar(object, methodKey)
			buzz = `{tostring(object)}{methodKey}`
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
			end
			local _key = barName
			Foo[_key] = function(self) end
			local _descriptor = foobar(Foo, _key, {
				value = Foo[_key],
			})
			if _descriptor then
				Foo[_key] = _descriptor.value
			end
		end
		expect(buzz).to.equal("Foobar")
	end)
	it("should support static property decorators", function()
		local buzz
		local function foobar(object, propertyKey)
			buzz = `{object}{propertyKey}`
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
			end
			Foo.bar = "baz"
			foobar(Foo, "bar")
		end
		expect(buzz).to.equal(`Foobar`)
	end)
	it("should support non-static property decorators", function()
		local buzz
		local function foobar(object, propertyKey)
			buzz = `{tostring(object)}{propertyKey}`
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
				self.bar = "baz"
			end
			foobar(Foo, "bar")
		end
		expect(buzz).to.equal(`Foobar`)
	end)
	it("should support computed property decorators", function()
		local barName = "bar"
		local buzz
		local function foobar(object, propertyKey)
			buzz = `{tostring(object)}{propertyKey}`
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
				self[barName] = "baz"
			end
			foobar(Foo, barName)
		end
		expect(buzz).to.equal(`Foobar`)
	end)
	it("should support constructor parameter decorators", function()
		local buzz
		local function foobar(object, _, paramNum)
			buzz = `{tostring(object)}{paramNum}`
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor(bar)
				if bar == nil then
					bar = "baz"
				end
			end
			foobar(Foo, nil, 0)
		end
		expect(buzz).to.equal(`Foo0`)
	end)
	it("should support complex property decorators", function()
		local baz = {
			buzz = "bizz",
		}
		local bizz
		local function foobar(object, propertyKey)
			bizz = `{tostring(object)}{propertyKey}`
		end
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
				self[baz.buzz] = "Hello, world!"
			end
			foobar(Foo, baz.buzz)
		end
		expect(bizz).to.equal("Foobizz")
	end)
	it("should support complex decorator ordering", function()
		local events = {}
		local function makeDecorator(name)
			return function(n)
				local initEvent = {
					tag = "",
					state = "initialized",
				}
				table.insert(events, initEvent)
				return function(...)
					local args = { ... }
					-- ▼ ReadonlyArray.filterUndefined ▼
					local _length = 0
					for _i in args do
						if _i > _length then
							_length = _i
						end
					end
					local _result = {}
					local _resultLength = 0
					for _i = 1, _length do
						local _v = args[_i]
						if _v ~= nil then
							_resultLength += 1
							_result[_resultLength] = _v
						end
					end
					-- ▲ ReadonlyArray.filterUndefined ▲
					-- ▼ ReadonlyArray.filter ▼
					local _newValue = {}
					local _callback = function(v)
						local _v = v
						local _condition = type(_v) == "table"
						if _condition then
							_condition = v.value ~= nil
						end
						return not _condition
					end
					local _length_1 = 0
					for _k, _v in _result do
						if _callback(_v, _k - 1, _result) == true then
							_length_1 += 1
							_newValue[_length_1] = _v
						end
					end
					-- ▲ ReadonlyArray.filter ▲
					-- ▼ ReadonlyArray.map ▼
					local _newValue_1 = table.create(#_newValue)
					for _k, _v in _newValue do
						_newValue_1[_k] = tostring(_v, _k - 1, _newValue)
					end
					-- ▲ ReadonlyArray.map ▲
					local argStr = table.concat(_newValue_1, "_")
					local tag = `{name}_{argStr}_{n}`
					initEvent.tag = tag
					local _arg0 = {
						tag = tag,
						state = "executed",
					}
					table.insert(events, _arg0)
				end
			end
		end
		local classDecorator = makeDecorator("class")
		local methodDecorator = makeDecorator("method")
		local staticMethodDecorator = makeDecorator("staticMethod")
		local propertyDecorator = makeDecorator("property")
		local staticPropertyDecorator = makeDecorator("staticProperty")
		local parameterDecorator = makeDecorator("parameter")
		local staticParameterDecorator = makeDecorator("staticParameter")
		local constructorParameterDecorator = makeDecorator("constructorParameter")
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor(param1, param2)
				self.property = "value"
			end
			function Foo:method()
			end
			function Foo:staticMethod()
			end
			function Foo:methodWithParameters(param1, param2)
			end
			function Foo:staticMethodWithParameters(param1, param2)
			end
			Foo.staticProperty = "value"
			local _decorator = methodDecorator(1)
			local _decorator_1 = methodDecorator(2)
			local _descriptor = methodDecorator(3)(Foo, "method", {
				value = Foo.method,
			})
			if _descriptor then
				Foo.method = _descriptor.value
			end
			local _descriptor_1 = _decorator_1(Foo, "method", {
				value = Foo.method,
			})
			if _descriptor_1 then
				Foo.method = _descriptor_1.value
			end
			local _descriptor_2 = _decorator(Foo, "method", {
				value = Foo.method,
			})
			if _descriptor_2 then
				Foo.method = _descriptor_2.value
			end
			local _decorator_2 = propertyDecorator(1)
			local _decorator_3 = propertyDecorator(2)
			propertyDecorator(3)(Foo, "property")
			_decorator_3(Foo, "property")
			_decorator_2(Foo, "property")
			local _decorator_4 = methodDecorator(1)
			local _decorator_5 = methodDecorator(2)
			local _decorator_6 = methodDecorator(3)
			local _decorator_7 = parameterDecorator(1)
			local _decorator_8 = parameterDecorator(2)
			local _decorator_9 = parameterDecorator(3)
			local _decorator_10 = parameterDecorator(4)
			local _decorator_11 = parameterDecorator(5)
			parameterDecorator(6)(Foo, "methodWithParameters", 1)
			_decorator_11(Foo, "methodWithParameters", 1)
			_decorator_10(Foo, "methodWithParameters", 1)
			_decorator_9(Foo, "methodWithParameters", 0)
			_decorator_8(Foo, "methodWithParameters", 0)
			_decorator_7(Foo, "methodWithParameters", 0)
			local _descriptor_3 = _decorator_6(Foo, "methodWithParameters", {
				value = Foo.methodWithParameters,
			})
			if _descriptor_3 then
				Foo.methodWithParameters = _descriptor_3.value
			end
			local _descriptor_4 = _decorator_5(Foo, "methodWithParameters", {
				value = Foo.methodWithParameters,
			})
			if _descriptor_4 then
				Foo.methodWithParameters = _descriptor_4.value
			end
			local _descriptor_5 = _decorator_4(Foo, "methodWithParameters", {
				value = Foo.methodWithParameters,
			})
			if _descriptor_5 then
				Foo.methodWithParameters = _descriptor_5.value
			end
			local _decorator_12 = staticMethodDecorator(1)
			local _decorator_13 = staticMethodDecorator(2)
			local _descriptor_6 = staticMethodDecorator(3)(Foo, "staticMethod", {
				value = Foo.staticMethod,
			})
			if _descriptor_6 then
				Foo.staticMethod = _descriptor_6.value
			end
			local _descriptor_7 = _decorator_13(Foo, "staticMethod", {
				value = Foo.staticMethod,
			})
			if _descriptor_7 then
				Foo.staticMethod = _descriptor_7.value
			end
			local _descriptor_8 = _decorator_12(Foo, "staticMethod", {
				value = Foo.staticMethod,
			})
			if _descriptor_8 then
				Foo.staticMethod = _descriptor_8.value
			end
			local _decorator_14 = staticPropertyDecorator(1)
			local _decorator_15 = staticPropertyDecorator(2)
			staticPropertyDecorator(3)(Foo, "staticProperty")
			_decorator_15(Foo, "staticProperty")
			_decorator_14(Foo, "staticProperty")
			local _decorator_16 = staticMethodDecorator(1)
			local _decorator_17 = staticMethodDecorator(2)
			local _decorator_18 = staticMethodDecorator(3)
			local _decorator_19 = staticParameterDecorator(1)
			local _decorator_20 = staticParameterDecorator(2)
			local _decorator_21 = staticParameterDecorator(3)
			local _decorator_22 = staticParameterDecorator(4)
			local _decorator_23 = staticParameterDecorator(5)
			staticParameterDecorator(6)(Foo, "staticMethodWithParameters", 1)
			_decorator_23(Foo, "staticMethodWithParameters", 1)
			_decorator_22(Foo, "staticMethodWithParameters", 1)
			_decorator_21(Foo, "staticMethodWithParameters", 0)
			_decorator_20(Foo, "staticMethodWithParameters", 0)
			_decorator_19(Foo, "staticMethodWithParameters", 0)
			local _descriptor_9 = _decorator_18(Foo, "staticMethodWithParameters", {
				value = Foo.staticMethodWithParameters,
			})
			if _descriptor_9 then
				Foo.staticMethodWithParameters = _descriptor_9.value
			end
			local _descriptor_10 = _decorator_17(Foo, "staticMethodWithParameters", {
				value = Foo.staticMethodWithParameters,
			})
			if _descriptor_10 then
				Foo.staticMethodWithParameters = _descriptor_10.value
			end
			local _descriptor_11 = _decorator_16(Foo, "staticMethodWithParameters", {
				value = Foo.staticMethodWithParameters,
			})
			if _descriptor_11 then
				Foo.staticMethodWithParameters = _descriptor_11.value
			end
			local _decorator_24 = classDecorator(1)
			local _decorator_25 = classDecorator(2)
			local _decorator_26 = classDecorator(3)
			local _decorator_27 = constructorParameterDecorator(1)
			local _decorator_28 = constructorParameterDecorator(2)
			local _decorator_29 = constructorParameterDecorator(3)
			local _decorator_30 = constructorParameterDecorator(4)
			local _decorator_31 = constructorParameterDecorator(5)
			constructorParameterDecorator(6)(Foo, nil, 1)
			_decorator_31(Foo, nil, 1)
			_decorator_30(Foo, nil, 1)
			_decorator_29(Foo, nil, 0)
			_decorator_28(Foo, nil, 0)
			_decorator_27(Foo, nil, 0)
			Foo = _decorator_26(Foo) or Foo
			Foo = _decorator_25(Foo) or Foo
			Foo = _decorator_24(Foo) or Foo
		end
		local EXPECTED_EVENT_ORDER = { "method_Foo_method_1_initialized", "method_Foo_method_2_initialized", "method_Foo_method_3_initialized", "method_Foo_method_3_executed", "method_Foo_method_2_executed", "method_Foo_method_1_executed", "property_Foo_property_1_initialized", "property_Foo_property_2_initialized", "property_Foo_property_3_initialized", "property_Foo_property_3_executed", "property_Foo_property_2_executed", "property_Foo_property_1_executed", "method_Foo_methodWithParameters_1_initialized", "method_Foo_methodWithParameters_2_initialized", "method_Foo_methodWithParameters_3_initialized", "parameter_Foo_methodWithParameters_0_1_initialized", "parameter_Foo_methodWithParameters_0_2_initialized", "parameter_Foo_methodWithParameters_0_3_initialized", "parameter_Foo_methodWithParameters_1_4_initialized", "parameter_Foo_methodWithParameters_1_5_initialized", "parameter_Foo_methodWithParameters_1_6_initialized", "parameter_Foo_methodWithParameters_1_6_executed", "parameter_Foo_methodWithParameters_1_5_executed", "parameter_Foo_methodWithParameters_1_4_executed", "parameter_Foo_methodWithParameters_0_3_executed", "parameter_Foo_methodWithParameters_0_2_executed", "parameter_Foo_methodWithParameters_0_1_executed", "method_Foo_methodWithParameters_3_executed", "method_Foo_methodWithParameters_2_executed", "method_Foo_methodWithParameters_1_executed", "staticMethod_Foo_staticMethod_1_initialized", "staticMethod_Foo_staticMethod_2_initialized", "staticMethod_Foo_staticMethod_3_initialized", "staticMethod_Foo_staticMethod_3_executed", "staticMethod_Foo_staticMethod_2_executed", "staticMethod_Foo_staticMethod_1_executed", "staticProperty_Foo_staticProperty_1_initialized", "staticProperty_Foo_staticProperty_2_initialized", "staticProperty_Foo_staticProperty_3_initialized", "staticProperty_Foo_staticProperty_3_executed", "staticProperty_Foo_staticProperty_2_executed", "staticProperty_Foo_staticProperty_1_executed", "staticMethod_Foo_staticMethodWithParameters_1_initialized", "staticMethod_Foo_staticMethodWithParameters_2_initialized", "staticMethod_Foo_staticMethodWithParameters_3_initialized", "staticParameter_Foo_staticMethodWithParameters_0_1_initialized", "staticParameter_Foo_staticMethodWithParameters_0_2_initialized", "staticParameter_Foo_staticMethodWithParameters_0_3_initialized", "staticParameter_Foo_staticMethodWithParameters_1_4_initialized", "staticParameter_Foo_staticMethodWithParameters_1_5_initialized", "staticParameter_Foo_staticMethodWithParameters_1_6_initialized", "staticParameter_Foo_staticMethodWithParameters_1_6_executed", "staticParameter_Foo_staticMethodWithParameters_1_5_executed", "staticParameter_Foo_staticMethodWithParameters_1_4_executed", "staticParameter_Foo_staticMethodWithParameters_0_3_executed", "staticParameter_Foo_staticMethodWithParameters_0_2_executed", "staticParameter_Foo_staticMethodWithParameters_0_1_executed", "staticMethod_Foo_staticMethodWithParameters_3_executed", "staticMethod_Foo_staticMethodWithParameters_2_executed", "staticMethod_Foo_staticMethodWithParameters_1_executed", "class_Foo_1_initialized", "class_Foo_2_initialized", "class_Foo_3_initialized", "constructorParameter_Foo_0_1_initialized", "constructorParameter_Foo_0_2_initialized", "constructorParameter_Foo_0_3_initialized", "constructorParameter_Foo_1_4_initialized", "constructorParameter_Foo_1_5_initialized", "constructorParameter_Foo_1_6_initialized", "constructorParameter_Foo_1_6_executed", "constructorParameter_Foo_1_5_executed", "constructorParameter_Foo_1_4_executed", "constructorParameter_Foo_0_3_executed", "constructorParameter_Foo_0_2_executed", "constructorParameter_Foo_0_1_executed", "class_Foo_3_executed", "class_Foo_2_executed", "class_Foo_1_executed" }
		for i = 0, #EXPECTED_EVENT_ORDER - 1 do
			local _binding = events[i + 1]
			local tag = _binding.tag
			local state = _binding.state
			expect(`{tag}_{state}`).to.equal(EXPECTED_EVENT_ORDER[i + 1])
		end
	end)
end
