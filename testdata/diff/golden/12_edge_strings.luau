-- Compiled with sloptor v2.3.2
local brackets = "contains ]] double brackets"
local bothQuotes = `has "double" and 'single'`
local newlines = "line1\nline2"
local tabs = "col1\tcol2"
local braces = `template with {"literal"} and \{braces\}`
local nested = `outer {`inner {1 + 1}`} end`
local unicode = "héllo wörld"
print(brackets, bothQuotes, newlines, tabs, braces, nested, unicode)
return nil
