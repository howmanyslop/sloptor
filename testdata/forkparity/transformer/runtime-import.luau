-- Compiled with sloptor v2.3.0
local TS = _G[script]
local function runtimePromise()
	return TS.Promise.new(function(resolve)
		return resolve(42)
	end)
end
return {
	runtimePromise = runtimePromise,
}
