-- Compiled with sloptor v3.0.0-beta.1-beta.1
return function()
	it("should support using the Roblox API", function()
		expect(game:FindFirstChild("Workspace")).to.equal(game:GetService("Workspace"))
	end)
	it("should support calling Roblox API methods with element expressions", function()
		expect(game.FindFirstChild(game, "Workspace")).to.equal(game.GetService(game, "Workspace"))
	end)
end
