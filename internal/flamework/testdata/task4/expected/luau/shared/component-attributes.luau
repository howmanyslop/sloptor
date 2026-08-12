-- Compiled with roblox-ts v3.0.0
local TS = require(game:GetService("ReplicatedStorage"):WaitForChild("rbxts_include"):WaitForChild("RuntimeLib"))
local Reflect = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@flamework", "core", "out").Reflect
local t = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@flamework", "core", "out", "prelude").t
local _components = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@flamework", "components", "out")
local BaseComponent = _components.BaseComponent
local Component = _components.Component
local FixtureComponent
do
	local super = BaseComponent
	FixtureComponent = setmetatable({}, {
		__tostring = function()
			return "FixtureComponent"
		end,
		__index = super,
	})
	FixtureComponent.__index = FixtureComponent
	function FixtureComponent.new(...)
		local self = setmetatable({}, FixtureComponent)
		return self:constructor(...) or self
	end
	function FixtureComponent:constructor(...)
		super.constructor(self, ...)
	end
	do
		-- (Flamework) FixtureComponent metadata
		Reflect.defineMetadata(FixtureComponent, "identifier", "task4:shared/component-attributes@FixtureComponent")
	end
end
-- (Flamework) FixtureComponent decorators
Reflect.decorate(FixtureComponent, "$c:components@Component", Component, { {
	tag = "Task4Fixture",
	attributes = {
		active = t.boolean,
		label = t.string,
		retries = t.number,
	},
	instanceGuard = t.instanceIsA("Part"),
} })
return {
	FixtureComponent = FixtureComponent,
}
