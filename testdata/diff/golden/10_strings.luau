-- Compiled with sloptor v2.3.3
local name = "world"
local n = 3
local plain = `no substitution`
local multi = `a {name} b {n} c`
local tableInTemplate = `{({ 1, 2 })}`
local concat = "x" .. "y" .. "z"
print(plain, multi, tableInTemplate, concat)
return nil
