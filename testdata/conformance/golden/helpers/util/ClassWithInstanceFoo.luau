-- Compiled with sloptor v2.3.2
local ClassWithInstanceFoo
do
	ClassWithInstanceFoo = setmetatable({}, {
		__tostring = function()
			return "ClassWithInstanceFoo"
		end,
	})
	ClassWithInstanceFoo.__index = ClassWithInstanceFoo
	function ClassWithInstanceFoo.new(...)
		local self = setmetatable({}, ClassWithInstanceFoo)
		return self:constructor(...) or self
	end
	function ClassWithInstanceFoo:constructor()
	end
	function ClassWithInstanceFoo:foo()
	end
end
return {
	ClassWithInstanceFoo = ClassWithInstanceFoo,
}
