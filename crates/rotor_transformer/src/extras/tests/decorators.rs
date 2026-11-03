//! `decorators_test.go`. All expected text is byte for byte what rbxtsc 3.0.0 emits for the same
//! source with `"experimentalDecorators": true` (header and trailing return stripped).

use super::{assert_renders, render_module_fixture};

/// The full legacy-decorator interleave: instance members, then static members, then class
/// decorators, emitted last inside the class do-block; expressions initialize first to last but
/// apply last to first; the method, property and parameter shapes; the `shouldInline` spills;
/// and computed method names reusing the pinned `_key` temp.
#[test]
#[expect(clippy::too_many_lines, reason = "the expected text is verbatim")]
fn decorators() {
	let want = r#"local function Component(ctor)
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
local function LogMethod(target, key, descriptor)
	print("LogMethod", target, key, descriptor)
end
local function LogParam(target, key, index)
	print("LogParam", target, key, index)
end
local mutableDeco = function(ctor)
	print("mutable", ctor)
end
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
local Exported
do
	Exported = setmetatable({}, {
		__tostring = function()
			return "Exported"
		end,
	})
	Exported.__index = Exported
	function Exported.new(...)
		local self = setmetatable({}, Exported)
		return self:constructor(...) or self
	end
	function Exported:constructor()
	end
	function Exported:multi()
	end
	local _descriptor = LogMethod(Exported, "multi", {
		value = Exported.multi,
	})
	if _descriptor then
		Exported.multi = _descriptor.value
	end
	local _descriptor_1 = LogMethod(Exported, "multi", {
		value = Exported.multi,
	})
	if _descriptor_1 then
		Exported.multi = _descriptor_1.value
	end
	Exported = Component(Exported) or Exported
end
print(Decorated, MutableDecorated, ComputedDecorated, Exported, mutableDeco)
"#;
	assert_renders(&render_module_fixture("classes", "src/decorators.ts"), want);
}

/// `@ReactComponent export class ErrorBoundary extends React.Component<P, S>`: the class
/// decorator emits after all members, a decorator factory on a property or method inlines as a
/// call of a call, and a property-access decorator inlines too.
#[test]
fn decorator_error_boundary() {
	let want = r#"local function ReactComponent(ctor)
	print("ReactComponent", ctor)
end
local function LogF(tag)
	return function(target, key)
		print("LogF", tag, target, key)
	end
end
local function LogMF(tag)
	return function(target, key, descriptor)
		print("LogMF", tag, target, key, descriptor)
	end
end
local ns = {
	deco = function(target, key)
		print("ns.deco", target, key)
	end,
}
local Component
do
	Component = setmetatable({}, {
		__tostring = function()
			return "Component"
		end,
	})
	Component.__index = Component
	function Component.new(...)
		local self = setmetatable({}, Component)
		return self:constructor(...) or self
	end
	function Component:constructor()
	end
	function Component:setState(s)
		self.state = s
	end
end
local React = {
	Component = Component,
}
local ErrorBoundary
do
	local super = React.Component
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
		self.other = 1
	end
	function ErrorBoundary:componentDidCatch(message)
		self:setState({
			hasError = true,
		})
		print(message)
	end
	LogF("state")(ErrorBoundary, "currentState")
	ns.deco(ErrorBoundary, "other")
	local _descriptor = LogMF("derived")(ErrorBoundary, "componentDidCatch", {
		value = ErrorBoundary.componentDidCatch,
	})
	if _descriptor then
		ErrorBoundary.componentDidCatch = _descriptor.value
	end
	ErrorBoundary = ReactComponent(ErrorBoundary) or ErrorBoundary
end
print(ErrorBoundary)
"#;
	assert_renders(&render_module_fixture("classes", "src/errorboundary.ts"), want);
}
