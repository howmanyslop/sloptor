-- Compiled with sloptor v2.3.1
local ClassWithStaticFoo
do
	ClassWithStaticFoo = setmetatable({}, {
		__tostring = function()
			return "ClassWithStaticFoo"
		end,
	})
	ClassWithStaticFoo.__index = ClassWithStaticFoo
	function ClassWithStaticFoo.new(...)
		local self = setmetatable({}, ClassWithStaticFoo)
		return self:constructor(...) or self
	end
	function ClassWithStaticFoo:constructor()
	end
	function ClassWithStaticFoo:foo()
	end
end
return {
	ClassWithStaticFoo = ClassWithStaticFoo,
}
