-- Game tests for the character: run them from the editor's Tests panel,
-- or with `cargo run -p axcenity-testing -- examples/hello-character`.
describe("Character", function()
	local body

	beforeEach(function()
		body = Test.find("Workspace.Character.Body")
	end)

	it("walks more than 5 studs while W is held for a second", function()
		local start = body.Position
		Test.press("W", 1)
		local moved = (body.Position - start).Magnitude
		expect(moved).toBeGreaterThan(5)
	end)

	it("stands still on the floor without input", function()
		local start = body.Position
		Test.step(0.5)
		expect(body.Position).toBeNear(start, 0.01)
	end)

	it("runs while Shift is held", function()
		local controller = Test.find("Workspace.Character.ThirdPersonController")
		Test.press("LeftShift")
		expect(controller.Movement).toBe("Run")
	end)
end)
