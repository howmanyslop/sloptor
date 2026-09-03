-- Compiled with sloptor v2.3.3
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
