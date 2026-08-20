-- Compiled with sloptor v2.3.2
local TS = require(game:GetService("ReplicatedStorage"):WaitForChild("rbxts_include"):WaitForChild("RuntimeLib"))
local t = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@flamework", "core", "out", "prelude").t
local Flamework = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@flamework", "core", "out").Flamework
local dedup = t.interface({
	id = t.string,
	value = t.number,
})
local repeatedGuard = Flamework.createGuard(t.interface({
	first = dedup,
	second = dedup,
	third = dedup,
}))
local unionGuard = Flamework.createGuard(t.optional(t.union(t.number, t.literal("ready"))))
local tupleGuard = Flamework.createGuard(t.strictArray(t.string, t.number, t.boolean))
return {
	repeatedGuard = repeatedGuard,
	unionGuard = unionGuard,
	tupleGuard = tupleGuard,
}
