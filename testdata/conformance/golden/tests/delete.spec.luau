-- Compiled with sloptor v2.3.3
return function()
	describe("should work for objects", function()
		it("should delete from the object with property acccess", function()
			local myTrolly = {
				apples = 5,
				bananas = 6,
				pears = 7,
			}
			myTrolly.apples = nil
			-- prettier-ignore
			myTrolly.pears = nil
			expect(myTrolly.apples).to.equal(nil)
			expect(myTrolly.pears).to.equal(nil)
		end)
		it("should delete with optional property access", function()
			local myTrolly = {
				apples = 5,
				bananas = 6,
				pears = 7,
			}
			local _result = myTrolly
			if _result ~= nil then
				_result.apples = nil
			end
			-- prettier-ignore
			local _result_1 = myTrolly
			if _result_1 ~= nil then
				_result_1.pears = nil
			end
			expect(myTrolly.apples).to.equal(nil)
			expect(myTrolly.pears).to.equal(nil)
		end)
		it("should delete from the object with element access", function()
			local myTrolly = {
				apples = 5,
				bananas = 6,
				pears = 7,
			}
			myTrolly.apples = nil
			-- prettier-ignore
			myTrolly.pears = nil
			expect(myTrolly.apples).to.equal(nil)
			expect(myTrolly.pears).to.equal(nil)
		end)
		it("should delete with optional element access", function()
			local myTrolly = {
				apples = 5,
				bananas = 6,
				pears = 7,
			}
			local _result = myTrolly
			if _result ~= nil then
				_result.apples = nil
			end
			-- prettier-ignore
			local _result_1 = myTrolly
			if _result_1 ~= nil then
				_result_1.pears = nil
			end
			expect(myTrolly.apples).to.equal(nil)
			expect(myTrolly.pears).to.equal(nil)
		end)
		it("should not delete other properties", function()
			local myTrolly = {
				apples = 5,
				bananas = 6,
			}
			myTrolly.apples = nil
			local _result = myTrolly
			if _result ~= nil then
				_result.apples = nil
			end
			local _result_1 = myTrolly
			if _result_1 ~= nil then
				_result_1.apples = nil
			end
			local _result_2 = myTrolly
			if _result_2 ~= nil then
				_result_2.apples = nil
			end
			expect(myTrolly.bananas).to.equal(6)
		end)
		it("should be able to do objects inside objects", function()
			local myTrollies = {
				c = {
					apples = 5,
					bananas = 6,
				},
			}
			myTrollies.c.apples = nil
			expect(myTrollies.c.apples).to.equal(nil)
		end)
	end)
	-- we use this function as otherwise TypeScript will evaluate the type of the variable and determine if it's possible to perform optional chaining on it.
	local function getFruitsOrUndefined()
		return nil
	end
	describe("should return true", function()
		it("should return true for property access", function()
			local myTrolly = {
				apples = 5,
				bananas = 6,
				pears = 7,
			}
			myTrolly.apples = nil
			expect(true).to.equal(true)
			-- prettier-ignore
			myTrolly.pears = nil
			expect(true).to.equal(true)
		end)
		it("should return true for optional property access", function()
			local myTrolly = getFruitsOrUndefined()
			local _result = myTrolly
			if _result ~= nil then
				_result.apples = nil
			end
			expect(true).to.equal(true)
			-- prettier-ignore
			local _result_1 = myTrolly
			if _result_1 ~= nil then
				_result_1.pears = nil
			end
			expect(true).to.equal(true)
		end)
		it("should return true for element access", function()
			local myTrolly = {
				apples = 5,
				bananas = 6,
				pears = 7,
			}
			myTrolly.apples = nil
			expect(true).to.equal(true)
			-- prettier-ignore
			myTrolly.apples = nil
			expect(true).to.equal(true)
		end)
		it("should return true for optional element access", function()
			local myTrolly = getFruitsOrUndefined()
			local _result = myTrolly
			if _result ~= nil then
				_result.apples = nil
			end
			expect(true).to.equal(true)
			-- prettier-ignore
			local _result_1 = myTrolly
			if _result_1 ~= nil then
				_result_1.pears = nil
			end
			expect(true).to.equal(true)
		end)
	end)
end
