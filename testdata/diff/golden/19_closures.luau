-- Compiled with sloptor v2.3.0
local fns = {}
for i = 0, 2 do
	fns[i + 1] = function()
		return i
	end
end
local gns = {}
do
	local _k = 0
	local _shouldIncrement = false
	while true do
		local k = _k
		if _shouldIncrement then
			k += 1
		else
			_shouldIncrement = true
		end
		if not (k ~= 3) then
			break
		end
		gns[k + 1] = function()
			return k
		end
		_k = k
	end
end
local calls = 0
do
	local _j = 0
	while true do
		local j = _j
		if not (j ~= 5) then
			break
		end
		j = j + 2
		calls += 1
		_j = j
	end
end
print(fns[1](), fns[2](), fns[3](), gns[1](), gns[2](), gns[3](), calls)
return nil
