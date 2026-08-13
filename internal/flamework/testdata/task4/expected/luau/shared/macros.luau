-- Compiled with sloptor v2.3.0
local TS = require(game:GetService("ReplicatedStorage"):WaitForChild("rbxts_include"):WaitForChild("RuntimeLib"))
local t = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@flamework", "core", "out", "prelude").t
local _core = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@flamework", "core", "out")
local Flamework = _core.Flamework
local Modding = _core.Modding
local payloadId = "task4:shared/macros@Payload"
local payloadHash = "00000000-0000-4000-8000-000000000004"
local payloadGuard = Flamework.createGuard(t.interface({
	count = t.number,
	enabled = t.boolean,
	label = t.string,
}))
local payloadShape = Modding.inspect({
	id = "task4:shared/macros@Payload",
	text = "Payload",
})
local tupleLabels = Modding.inspect({ "name", "amount" })
local callsite = Modding.inspect({
	text = 'Modding.inspect<Modding.CallerMany<\"line\" | \"character\" | \"text\" | \"uuid\">>()',
	line = 14,
	character = 25,
	uuid = "00000000-0000-4000-8000-000000000004",
})
Flamework._addPaths({ { "ServerScriptService", "TS" } })
Flamework._addPathsGlob("src/**/*.ts")
return {
	payloadId = payloadId,
	payloadHash = payloadHash,
	payloadGuard = payloadGuard,
	payloadShape = payloadShape,
	tupleLabels = tupleLabels,
	callsite = callsite,
}
