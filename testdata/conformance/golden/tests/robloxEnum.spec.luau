-- Compiled with sloptor v2.3.3
return function()
	it("should permit the use of GetEnums()", function()
		for _, enumType in Enum:GetEnums() do
			for _1, _binding in enumType:GetEnumItems() do
				local name = _binding.Name
				expect(typeof(name)).to.equal("string")
			end
		end
	end)
	it("should permit the use of GetEnumItems()", function()
		local axisNames = {
			["X"] = true,
			["Y"] = true,
			["Z"] = true,
		}
		for _, _binding in Enum.Axis:GetEnumItems() do
			local name = _binding.Name
			expect(axisNames[name] ~= nil).to.equal(true)
		end
	end)
end
