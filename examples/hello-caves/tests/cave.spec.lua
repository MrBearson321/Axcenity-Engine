-- Game tests for the cave: run them from the editor's Tests panel, or with
-- `cargo run -p axcenity-testing -- examples/hello-caves`.
describe("Cave", function()
	local body

	beforeEach(function()
		body = Test.find("Workspace.Character.Body")
	end)

	it("walks through the hole in the hillside into the cave", function()
		Test.press("W", 3)
		-- Past the mouth, under the hill, on the cave floor.
		expect(body.Position.Z).toBeLessThan(-18)
		expect(body.Position.Y).toBeLessThan(6)
		expect(body.Position.Y).toBeGreaterThan(1)
	end)
end)
