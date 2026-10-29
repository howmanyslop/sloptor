-- Compiled with sloptor v3.0.0-beta.1
local name = "world"
local n = 3
local plain = `no substitution`
local multi = `a {name} b {n} c`
local tableInTemplate = `{({ 1, 2 })}`
local concat = "x" .. "y" .. "z"
print(plain, multi, tableInTemplate, concat)
return nil
