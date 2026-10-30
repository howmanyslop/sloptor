-- Compiled with sloptor v3.0.0-beta.1
local TS = _G[script]
-- Locals: an explicitly annotated `let`, an annotation wider than its
-- initializer, an uninitialized `let`, an inferred `const`, a local hoisted
-- above its declaration, and bindings imported from a sibling module.
local _primitives = TS.import(script, script.Parent, "primitives")
local aNumber: number = _primitives.aNumber
local aString: string = _primitives.aString
local function locals(): number
	local count: number = 0
	local label: string?
	local names = { "a", "b" }
	for _, name in names do
		count += #name
		label = name
	end
	return if label == nil then count else count + #label
end
local function widened(flag: boolean): string | number
	local value: string | number = if flag then "a" else 1
	return value
end
local later: () -> number
local function hoisted(): number
	return later() + aNumber + #aString
end
function later(): number
	return 1
end
return {
	locals = locals,
	widened = widened,
	hoisted = hoisted,
}
