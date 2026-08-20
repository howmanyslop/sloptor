-- Compiled with sloptor v2.3.2
local i = 0
i += 1
i -= 1
i += 1
local neg = -i
local pos = 5
local negExpr = -(pos + 1)
local notted = not false
print(i, neg, negExpr, notted)
local _original = i
i += 1
local after = _original
i += 1
local before = i
print(after, before, i)
return nil
