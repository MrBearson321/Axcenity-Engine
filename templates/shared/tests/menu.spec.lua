-- Game tests for the pause and settings menus. Run them from the editor's
-- Tests panel (View > Panels > Tests), or with
-- `cargo run -p axcenity-testing -- <this project's folder>`.
describe("Pause menu", function()
	local menu

	beforeEach(function()
		menu = Test.find("Workspace.GameMenu")
		Test.step()
	end)

	it("opens on Menu and closes on Resume", function()
		expect(menu.Pause.Visible).toBe(false)
		Test.press("Menu")
		expect(menu.Pause.Visible).toBe(true)
		Test.click(menu.Pause.Resume)
		expect(menu.Pause.Visible).toBe(false)
	end)

	it("holds the character still while it's open", function()
		Test.press("Menu")
		local character = game.Players:GetPlayers()[1].Character
		local body = character:FindFirstChildWhichIsA("BasePart", true)
		local start = body.Position
		Test.press("W", 0.5)
		expect((body.Position - start).Magnitude).toBeLessThan(0.1)
	end)

	it("rebinds Jump in Settings", function()
		Test.press("Menu")
		Test.click(menu.Pause.Settings)
		expect(menu.Settings.Visible).toBe(true)
		local jump = menu.Settings.Rows.Jump
		Test.click(jump.Binding)
		Test.press("K")
		expect(jump.Binding.Text).toBe("K")
	end)

	it("saves to the slot and loads it back", function()
		Test.press("Menu")
		Test.click(menu.Pause.Save)
		Test.wait(function()
			return menu.Pause.Status.Text == "Saved to slot 1"
		end)
		local probe = Instance.new("Part")
		probe.Name = "MadeAfterSaving"
		probe.Anchored = true
		probe.Parent = workspace
		Test.click(menu.Pause.Load)
		Test.wait(function()
			return menu.Pause.Status.Text == "Loaded slot 1"
		end)
		expect(workspace:FindFirstChild("MadeAfterSaving")).toBe(nil)
	end)
end)
