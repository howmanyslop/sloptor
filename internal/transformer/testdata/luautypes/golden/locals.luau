-- Compiled with sloptor v3.0.0-beta.1
local TS = _G[script]
-- Locals: an explicitly annotated `let`, an annotation wider than its
-- initializer, an uninitialized `let`, an inferred `const`, a local hoisted
-- above its declaration, and bindings imported from a sibling module.
local _primitives = TS.import(script, script.Parent, "primitives")
local aNumber = _primitives.aNumber
local aString = _primitives.aString
local function locals(): number
	local count = 0
	local label: string?
	local names = { "a", "b" }
	for _, name in names do
		count += #name
		label = name
	end
	return if label == nil then count else count + #label
end
local function widened(flag: boolean): string | number
	local value = if flag then "a" else 1
	return value
end
local later: () -> number
local function hoisted(): number
	return later() + aNumber + #aString
end
function later(): number
	return 1
end
local function narrowed(write: boolean): "r" | "w"
	local mode: "r" | "w" = "r"
	if write then
		mode = "w"
	end
	return mode
end
return {
	locals = locals,
	widened = widened,
	hoisted = hoisted,
	narrowed = narrowed,
}
