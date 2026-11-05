local Type = require(script.Type)

local intrinsicComponentNames = {
	billboardgui = "BillboardGui",
	camera = "Camera",
	canvasgroup = "CanvasGroup",
	frame = "Frame",
	imagebutton = "ImageButton",
	imagelabel = "ImageLabel",
	screengui = "ScreenGui",
	scrollingframe = "ScrollingFrame",
	surfacegui = "SurfaceGui",
	textbox = "TextBox",
	textbutton = "TextButton",
	textlabel = "TextLabel",
	uiaspectratioconstraint = "UIAspectRatioConstraint",
	uicorner = "UICorner",
	uigradient = "UIGradient",
	uigridlayout = "UIGridLayout",
	uilistlayout = "UIListLayout",
	uipadding = "UIPadding",
	uipagelayout = "UIPageLayout",
	uiscale = "UIScale",
	uisizeconstraint = "UISizeConstraint",
	uistroke = "UIStroke",
	uitablelayout = "UITableLayout",
	uitextsizeconstraint = "UITextSizeConstraint",
	viewportframe = "ViewportFrame",
}

local Fragment = {}

local function cloneProps(props)
	if props == nil then
		return {}
	end

	local cloned = {}
	for key, value in pairs(props) do
		cloned[key] = value
	end
	return cloned
end

local function normalizeSpecialProps(props)
	local normalized = cloneProps(props)

	local ref = rawget(normalized, "Ref")
	if ref ~= nil then
		normalized.Ref = nil
		normalized[Roact.Ref] = ref
	end

	local events = rawget(normalized, "Event")
	if events ~= nil then
		normalized.Event = nil
		for eventName, listener in pairs(events) do
			normalized[Roact.Event[eventName]] = listener
		end
	end

	local changes = rawget(normalized, "Change")
	if changes ~= nil then
		normalized.Change = nil
		for propertyName, listener in pairs(changes) do
			normalized[Roact.Change[propertyName]] = listener
		end
	end

	local key = rawget(normalized, "Key")
	if key ~= nil then
		normalized.Key = nil
	end

	return normalized, key
end

local function getChildKey(child)
	if typeof(child) ~= "table" then
		return nil
	end
	return rawget(child, "_rotorKey")
end

local function normalizeChildren(...)
	local count = select("#", ...)
	if count == 0 then
		return nil
	end

	if count == 1 then
		local child = select(1, ...)
		if child == nil or typeof(child) == "boolean" then
			return nil
		end
		if typeof(child) == "table" and Type.of(child) ~= Type.Element then
			return child
		end
		local key = getChildKey(child)
		if key ~= nil then
			return { [key] = child }
		end
		return child
	end

	local children = {}
	local index = 1
	for i = 1, count do
		local child = select(i, ...)
		if child ~= nil and typeof(child) ~= "boolean" then
			local key = getChildKey(child)
			if key ~= nil then
				children[key] = child
			else
				children[index] = child
				index += 1
			end
		end
	end

	if next(children) == nil then
		return nil
	end
	return children
end

rawset(Roact, "Fragment", Fragment)

rawset(Roact, "jsx", function(component, props, ...)
	local normalizedProps, key = normalizeSpecialProps(props)
	local children = normalizeChildren(...)

	if component == Fragment then
		local fragment = Roact.createFragment(children)
		if key ~= nil then
			fragment._rotorKey = key
		end
		return fragment
	end

	if typeof(component) == "string" then
		component = intrinsicComponentNames[component] or (string.upper(string.sub(component, 1, 1)) .. string.sub(component, 2))
	end

	local element = Roact.createElement(component, normalizedProps, children)
	if key ~= nil then
		element._rotorKey = key
	end
	return element
end)
