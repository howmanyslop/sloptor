-- Compiled with sloptor v2.3.1
local function describe(n)
	repeat
		local _fallthrough = false
		if n == 0 then
			return "zero"
		end
		if n == 1 then
			_fallthrough = true
		end
		if _fallthrough or n == 2 then
			return "small"
		end
		if n == 3 then
			local label = "three"
			return label
		end
		return "big"
	until true
end
local mode = "idle"
repeat
	if mode == "idle" then
		mode = "running"
		break
	end
	mode = "unknown"
until true
print(describe(0), describe(2), describe(3), describe(9), mode)
return nil
