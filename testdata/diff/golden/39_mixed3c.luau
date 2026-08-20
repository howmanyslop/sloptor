-- Compiled with sloptor v2.3.2
local TS = require(script.Parent.include.RuntimeLib)
-- Phase 3c interactions: decorated class components rendering JSX, spread
-- props through table.clone, enum-keyed Map iteration feeding children,
-- generators (with yield* inside try) spread into JSX, async methods with
-- try/await + break/continue rerouting inside loops, ??= on class fields,
-- namespace-exported components as JSX tags, call spread into variadics.
local React = TS.import(script, script.Parent, "node_modules", "@rbxts", "react")
local Slot
do
	local _inverse = {}
	Slot = setmetatable({}, {
		__index = _inverse,
	})
	Slot.Head = 0
	_inverse[0] = "Head"
	Slot.Body = 1
	_inverse[1] = "Body"
	Slot.Legs = 2
	_inverse[2] = "Legs"
end
local labels = {
	[Slot.Head] = "Head",
	[Slot.Body] = "Body",
	[Slot.Legs] = "Legs",
}
local function Card(props)
	return React.createElement("textlabel", {
		Text = props.title,
		LayoutOrder = props.order,
	})
end
local UI = {}
do
	local _container = UI
	local function Badge(props)
		return React.createElement("textlabel", {
			Text = props.text,
		})
	end
	_container.Badge = Badge
	local DEFAULT_TEXT = "badge"
	_container.DEFAULT_TEXT = DEFAULT_TEXT
end
local function Tag(label)
	return function(ctor)
		return print("Tag", label, ctor)
	end
end
local function LogMethod(target, key, descriptor)
	print("LogMethod", key, descriptor)
end
local function extras()
	return TS.generator(function()
		coroutine.yield(React.createElement(UI.Badge, {
			key = "extra",
			text = UI.DEFAULT_TEXT,
		}))
	end)
end
local function cards()
	return TS.generator(function()
		local order = 0
		TS.try(function()
			for slot, title in labels do
				if slot == Slot.Legs then
					break
				end
				order += 1
				coroutine.yield(React.createElement(Card, {
					key = `gen{order}`,
					title = title,
					order = order,
				}))
			end
			for _result in extras().next do
				if _result.done then
					break
				end
				coroutine.yield(_result.value)
			end
		end, function(e)
			print("generator failed", e)
		end)
	end)
end
local Panel
do
	local super = React.Component
	Panel = setmetatable({}, {
		__tostring = function()
			return "Panel"
		end,
		__index = super,
	})
	Panel.__index = Panel
	function Panel.new(...)
		local self = setmetatable({}, Panel)
		return self:constructor(...) or self
	end
	function Panel:constructor(props)
		super.constructor(self, props)
		Panel.instances += 1
	end
	Panel.load = TS.async(function(self, sources)
		local found
		for _, src in sources do
			local _exitType, _returns = TS.try(function()
				local value = TS.await(src())
				if found == nil then
					found = value
				end
				if value == "stop" then
					return TS.TRY_BREAK
				end
			end, function(e)
				print("load failed", e)
				return TS.TRY_CONTINUE
			end)
			if _exitType == TS.TRY_BREAK then
				break
			elseif _exitType then
				continue
			end
		end
		return found
	end)
	function Panel:render()
		if self.cache == nil then
			self.cache = labels
		end
		local entries = (self.cache)
		local children = {}
		for slot, title in entries do
			local _arg0 = React.createElement(Card, {
				key = tostring(slot),
				title = title,
				order = slot,
			})
			table.insert(children, _arg0)
		end
		local _object = {
			title = self.props.header,
		}
		local _left = "order"
		local _condition = self.props.accent
		if _condition == nil then
			_condition = 0
		end
		_object[_left] = _condition
		local shared = _object
		local _array = {}
		local _length = #_array
		for _k, _v in labels do
			_length += 1
			_array[_length] = { _k, _v }
		end
		-- ▼ ReadonlyArray.map ▼
		local _newValue = table.create(#_array)
		local _callback = function(_param)
			local slot = _param[1]
			return slot
		end
		for _k, _v in _array do
			_newValue[_k] = _callback(_v, _k - 1, _array)
		end
		-- ▲ ReadonlyArray.map ▲
		local orders = _newValue
		local _attributes = table.clone(shared)
		setmetatable(_attributes, nil)
		local _exp = React.createElement(Card, _attributes)
		local _attributes_1 = table.clone(shared)
		setmetatable(_attributes_1, nil)
		_attributes_1.order = Panel.instances
		local _exp_1 = React.createElement(Card, _attributes_1)
		local _exp_2 = children
		local _array_1 = {}
		local _length_1 = #_array_1
		for _result in cards().next do
			if _result.done then
				break
			end
			_length_1 += 1
			_array_1[_length_1] = _result.value
		end
		return React.createElement("frame", {
			LayoutOrder = math.max(0, unpack(orders)),
		}, _exp, _exp_1, _exp_2, _array_1, React.createElement("textbutton", {
			Text = Slot[Slot.Body],
			Event = {
				MouseButton1Click = TS.async(function()
					TS.try(function()
						print(TS.await(self:load({ TS.async(function()
							return "stop"
						end) })))
					end, nil, function()
						print("handled")
					end)
				end),
			},
		}))
	end
	Panel.instances = 0
	local _descriptor = LogMethod(Panel, "load", {
		value = Panel.load,
	})
	if _descriptor then
		Panel.load = _descriptor.value
	end
	local _decorator = Tag("panel")
	Panel = React.ReactComponent(Panel) or Panel
	Panel = _decorator(Panel) or Panel
end
local tree = (React.createElement("screengui", {
	ResetOnSpawn = false,
}, React.createElement(Panel, {
	header = "H",
	accent = 2,
}), React.createElement(UI.Badge, {
	text = "outer",
})))
return {
	Panel = Panel,
	tree = tree,
}
