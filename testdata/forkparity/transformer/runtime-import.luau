-- Compiled with sloptor v3.0.0-beta.1
local TS = _G[script]
local function runtimePromise()
	return TS.Promise.new(function(resolve)
		return resolve(42)
	end)
end
return {
	runtimePromise = runtimePromise,
}
