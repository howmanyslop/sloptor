-- Compiled with sloptor v2.3.3
local items = { 10, 20, 30 }
local total = 0
for _, item in items do
	total += item
end
for _, _binding in { { 1, 2 }, { 3, 4 } } do
	local i = _binding[1]
	local v = _binding[2]
	total += i + v
end
local words = { "a", "b" }
local joined = ""
for _, w in words do
	joined ..= w
	if joined ~= "" then
		break
	end
end
print(total, joined)
return nil
