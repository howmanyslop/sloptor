local fs = require("@lune/fs")
local luau = require("@lune/luau")
local process = require("@lune/process")
local roblox = require("@lune/roblox")

local mode = process.args[1]
assert(mode == "server" or mode == "client", "mode must be server or client")

for _, root in { "out/server", "out/client" } do
	for _, entry in fs.readDir(root) do
		local path = `{root}/{entry}`
		if fs.isFile(path) and string.sub(path, -5) == ".luau" then
			luau.load(fs.readFile(path), { debugName = path })
		end
	end
end

local placeInstance = roblox.deserializePlace(fs.readFile("task7-runtime.rbxl"))
local wrappers = {}
local wrap
function wrap(instance)
	if instance == nil then
		return nil
	end
	if wrappers[instance] then
		return wrappers[instance]
	end
	local wrapper = {}
	wrappers[instance] = wrapper
	setmetatable(wrapper, {
		__index = function(_, key)
			if key == "ClassName" or key == "Name" or key == "Source" then
				return instance[key]
		end
			if key == "Parent" then
				return wrap(instance.Parent)
			end
		end,
	})
	function wrapper:FindFirstChild(name)
		return wrap(instance:FindFirstChild(name))
	end
	function wrapper:GetDescendants()
		local descendants = {}
		for _, descendant in instance:GetDescendants() do
			table.insert(descendants, wrap(descendant))
		end
		return descendants
	end
	function wrapper:GetFullName()
		local names = { instance.Name }
		local current = instance.Parent
		while current do
			table.insert(names, 1, current.Name)
			current = current.Parent
		end
		return table.concat(names, ".")
	end
	function wrapper:IsA(className)
		return instance:IsA(className)
	end
	function wrapper:WaitForChild(name)
		local child = self:FindFirstChild(name)
		assert(child, `{self:GetFullName()} is missing {name}`)
		return child
	end
	return wrapper
end

local place = wrap(placeInstance)
local replicatedStorage = place:FindFirstChild("ReplicatedStorage")
local serverScriptService = place:FindFirstChild("ServerScriptService")
local starterPlayer = place:FindFirstChild("StarterPlayer")
assert(replicatedStorage and serverScriptService and starterPlayer, "Rojo place services missing")

local function newSignal()
	local listeners = {}
	return {
		Connect = function(_, callback)
			table.insert(listeners, callback)
			return { Disconnect = function() end }
		end,
		Once = function(_, callback)
			table.insert(listeners, callback)
			return { Disconnect = function() end }
		end,
		ConnectParallel = function(self, callback)
			return self:Connect(callback)
		end,
		Fire = function(_, ...)
			for _, callback in listeners do
				callback(...)
			end
		end,
		Wait = function()
			error("signal Wait is not used by this scenario")
		end,
		Destroy = function()
			table.clear(listeners)
		end,
	}
end

local runService = {
	Heartbeat = newSignal(),
	Stepped = newSignal(),
	RenderStepped = newSignal(),
	IsClient = function()
		return mode == "client"
	end,
	IsRunning = function()
		return true
	end,
	IsServer = function()
		return mode == "server"
	end,
	IsStudio = function()
		return false
	end,
}

local game = {
	GetService = function(_, name)
		if name == "RunService" then
			return runService
		end
		if name == "Players" then
			return {}
		end
		return place:FindFirstChild(name)
	end,
	IsLoaded = function()
		return true
	end,
}

local signalModule = {
	new = newSignal,
}
local events = {}
local baseDebug = debug
local runtimeDebug = setmetatable({
	profilebegin = function() end,
	resetmemorycategory = function() end,
	setmemorycategory = function() end,
}, { __index = baseDebug })
local runtimeTask = {
	spawn = function(target, ...)
		local thread = if type(target) == "thread" then target else coroutine.create(target)
		local ok, result = coroutine.resume(thread, ...)
		assert(ok, result)
		return thread
	end,
}

local moduleCache = {}
local loading = {}
local sharedGlobal = {}
local TS = {
	Promise = {
		Status = { Rejected = "Rejected" },
		is = function()
			return false
		end,
	},
}

local loadModule

function TS.getModule(_, scope, name)
	local include = replicatedStorage:WaitForChild("rbxts_include")
	return include:WaitForChild("node_modules"):WaitForChild(scope):WaitForChild(name)
end

function TS.import(_, module, ...)
	for index = 1, select("#", ...) do
		module = module:WaitForChild(select(index, ...))
	end
	return loadModule(module)
end

function TS.instanceof(object, class)
	local metatable = getmetatable(object)
	while metatable do
		if metatable == class then
			return true
		end
		metatable = getmetatable(metatable)
	end
	return false
end

local function runtimePrint(...)
	local first = tostring(select(1, ...))
	if string.sub(first, 1, 7) == "ASSERT " then
		table.insert(events, first)
	end
	print(...)
end

function loadModule(module)
	if module.Name == "RuntimeLib" then
		return TS
	end
	if module.Name == "services" then
		return setmetatable({}, {
			__index = function(_, name)
				return game:GetService(name)
			end,
		})
	end
	if module.Name == "signal" then
		return signalModule
	end
	local key = module:GetFullName()
	if moduleCache[key] ~= nil then
		return moduleCache[key]
	end
	assert(not loading[key], `circular module load: {key}`)
	loading[key] = true
	sharedGlobal[module] = TS
	local source = module.Source
	assert(type(source) == "string" and #source > 0, `module has no source: {module:GetFullName()}`)
	local environment = {
		_G = sharedGlobal,
		debug = runtimeDebug,
		game = game,
		print = runtimePrint,
		require = loadModule,
		script = module,
		task = runtimeTask,
		warn = print,
	}
	local chunk = luau.load(source, {
		debugName = module:GetFullName(),
		environment = environment,
		injectGlobals = true,
	})
	local result = chunk()
	moduleCache[key] = if result == nil then true else result
	loading[key] = nil
	return moduleCache[key]
end

local main
if mode == "server" then
	main = serverScriptService:WaitForChild("TS"):WaitForChild("main")
else
	main = starterPlayer:WaitForChild("StarterPlayerScripts"):WaitForChild("TS"):WaitForChild("main")
end
assert(string.find(main.Source, "Flamework.ignite", 1, true), "generated entrypoint missing Flamework ignition")
loadModule(main)

local expected = if mode == "server"
	then {
		"ASSERT service constructor injected singleton/value",
		"ASSERT service value init",
		"ASSERT service consumer init",
		"ASSERT service value start",
		"ASSERT service consumer start",
	}
	else {
		"ASSERT controller constructor injected singleton/value",
		"ASSERT controller value init",
		"ASSERT controller consumer init",
		"ASSERT controller value start",
		"ASSERT controller consumer start",
	}

assert(#events == #expected, `lifecycle event count {#events}, expected {#expected}`)
for index, event in expected do
	assert(events[index] == event, `lifecycle event {index}: {events[index]} ~= {event}`)
end
print(`PASS {mode} lifecycle order and constructor injection`)
