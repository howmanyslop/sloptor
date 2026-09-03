-- Compiled with sloptor v2.3.3
local TS = require(script.Parent.include.RuntimeLib)
local React = TS.import(script, script.Parent, "node_modules", "@rbxts", "react")
-- self-closing host element, props incl. implicit-true
local a = React.createElement("frame", {
	BackgroundTransparency = 0.5,
	Visible = true,
})
-- element with props AND children (no nil separator)
local b = (React.createElement("screengui", {
	ResetOnSpawn = false,
}, React.createElement("frame", {
	Visible = true,
}), React.createElement("textlabel", {
	Text = "hi",
})))
-- fragment with children
local c = (React.createElement(React.Fragment, nil, React.createElement("frame"), React.createElement("frame")))
-- empty fragment
local empty = React.createElement(React.Fragment)
-- && and ternary expression children
local d = (React.createElement("frame", nil, cond and React.createElement("frame"), if cond then React.createElement("frame") else React.createElement("textlabel")))
-- key is a plain prop in the React world
local function Item(props)
	return React.createElement("textlabel", {
		Text = props.text,
	})
end
local f = (React.createElement("frame", {
	key = "container",
}, React.createElement(Item, {
	key = "one",
	text = "1",
}), React.createElement(Item, {
	key = "two",
	text = "2",
})))
-- Event/Change tables are ordinary props
local g = (React.createElement("textbutton", {
	Event = {
		MouseButton1Click = function()
			return print("click")
		end,
	},
	Change = {
		AbsoluteSize = function()
			return print("size")
		end,
	},
}))
-- .map children — the array is ONE child
-- ▼ ReadonlyArray.map ▼
local _newValue = table.create(#list)
local _callback = function(s)
	return React.createElement("textlabel", {
		key = s,
		Text = s,
	})
end
for _k, _v in list do
	_newValue[_k] = _callback(_v, _k - 1, list)
end
-- ▲ ReadonlyArray.map ▲
local h = React.createElement("frame", nil, _newValue)
-- attrs + children together
local j = React.createElement("frame", {
	Visible = true,
}, getEl())
-- the providers.tsx shape
local k = React.createElement(React.Fragment, nil, children)
return {
	a = a,
	b = b,
	c = c,
	empty = empty,
	d = d,
	f = f,
	g = g,
	h = h,
	j = j,
	k = k,
}
