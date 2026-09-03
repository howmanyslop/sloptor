-- Compiled with sloptor v2.4.0
local n1 = {}
do
	local _container = n1
	local a = "a"
	_container.a = a
end
local n2 = {}
do
	local _container = n2
	local n3 = {}
	do
		local _container_1 = n3
		local a = "a"
		_container_1.a = a
	end
	_container.n3 = n3
end
local foo = {}
do
	local _container = foo
	local foo = {}
	do
		local _container_1 = foo
		local foo = {}
		do
			local _container_2 = foo
			local bar = "bar"
			_container_2.bar = bar
		end
		_container_1.foo = foo
	end
	_container.foo = foo
end
return function()
	it("should support namespaces", function()
		expect(n1.a).to.equal("a")
	end)
	it("should support nested namespaces", function()
		expect(n2.n3.a).to.equal("a")
	end)
	it("should support shadowed namespaces", function()
		expect(foo.foo.foo.bar).to.equal("bar")
	end)
end
