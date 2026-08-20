-- Compiled with sloptor v2.3.2
local TS = require(script.Parent.Parent.include.RuntimeLib)
local Roact = TS.import(script, script.Parent.Parent, "node_modules", "@rbxts", "roact", "src")
return function()
	describe("Roact Spread operator", function()
		it("should spread properties", function()
			local props = {
				BackgroundColor3 = Color3.fromRGB(50, 50, 50),
			}
			local _attributes = table.clone(props)
			setmetatable(_attributes, nil)
			local element = Roact.jsx("frame", _attributes)
			expect(element.props.BackgroundColor3).to.equal(props.BackgroundColor3)
		end)
		it("should support spread as well as regular properties", function()
			local props = {
				BackgroundColor3 = Color3.fromRGB(50, 50, 50),
			}
			local TextColor = Color3.fromRGB(220, 220, 220)
			local _attributes = {
				TextColor3 = TextColor,
			}
			for _k, _v in props do
				_attributes[_k] = _v
			end
			local element = Roact.jsx("textlabel", _attributes)
			local elementProps = element.props
			expect(elementProps.BackgroundColor3).to.equal(props.BackgroundColor3)
			expect(elementProps.TextColor3).to.equal(TextColor)
		end)
		it("should support multiple spreads", function()
			local props = {
				BackgroundColor3 = Color3.fromRGB(50, 50, 50),
			}
			local props2 = {
				TextColor3 = Color3.fromRGB(220, 220, 220),
			}
			local _attributes = table.clone(props)
			setmetatable(_attributes, nil)
			for _k, _v in props2 do
				_attributes[_k] = _v
			end
			local element = Roact.jsx("textlabel", _attributes)
			local elementProps = element.props
			expect(elementProps.TextColor3).to.equal(props2.TextColor3)
			expect(elementProps.BackgroundColor3).to.equal(props.BackgroundColor3)
		end)
		it("should support spreading for custom objects", function()
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
					return Roact.jsx("textlabel", {
						Text = self.props.Text,
						TextColor3 = self.props.TextColor3,
					})
				end
			end
			local setValues = {
				Text = "The text here",
			}
			local _attributes = {
				TextColor3 = Color3.new(1, 0, 0),
			}
			for _k, _v in setValues do
				_attributes[_k] = _v
			end
			local element = Roact.jsx(RoactClass, _attributes)
			local elementProps = element.props
			expect(elementProps.Text).to.equal(setValues.Text)
			expect(elementProps.TextColor3).to.equal(Color3.new(1, 0, 0))
		end)
		it("should support making templated objects", function()
			local ButtonSize
			do
				local _inverse = {}
				ButtonSize = setmetatable({}, {
					__index = _inverse,
				})
				ButtonSize.Small = 0
				_inverse[0] = "Small"
				ButtonSize.Normal = 1
				_inverse[1] = "Normal"
				ButtonSize.Large = 2
				_inverse[2] = "Large"
			end
			local smallButtonProps = {
				Size = UDim2.new(0, 150, 0, 20),
				TextSize = 12,
			}
			local normalButtonProps = {
				Size = UDim2.new(0, 200, 0, 30),
				TextSize = 20,
			}
			local largeButtonProps = {
				Size = UDim2.new(0, 300, 0, 40),
				TextSize = 30,
			}
			local buttonRef = Roact.createRef()
			local MyButton
			do
				local super = Roact.Component
				MyButton = setmetatable({}, {
					__tostring = function()
						return "MyButton"
					end,
					__index = super,
				})
				MyButton.__index = MyButton
				function MyButton.new(...)
					local self = setmetatable({}, MyButton)
					return self:constructor(...) or self
				end
				function MyButton:constructor(...)
					super.constructor(self, ...)
				end
				function MyButton:render()
					local _binding = self.props
					local Size = _binding.Size
					local Text = _binding.Text
					local overloadProps = {
						Text = Text,
					}
					if Size == ButtonSize.Normal then
						local _attributes = {
							Ref = buttonRef,
						}
						for _k, _v in normalButtonProps do
							_attributes[_k] = _v
						end
						for _k, _v in overloadProps do
							_attributes[_k] = _v
						end
						return Roact.jsx("textbutton", _attributes)
					elseif Size == ButtonSize.Large then
						local _attributes = {
							Ref = buttonRef,
						}
						for _k, _v in largeButtonProps do
							_attributes[_k] = _v
						end
						for _k, _v in overloadProps do
							_attributes[_k] = _v
						end
						return Roact.jsx("textbutton", _attributes)
					elseif Size == ButtonSize.Small then
						local _attributes = {
							Ref = buttonRef,
						}
						for _k, _v in smallButtonProps do
							_attributes[_k] = _v
						end
						for _k, _v in overloadProps do
							_attributes[_k] = _v
						end
						return Roact.jsx("textbutton", _attributes)
					else
						error("Size specified not supported.")
					end
				end
			end
			local largeButton = Roact.jsx(MyButton, {
				Size = ButtonSize.Large,
				Text = "Large Button",
			})
			local normalButton = Roact.jsx(MyButton, {
				Size = ButtonSize.Normal,
				Text = "Normal Button",
			})
			local smallButton = Roact.jsx(MyButton, {
				Size = ButtonSize.Small,
				Text = "Small Button",
			})
			expect((largeButton.props).Size).to.equal(ButtonSize.Large)
			expect((normalButton.props).Size).to.equal(ButtonSize.Normal)
			expect((smallButton.props).Size).to.equal(ButtonSize.Small)
			local buttons = { {
				button = largeButton,
				template = largeButtonProps,
			}, {
				button = normalButton,
				template = normalButtonProps,
			}, {
				button = smallButton,
				template = smallButtonProps,
			} }
			for _, button in buttons do
				Roact.mount(button.button)
				expect(buttonRef:getValue()).to.be.ok()
				expect(buttonRef:getValue().TextSize).to.equal(button.template.TextSize)
				expect(buttonRef:getValue().Size).to.equal(button.template.Size)
			end
		end)
	end)
end
