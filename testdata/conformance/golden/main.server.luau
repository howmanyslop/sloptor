-- Compiled with sloptor v2.3.0
local TS = require(script.Parent.include.RuntimeLib)
--/ <reference types="@rbxts/testez/globals" />
local ServerScriptService = TS.import(script, script.Parent, "node_modules", "@rbxts", "services").ServerScriptService
local TestEZ = TS.import(script, script.Parent, "node_modules", "@rbxts", "testez", "src")
local results = TestEZ.TestBootstrap:run({ ServerScriptService.tests })
if #results.errors > 0 or results.failureCount > 0 then
	error("Tests failed!")
end
