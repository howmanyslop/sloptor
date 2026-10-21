-- Compiled with sloptor v2.9.0-beta.1
local function iterateTable(t)
	local sum = 0
	for k, v in t do
		sum += v
	end
	return sum
end
return {
	iterateTable = iterateTable,
}
