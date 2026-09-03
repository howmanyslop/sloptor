-- Compiled with sloptor v2.3.3
local function Component(ctor)
	print("Component", ctor)
end
local function ComponentFactory(arg)
	return function(ctor)
		print("ComponentFactory", arg, ctor)
	end
end
local function LogProp(target, key)
	print("LogProp", target, key)
end
local function LogPropFactory(tag)
	return function(target, key)
		print("LogPropFactory", tag, target, key)
	end
end
local function LogMethod(target, key, descriptor)
	print("LogMethod", target, key, descriptor)
end
local function LogMethodFactory(tag)
	return function(target, key, descriptor)
		print("LogMethodFactory", tag, target, key, descriptor)
	end
end
local function LogParam(target, key, index)
	print("LogParam", target, key, index)
end
local mutableDeco = function(ctor)
	print("mutable", ctor)
end
local ns = {
	deco = function(target, key)
		print("ns.deco", target, key)
	end,
}
local Decorated
do
	Decorated = setmetatable({}, {
		__tostring = function()
			return "Decorated"
		end,
	})
	Decorated.__index = Decorated
	function Decorated.new(...)
		local self = setmetatable({}, Decorated)
		return self:constructor(...) or self
	end
	function Decorated:constructor(x)
		self.value = 1
		print(x)
	end
	function Decorated:method(a, b)
		print(a, b)
	end
	function Decorated:smethod()
	end
	Decorated.svalue = 2
	LogProp(Decorated, "value")
	LogParam(Decorated, "method", 1)
	LogParam(Decorated, "method", 0)
	local _descriptor = LogMethod(Decorated, "method", {
		value = Decorated.method,
	})
	if _descriptor then
		Decorated.method = _descriptor.value
	end
	LogProp(Decorated, "svalue")
	local _descriptor_1 = LogMethod(Decorated, "smethod", {
		value = Decorated.smethod,
	})
	if _descriptor_1 then
		Decorated.smethod = _descriptor_1.value
	end
	local _decorator = ComponentFactory("hi")
	LogParam(Decorated, nil, 0)
	Decorated = _decorator(Decorated) or Decorated
	Decorated = Component(Decorated) or Decorated
end
local MutableDecorated
do
	MutableDecorated = setmetatable({}, {
		__tostring = function()
			return "MutableDecorated"
		end,
	})
	MutableDecorated.__index = MutableDecorated
	function MutableDecorated.new(...)
		local self = setmetatable({}, MutableDecorated)
		return self:constructor(...) or self
	end
	function MutableDecorated:constructor()
	end
	local _decorator = mutableDeco
	MutableDecorated = Component(MutableDecorated) or MutableDecorated
	MutableDecorated = _decorator(MutableDecorated) or MutableDecorated
end
local KEY = "dyn"
local ComputedDecorated
do
	ComputedDecorated = setmetatable({}, {
		__tostring = function()
			return "ComputedDecorated"
		end,
	})
	ComputedDecorated.__index = ComputedDecorated
	function ComputedDecorated.new(...)
		local self = setmetatable({}, ComputedDecorated)
		return self:constructor(...) or self
	end
	function ComputedDecorated:constructor()
	end
	local _key = KEY
	ComputedDecorated[_key] = function(self) end
	local _descriptor = LogMethod(ComputedDecorated, _key, {
		value = ComputedDecorated[_key],
	})
	if _descriptor then
		ComputedDecorated[_key] = _descriptor.value
	end
end
local Factories
do
	Factories = setmetatable({}, {
		__tostring = function()
			return "Factories"
		end,
	})
	Factories.__index = Factories
	function Factories.new(...)
		local self = setmetatable({}, Factories)
		return self:constructor(...) or self
	end
	function Factories:constructor()
		self.prop = 3
	end
	function Factories:multi()
	end
	local _decorator = LogPropFactory("p")
	ns.deco(Factories, "prop")
	_decorator(Factories, "prop")
	local _decorator_1 = LogMethodFactory("m")
	local _descriptor = LogMethod(Factories, "multi", {
		value = Factories.multi,
	})
	if _descriptor then
		Factories.multi = _descriptor.value
	end
	local _descriptor_1 = _decorator_1(Factories, "multi", {
		value = Factories.multi,
	})
	if _descriptor_1 then
		Factories.multi = _descriptor_1.value
	end
end
local Base
do
	Base = setmetatable({}, {
		__tostring = function()
			return "Base"
		end,
	})
	Base.__index = Base
	function Base.new(...)
		local self = setmetatable({}, Base)
		return self:constructor(...) or self
	end
	function Base:constructor()
	end
	function Base:greet()
		return "base"
	end
end
local ErrorBoundary
do
	local super = Base
	ErrorBoundary = setmetatable({}, {
		__tostring = function()
			return "ErrorBoundary"
		end,
		__index = super,
	})
	ErrorBoundary.__index = ErrorBoundary
	function ErrorBoundary.new(...)
		local self = setmetatable({}, ErrorBoundary)
		return self:constructor(...) or self
	end
	function ErrorBoundary:constructor(...)
		super.constructor(self, ...)
		self.currentState = {
			hasError = false,
		}
	end
	function ErrorBoundary:componentDidCatch(message)
		print(message, self:greet())
	end
	LogPropFactory("state")(ErrorBoundary, "currentState")
	local _descriptor = LogMethodFactory("derived")(ErrorBoundary, "componentDidCatch", {
		value = ErrorBoundary.componentDidCatch,
	})
	if _descriptor then
		ErrorBoundary.componentDidCatch = _descriptor.value
	end
	ErrorBoundary = Component(ErrorBoundary) or ErrorBoundary
end
print(Decorated.new(1), MutableDecorated, ComputedDecorated, Factories, ErrorBoundary, mutableDeco)
return {
	ErrorBoundary = ErrorBoundary,
}
