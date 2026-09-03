-- Compiled with sloptor v2.4.0
return function()
	it("should support typeOf", function()
		expect(typeof({})).to.equal("table")
		expect(typeof(nil)).to.equal("nil")
		expect(typeof(1)).to.equal("number")
		expect(typeof("foo")).to.equal("string")
		expect(typeof(true)).to.equal("boolean")
		expect(typeof({})).to.equal("table")
		local _vector2 = Vector2.new()
		expect(typeof(_vector2)).to.equal("Vector2")
	end)
	it("should support typeIs", function()
		expect(type({}) == "table").to.equal(true)
		expect(type(nil) == "nil").to.equal(true)
		expect(type(1) == "number").to.equal(true)
		expect(type("foo") == "string").to.equal(true)
		expect(type(true) == "boolean").to.equal(true)
		expect(type({}) == "table").to.equal(true)
		local _vector2 = Vector2.new()
		expect(typeof(_vector2) == "Vector2").to.equal(true)
		-- make sure this doesn't produce invalid Lua
		local _ = type("Hello, world!") == "string"
		local _1 = type("Hello, world!") == "string"
		local _2 = not (type("Hello, world!") == "string")
		local _3 = not (type("Hello, world!") == "string")
		local lel = not (type("Hello, world!") == "string")
		if type("Hello, world!") == "string" then
		end
		local _4 = type("s") == "table"
	end)
end
