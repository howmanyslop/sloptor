-- Compiled with sloptor v2.4.0
local function arrayRest()
	local _binding = { 1, 2, 3 }
	local a = _binding[1]
	local b = table.move(_binding, 2, #_binding, 1, {})
	return b
end
return {
	arrayRest = arrayRest,
}
