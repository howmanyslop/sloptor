-- Compiled with sloptor v2.3.2
-- Basic enum test
local Fruits
do
	local _inverse = {}
	Fruits = setmetatable({}, {
		__index = _inverse,
	})
	Fruits.Apple = 0
	_inverse[0] = "Apple"
	Fruits.Orange = 1
	_inverse[1] = "Orange"
	Fruits.Pear = 2
	_inverse[2] = "Pear"
	Fruits["$"] = 3
	_inverse[3] = "$"
end
-- Numeric enum test
local Breads
do
	local _inverse = {}
	Breads = setmetatable({}, {
		__index = _inverse,
	})
	Breads.White = 5
	_inverse[5] = "White"
	Breads.Wheat = 6
	_inverse[6] = "Wheat"
	Breads.Pumpernickel = 7
	_inverse[7] = "Pumpernickel"
	Breads["$"] = 8
	_inverse[8] = "$"
end
-- String enums
local Soups = {
	Tomato = "TOMATO",
	ChickenNoodle = "CHICKENNOODLE",
	Dumpling = "DUMPLING",
	["$"] = "DOLLARS",
}
local EnumWithEscapedQuote = {
	Quote = '"',
}
local function getValue()
	return 123
end
local WithInitializer
do
	local _inverse = {}
	WithInitializer = setmetatable({}, {
		__index = _inverse,
	})
	local _value = getValue()
	WithInitializer.value = _value
	_inverse[_value] = "value"
end
return function()
	it("should expose enums by number", function()
		expect(Fruits[0]).to.equal("Apple")
		expect(Fruits.Orange).to.equal(1)
		expect(Fruits[2]).to.equal("Pear")
	end)
	it("should support overriding indices", function()
		expect(Breads[5]).to.equal("White")
		expect(Breads.Wheat).to.equal(6)
		expect(Breads[0]).never.to.be.ok()
	end)
	it("should support for string indices", function()
		expect(Soups.Tomato).to.equal("TOMATO")
		expect(Soups.ChickenNoodle).to.equal("CHICKENNOODLE")
		expect(Soups.Dumpling).to.equal("DUMPLING")
		expect(Soups["$"]).to.equal("DOLLARS")
	end)
	it("should support numeric const enums", function()
		expect(0).to.equal(0)
		expect(1).to.equal(1)
		expect(2).to.equal(2)
		expect(3).to.equal(3)
		expect(4).to.equal(4)
		expect(5).to.equal(5)
	end)
	it("should support string const enums", function()
		expect("BEAR").to.equal("BEAR")
		expect("DOG").to.equal("DOG")
		expect("SNAKE").to.equal("SNAKE")
		expect("SCARAB").to.equal("SCARAB")
	end)
	it("should support members with initializers", function()
		expect(WithInitializer.value).to.equal(123)
	end)
	local Animal2
	it("should support hoisted enums", function()
		expect(Animal2.Bear).to.equal("BEAR")
		expect(Animal2.Dog).to.equal("DOG")
		expect(Animal2.Snake).to.equal("SNAKE")
	end)
	Animal2 = {
		Bear = "BEAR",
		Dog = "DOG",
		Snake = "SNAKE",
	}
	it("should support enums with escaped quotes", function()
		expect(EnumWithEscapedQuote.Quote).to.equal('"')
	end)
	it("should support const enums with escaped quotes", function()
		expect('"').to.equal('"')
	end)
	it("should support computed keys", function()
		local constKeyIndex = 4
		local constKeyValue = 7
		local ConstComputedKey
		do
			local _inverse = {}
			ConstComputedKey = setmetatable({}, {
				__index = _inverse,
			})
			ConstComputedKey[constKeyIndex] = constKeyValue
			_inverse[constKeyValue] = constKeyIndex
		end
		expect(ConstComputedKey[4]).to.equal(7)
		local computedKeyIndex = 1
		local ComputedKey
		do
			local _inverse = {}
			ComputedKey = setmetatable({}, {
				__index = _inverse,
			})
			local _original = computedKeyIndex
			computedKeyIndex += 1
			ComputedKey[_original] = computedKeyIndex
			_inverse[computedKeyIndex] = _original
		end
		expect(ComputedKey[1]).to.equal(2)
		expect(computedKeyIndex).to.equal(2)
		local computedValueIndex = 3
		local PrereqInValue
		do
			local _inverse = {}
			PrereqInValue = setmetatable({}, {
				__index = _inverse,
			})
			local _computedValueIndex = computedValueIndex
			computedValueIndex = 6
			PrereqInValue[_computedValueIndex] = computedValueIndex
			_inverse[computedValueIndex] = _computedValueIndex
		end
		expect(PrereqInValue[3]).to.equal(6)
		expect(computedValueIndex).to.equal(6)
		local obj = {
			prop = 8,
		}
		local PrereqValueReference
		do
			local _inverse = {}
			PrereqValueReference = setmetatable({}, {
				__index = _inverse,
			})
			local _prop = obj.prop
			obj.prop = 6
			local _value = (obj.prop)
			PrereqValueReference[_prop] = _value
			_inverse[_value] = _prop
		end
		expect(PrereqValueReference[8]).to.equal(6)
		expect(obj.prop).to.equal(6)
	end)
end
