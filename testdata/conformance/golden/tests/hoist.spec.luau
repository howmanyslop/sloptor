-- Compiled with sloptor v2.3.2
return function()
	it("should support variable hoisting", function()
		local x
		local function test()
			expect(x).to.equal(1)
		end
		x = 1
		test()
	end)
	it("should hoist classes", function()
		local Foo
		local function test()
			local foo = Foo.new()
			expect(foo:bar()).to.equal("baz")
		end
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
			function Foo:bar()
				return "baz"
			end
		end
		test()
	end)
end
