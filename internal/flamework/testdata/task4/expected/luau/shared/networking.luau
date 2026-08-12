-- Compiled with roblox-ts v3.0.0
local TS = require(game:GetService("ReplicatedStorage"):WaitForChild("rbxts_include"):WaitForChild("RuntimeLib"))
local Networking = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@flamework", "networking", "out").Networking
local events = Networking.createEvent("task4:shared/networking@events")
local functions = Networking.createFunction("task4:shared/networking@functions")
return {
	events = events,
	functions = functions,
}
