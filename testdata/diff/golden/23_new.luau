-- Compiled with sloptor v2.3.0
local filled = table.create(4)
filled[1] = 7
local tags = {
	[1] = true,
	[2] = true,
}
local scores = {}
local cache = setmetatable({}, {
	__mode = "k",
})
print(filled[1], tags, scores, cache)
local part = Instance.new("Part")
print(part.Name)
return nil
