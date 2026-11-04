--!strict
--!nolint
type function Partial2(T: type)
	if not T:is("table") then
		return T
	end
	local result = types.copy(T)
	for key, prop in T:properties() do
		if prop.read then
			result:setreadproperty(key, types.optional(prop.read))
		end
		if prop.write then
			result:setwriteproperty(key, types.optional(prop.write))
		end
	end
	return result
end
export type Rec<T = number> = { read a: T, write b: string?, [string]: boolean }
type P = Partial2<{ a: number, b: string }>
local p: P = {}
local keys: keyof<{ a: number, b: string }> = "a"
local idx: index<{ a: number }, "a"> = 1
local arr: { read number } = { 1, 2 }
local f: <T, U...>(T, U...) -> (T, U...) = nil :: any
local function g(...: number): ...number return ... end
export type Fn<T = number, U... = ...string> = (T, U...) -> ()
return nil
