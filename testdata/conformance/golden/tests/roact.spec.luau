-- Compiled with sloptor v2.4.0
local TS = require(script.Parent.Parent.include.RuntimeLib)
local Roact = TS.import(script, script.Parent.Parent, "node_modules", "@rbxts", "roact", "src")
local _result = game:GetService("ReplicatedStorage"):FindFirstChild("include")
if _result ~= nil then
	_result = _result:FindFirstChild("node_modules")
	if _result ~= nil then
		_result = _result:FindFirstChild("@rbxts")
		if _result ~= nil then
			_result = _result:FindFirstChild("roact")
			if _result ~= nil then
				_result = _result:FindFirstChild("src")
			end
		end
	end
end
local RoactModule = _result
assert(RoactModule, "Unable to find @rbxts/roact!")
local ElementKindModule = RoactModule:FindFirstChild("ElementKind")
assert(ElementKindModule, "Unable to find ElementKind module in @rbxts/roact!")
local ElementKind = require(ElementKindModule)
local TypeModule = RoactModule:FindFirstChild("Type")
assert(TypeModule, "Unable to find Type module in @rbxts/roact!")
local Type = require(TypeModule)
return function()
	describe("should support Roact.Component", function()
		it("should construct a roact class", function()
			local RoactClass
			do
				local super = Roact.Component
				RoactClass = setmetatable({}, {
					__tostring = function()
						return "RoactClass"
					end,
					__index = super,
				})
				RoactClass.__index = RoactClass
				function RoactClass.new(...)
					local self = setmetatable({}, RoactClass)
					return self:constructor(...) or self
				end
				function RoactClass:constructor(...)
					super.constructor(self, ...)
				end
				function RoactClass:render()
					return Roact.jsx("frame")
				end
			end
			expect(Type.of(RoactClass)).to.equal(Type.StatefulComponentClass)
		end)
		it("should construct a roact pure component class", function()
			local RoactPureClass
			do
				local super = Roact.PureComponent
				RoactPureClass = setmetatable({}, {
					__tostring = function()
						return "RoactPureClass"
					end,
					__index = super,
				})
				RoactPureClass.__index = RoactPureClass
				function RoactPureClass.new(...)
					local self = setmetatable({}, RoactPureClass)
					return self:constructor(...) or self
				end
				function RoactPureClass:constructor(...)
					super.constructor(self, ...)
				end
				function RoactPureClass:render()
					return Roact.jsx("frame")
				end
			end
			expect(Type.of(RoactPureClass)).to.equal(Type.StatefulComponentClass)
		end)
		it("should construct default props", function()
			local RoactClass
			do
				local super = Roact.Component
				RoactClass = setmetatable({}, {
					__tostring = function()
						return "RoactClass"
					end,
					__index = super,
				})
				RoactClass.__index = RoactClass
				function RoactClass.new(...)
					local self = setmetatable({}, RoactClass)
					return self:constructor(...) or self
				end
				function RoactClass:constructor(...)
					super.constructor(self, ...)
				end
				function RoactClass:render()
					return Roact.jsx("frame")
				end
				RoactClass.defaultProps = {
					value = 10,
				}
			end
			expect(RoactClass.defaultProps).to.be.a("table")
		end)
		it("should support static getDerivedStateFromProps", function()
			local RoactClass
			do
				local super = Roact.Component
				RoactClass = setmetatable({}, {
					__tostring = function()
						return "RoactClass"
					end,
					__index = super,
				})
				RoactClass.__index = RoactClass
				function RoactClass.new(...)
					local self = setmetatable({}, RoactClass)
					return self:constructor(...) or self
				end
				function RoactClass:constructor(...)
					super.constructor(self, ...)
				end
				function RoactClass:getDerivedStateFromProps(nextProps, lastState)
					return {
						someValue = nextProps.someValue,
					}
				end
				function RoactClass:render()
					return Roact.jsx("frame")
				end
			end
			expect(RoactClass.getDerivedStateFromProps).to.be.a("function")
		end)
		it("should mount a roact object", function()
			local RoactClass
			do
				local super = Roact.Component
				RoactClass = setmetatable({}, {
					__tostring = function()
						return "RoactClass"
					end,
					__index = super,
				})
				RoactClass.__index = RoactClass
				function RoactClass.new(...)
					local self = setmetatable({}, RoactClass)
					return self:constructor(...) or self
				end
				function RoactClass:constructor(...)
					super.constructor(self, ...)
				end
				function RoactClass:render()
					return Roact.jsx("frame")
				end
			end
			local element = Roact.jsx(RoactClass)
			expect(Type.of(element)).to.equal(Type.Element)
			local handle = Roact.mount(element)
			expect(Type.of(handle)).to.equal(Type.VirtualTree)
		end)
	end)
	it("should create roact intrinsics", function()
		local RoactIntrinsic = Roact.jsx("frame")
		local RoactIntrinsicManual = Roact.createElement("Frame")
		-- Both should be a Frame
		expect(RoactIntrinsic.component).to.equal(RoactIntrinsicManual.component)
	end)
	describe("should support all roact property types", function()
		-- it("should be able to have keyed children", () => {
		-- 	const KEY = "key1";
		-- 	const KEY2 = "key2";
		-- 	const element = (
		-- 		<screengui>
		-- 			<frame Key={KEY} />
		-- 			<frame Key={KEY2} />
		-- 		</screengui>
		-- 	);
		-- 	const handle = Roact.mount(element);
		-- 	const frameKey = handle._children![KEY];
		-- 	const frame2Key = handle._children![KEY2];
		-- 	expect(frameKey).to.be.ok();
		-- 	expect(frame2Key).to.be.ok();
		-- });
		it("should support props", function()
			local TEXT = "Hello, World!"
			local propElement = Roact.jsx("textbutton", {
				Text = TEXT,
			})
			local propElementProps = propElement.props
			expect(propElementProps.Text).to.equal(TEXT)
		end)
		local EventHack = Roact.Event
		local ChangeHack = Roact.Change
		it("should support [Roact.Event]", function()
			local eventElement = (Roact.jsx("textbutton", {
				Event = {
					MouseButton1Click = function() end,
				},
			}))
			local eventElementProps = eventElement.props
			expect(eventElementProps[EventHack.MouseButton1Click]).to.be.a("function")
		end)
		it("should support [Roact.Change]", function()
			local eventElement = (Roact.jsx("textbutton", {
				Change = {
					AbsoluteSize = function() end,
				},
			}))
			local eventElementProps = eventElement.props
			expect(eventElementProps[ChangeHack.AbsoluteSize]).to.be.a("function")
		end)
		describe("should support [Roact.Ref]", function()
			--[[
				
								These are based basically off the Roact tests.
							
			]]
			it("should handle object references properly", function()
				local frameRef = Roact.createRef()
				Roact.mount(Roact.jsx("frame", {
					Ref = frameRef,
				}))
				-- expect(frameRef.current).to.be.ok();
				expect(Type.of(frameRef)).to.equal(Type.Binding)
			end)
			it("should handle function references properly", function()
				local currentRbx
				local function ref(rbx)
					currentRbx = rbx
				end
				local element = Roact.jsx("frame", {
					Ref = ref,
				})
				local handle = Roact.mount(element)
				expect(currentRbx).to.be.ok()
			end)
			it("should handle class references properly", function()
				local RoactRefTest
				do
					local super = Roact.Component
					RoactRefTest = setmetatable({}, {
						__tostring = function()
							return "RoactRefTest"
						end,
						__index = super,
					})
					RoactRefTest.__index = RoactRefTest
					function RoactRefTest.new(...)
						local self = setmetatable({}, RoactRefTest)
						return self:constructor(...) or self
					end
					function RoactRefTest:constructor(p)
						super.constructor(self, p)
						self.ref = Roact.createRef()
					end
					function RoactRefTest:render()
						return Roact.jsx("screengui", {
							Ref = self.ref,
						})
					end
					function RoactRefTest:didUpdate()
						expect(self.ref:getValue()).to.be.ok()
					end
				end
				Roact.mount(Roact.jsx(RoactRefTest))
			end)
			it("should handle class function references properly", function()
				local worked = false
				local RoactRefTest
				do
					local super = Roact.Component
					RoactRefTest = setmetatable({}, {
						__tostring = function()
							return "RoactRefTest"
						end,
						__index = super,
					})
					RoactRefTest.__index = RoactRefTest
					function RoactRefTest.new(...)
						local self = setmetatable({}, RoactRefTest)
						return self:constructor(...) or self
					end
					function RoactRefTest:constructor(...)
						super.constructor(self, ...)
						self.onScreenGuiRender = function(rbx)
							worked = true
						end
					end
					function RoactRefTest:render()
						return Roact.jsx("screengui", {
							Ref = self.onScreenGuiRender,
						})
					end
				end
				Roact.mount(Roact.jsx(RoactRefTest))
				expect(worked).to.be.ok()
			end)
		end)
	end)
	it("should support JSX Fragments", function()
		local fragment = (Roact.jsx(Roact.Fragment, nil, Roact.jsx("frame", {
			Key = "TestKey",
		})))
		expect(Type.of(fragment)).to.equal(Type.Element)
		expect(ElementKind.of(fragment)).to.equal(ElementKind.Fragment)
		expect(ElementKind.of(fragment.elements.TestKey)).to.equal(ElementKind.Host)
	end)
	describe("BinaryExpressions", function()
		it("should support BinaryExpressions", function()
			local ref
			local test = Roact.jsx("frame", nil, true and Roact.jsx("frame", {
				Ref = function(rbx)
					ref = rbx
					return ref
				end,
			}))
			Roact.mount(test)
			expect(ref).to.be.ok()
		end)
	end)
	describe("ConditionalExpressions", function()
		it("should support ConditionalExpressions", function()
			local ref
			local ref2
			local test = (Roact.jsx("frame", nil, if false then Roact.jsx("frame", {
				Ref = function(rbx)
					ref = rbx
					return ref
				end,
			}) else Roact.jsx("textlabel", {
				Ref = function(rbx)
					ref = rbx
					return ref
				end,
			})))
			local test2 = (Roact.jsx("frame", nil, if true then Roact.jsx("frame", {
				Ref = function(rbx)
					ref2 = rbx
					return ref2
				end,
			}) else Roact.jsx("textlabel", {
				Ref = function(rbx)
					ref2 = rbx
					return ref2
				end,
			})))
			Roact.mount(test)
			Roact.mount(test2)
			expect(ref).to.be.ok()
			local _ref = ref
			local _condition = typeof(_ref) == "Instance"
			if _condition then
				_condition = ref:IsA("TextLabel")
			end
			expect(_condition).to.equal(true)
			expect(ref2).to.be.ok()
			local _ref2 = ref2
			local _condition_1 = typeof(_ref2) == "Instance"
			if _condition_1 then
				_condition_1 = ref2:IsA("Frame")
			end
			expect(_condition_1).to.equal(true)
		end)
	end)
	it("should support nesting maps", function()
		local map = {}
		local ref
		local testLabel = Roact.jsx("textlabel", {
			Ref = function(rbx)
				ref = rbx
				return ref
			end,
			Text = "Texty",
		})
		map.Testing = testLabel
		local element = Roact.jsx("screengui", nil, map)
		expect(Type.of(element)).to.equal(Type.Element)
		expect((element.props)[Roact.Children].Testing).to.equal(testLabel)
		Roact.mount(element)
		expect(ref).to.be.ok()
		local _ref = ref
		local _condition = typeof(_ref) == "Instance"
		if _condition then
			_condition = ref:IsA("TextLabel") and ref.Text == "Texty"
		end
		expect(_condition).to.equal(true)
	end)
	it("should support implicit true in JSX elements", function()
		local element = Roact.jsx("textlabel", {
			TextWrapped = true,
		})
		expect((element.props).TextWrapped).to.equal(true)
	end)
	it("should have correct-order prop joining", function()
		local TestProps = {
			Active = false,
			BackgroundColor3 = Color3.fromRGB(220, 0, 0),
		}
		local _attributes = {
			Active = true,
		}
		for _k, _v in TestProps do
			_attributes[_k] = _v
		end
		_attributes.BackgroundColor3 = Color3.new(0, 0, 0)
		local element = (Roact.jsx("frame", _attributes))
		local props = element.props
		expect(props.Active).to.equal(false)
		expect(props.BackgroundColor3).to.equal(Color3.new(0, 0, 0))
	end)
	it("should support passing identifier objects as events/changed handlers", function()
		local Events = {
			MouseEnter = function() end,
		}
		local f = Roact.jsx("frame", {
			Event = Events,
		})
		local props = f.props
		-- eslint-disable-next-line @typescript-eslint/ban-ts-ignore
		-- @ts-ignore because this is valid, but I can't infer the type for this
		expect(props[Roact.Event.MouseEnter]).to.equal(Events.MouseEnter)
	end)
end
