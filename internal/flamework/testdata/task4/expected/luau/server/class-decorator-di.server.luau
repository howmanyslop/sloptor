-- Compiled with roblox-ts v3.0.0
local TS = require(game:GetService("ReplicatedStorage"):WaitForChild("rbxts_include"):WaitForChild("RuntimeLib"))
local _core = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@flamework", "core", "out")
local Reflect = _core.Reflect
local Flamework = _core.Flamework
local _core_1 = TS.import(script, game:GetService("ReplicatedStorage"), "rbxts_include", "node_modules", "@flamework", "core", "out")
local Modding = _core_1.Modding
local Service = _core_1.Service
local Tracked = Modding.createDecorator("Class", function()
	return nil
end)
local LoggerService
do
	LoggerService = setmetatable({}, {
		__tostring = function()
			return "LoggerService"
		end,
	})
	LoggerService.__index = LoggerService
	function LoggerService.new(...)
		local self = setmetatable({}, LoggerService)
		return self:constructor(...) or self
	end
	function LoggerService:constructor()
		self.value = "controlled"
	end
	do
		-- (Flamework) LoggerService metadata
		Reflect.defineMetadata(LoggerService, "identifier", "task4:server/class-decorator-di.server@LoggerService")
	end
end
-- (Flamework) LoggerService decorators
Reflect.decorate(LoggerService, "$:flamework@Service", Service, { {
	loadOrder = 1,
} })
local ConsumerService
do
	ConsumerService = setmetatable({}, {
		__tostring = function()
			return "ConsumerService"
		end,
	})
	ConsumerService.__index = ConsumerService
	function ConsumerService.new(...)
		local self = setmetatable({}, ConsumerService)
		return self:constructor(...) or self
	end
	function ConsumerService:constructor(logger)
		self.logger = logger
	end
	function ConsumerService:onStart()
		print(self.logger.value)
	end
	do
		-- (Flamework) ConsumerService metadata
		Reflect.defineMetadata(ConsumerService, "identifier", "task4:server/class-decorator-di.server@ConsumerService")
		Reflect.defineMetadata(ConsumerService, "flamework:parameters", { "task4:server/class-decorator-di.server@LoggerService" })
		Reflect.defineMetadata(ConsumerService, "flamework:implements", { "$:flamework@OnStart" })
	end
end
-- (Flamework) ConsumerService decorators
Reflect.decorate(ConsumerService, "$:flamework@Service", Service, {})
Reflect.decorate(ConsumerService, "task4:server/class-decorator-di.server@Tracked", Tracked, { "fixture" })
local resolvedLogger = Flamework.resolveDependency("task4:server/class-decorator-di.server@LoggerService")
return {
	LoggerService = LoggerService,
	ConsumerService = ConsumerService,
	resolvedLogger = resolvedLogger,
}
