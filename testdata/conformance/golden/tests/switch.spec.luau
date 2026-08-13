-- Compiled with sloptor v2.3.0
return function()
	it("should support switch statements with fall through", function()
		local function foo(s)
			repeat
				local _fallthrough = false
				if s == "a" then
					_fallthrough = true
				end
				if _fallthrough or s == "b" then
					return 1
				end
				if s == "c" then
					return 2
				end
				if s == "d" then
					if true then
						break
					end
					_fallthrough = true
				end
				if _fallthrough or s == "e" then
					return 4
				end
				return -1
			until true
			return 3
		end
		expect(foo("a")).to.equal(1)
		expect(foo("b")).to.equal(1)
		expect(foo("c")).to.equal(2)
		expect(foo("d")).to.equal(3)
		expect(foo("e")).to.equal(4)
		expect(foo("f")).to.equal(-1)
	end)
	it("should support switch statements without fall through", function()
		local function bar(s)
			repeat
				if s == "a" then
					return 1
				end
				if s == "b" then
					return 2
				end
				if s == "c" then
					return 3
				end
			until true
			return 4
		end
		expect(bar("a")).to.equal(1)
		expect(bar("b")).to.equal(2)
		expect(bar("c")).to.equal(3)
		expect(bar("d")).to.equal(4)
	end)
	it("should support switch statements with remaining empty conditions", function()
		local function bar(s)
			repeat
				local _fallthrough = false
				if s == "a" then
					return 1
				end
				if s == "b" then
					_fallthrough = true
				end
				if _fallthrough or s == "c" then
				end
			until true
			return 2
		end
		expect(bar("a")).to.equal(1)
		expect(bar("b")).to.equal(2)
		expect(bar("c")).to.equal(2)
		expect(bar("d")).to.equal(2)
	end)
	it("should support switch statements with context", function()
		local function bar(n)
			local x = 1
			repeat
				local _original = x
				x += 1
				if n == _original then
					return -1
				end
				local _original_1 = x
				x += 1
				if n == _original_1 then
					return -2
				end
			until true
			return 0
		end
		expect(bar(1)).to.equal(-1)
		expect(bar(2)).to.equal(-2)
		expect(bar(3)).to.equal(0)
	end)
	it("should support switch statements with fallthrough and context", function()
		local function bar(n)
			local x = 1
			repeat
				local _fallthrough = false
				local _original = x
				x += 1
				if n == _original then
					_fallthrough = true
				end
				if not _fallthrough then
					local _original_1 = x
					x += 1
					_fallthrough = n == _original_1
				end
				if _fallthrough then
					return -2
				end
			until true
			return 0
		end
		expect(bar(1)).to.equal(-2)
		expect(bar(2)).to.equal(-2)
		expect(bar(3)).to.equal(0)
	end)
	it("should support switch statements with preceding statements", function()
		local function bar(n)
			local x = 1
			n += 1
			repeat
				local _fallthrough = false
				local _original = x
				x += 1
				if n == _original then
					_fallthrough = true
				end
				if not _fallthrough then
					local _original_1 = x
					x += 1
					_fallthrough = n == _original_1
				end
				if _fallthrough then
					_fallthrough = true
				end
				if not _fallthrough then
					local _original_1 = x
					x += 1
					_fallthrough = n == _original_1
				end
				if _fallthrough then
					_fallthrough = true
				end
				if not _fallthrough then
					local _original_1 = x
					x += 1
					_fallthrough = n == _original_1
				end
				if _fallthrough then
					return -2
				end
			until true
			return 0
		end
		expect(bar(0)).to.equal(-2)
		expect(bar(1)).to.equal(-2)
		expect(bar(2)).to.equal(-2)
		expect(bar(3)).to.equal(-2)
		expect(bar(4)).to.equal(0)
	end)
	it("should support switch statements with a boolean value", function()
		local function foo(n)
			repeat
				if true == (n >= 10) then
					return 10
				end
				if true == (n >= 5) then
					return 5
				end
				if true == (n >= 0) then
					return 0
				end
			until true
		end
		expect(foo(10)).to.equal(10)
		expect(foo(7)).to.equal(5)
		expect(foo(5)).to.equal(5)
		expect(foo(3)).to.equal(0)
		expect(foo(0)).to.equal(0)
	end)
end
