-- Compiled with sloptor v2.3.1
local TS = require(script.Parent.Parent.include.RuntimeLib)
return function()
	it("should support instanceof", function()
		local Foo
		do
			Foo = setmetatable({}, {
				__tostring = function()
					return "Foo"
				end,
			})
			Foo.__index = Foo
			function Foo.new(...)
				local self = setmetatable({}, Foo)
				return self:constructor(...) or self
			end
			function Foo:constructor()
			end
		end
		local Bar
		do
			local super = Foo
			Bar = setmetatable({}, {
				__tostring = function()
					return "Bar"
				end,
				__index = super,
			})
			Bar.__index = Bar
			function Bar.new(...)
				local self = setmetatable({}, Bar)
				return self:constructor(...) or self
			end
			function Bar:constructor(...)
				super.constructor(self, ...)
			end
		end
		local Baz
		do
			Baz = setmetatable({}, {
				__tostring = function()
					return "Baz"
				end,
			})
			Baz.__index = Baz
			function Baz.new(...)
				local self = setmetatable({}, Baz)
				return self:constructor(...) or self
			end
			function Baz:constructor()
			end
		end
		local foo = Foo.new()
		expect(TS.instanceof(foo, Foo)).to.equal(true)
		expect(TS.instanceof(foo, Bar)).to.equal(false)
		expect(TS.instanceof(foo, Baz)).to.equal(false)
		local bar = Bar.new()
		expect(TS.instanceof(bar, Foo)).to.equal(true)
		expect(TS.instanceof(bar, Bar)).to.equal(true)
		expect(TS.instanceof(bar, Baz)).to.equal(false)
		local baz = Baz.new()
		expect(TS.instanceof(baz, Foo)).to.equal(false)
		expect(TS.instanceof(baz, Bar)).to.equal(false)
		expect(TS.instanceof(baz, Baz)).to.equal(true)
	end)
end
