-- Game tests for the course: the character, the door, coins and the
-- checkpoint. Run them from the editor's Tests panel (View > Panels >
-- Tests), or with `cargo run -p axcenity-testing -- <this project's folder>`.
describe("Course", function()
	local player, character

	beforeEach(function()
		Test.step(0.2)
		player = game.Players:GetPlayers()[1]
		character = player.Character
	end)

	local function standAt(position)
		character:PivotTo(CFrame.new(position + Vector3.new(0, 3, 0)))
		Test.step(0.3)
	end

	it("spawns a Humanoid character on the spawn", function()
		expect(character:FindFirstChildWhichIsA("Humanoid")).toBeTruthy()
		local spawn = Test.find("Workspace.Spawn")
		local at = character:GetPivot().Position
		expect((at - spawn.Position).Magnitude).toBeLessThan(6)
	end)

	it("opens the door with its prompt", function()
		local panel = Test.find("Workspace.Door.Panel")
		standAt(Vector3.new(0, 0, -6))
		Test.press("Interact")
		Test.step(1)
		expect(panel.Position.X).toBeGreaterThan(5)
		Test.step(5)
		expect(panel.Position.X).toBeNear(0, 0.01)
	end)

	it("collects a coin and keeps it in the save slot", function()
		local menu = Test.find("Workspace.GameMenu")
		standAt(Test.find("Workspace.Coins.Coin1").Position)
		expect(player.leaderstats.Coins.Value).toBe(1)
		Test.wait(function()
			return Test.find("Workspace.Hud.Coins").Text == "Coins: 1"
		end)
		Test.press("Menu")
		Test.click(menu.Pause.Save)
		Test.wait(function()
			return menu.Pause.Status.Text == "Saved to slot 1"
		end)
		Test.click(menu.Pause.Resume)
		standAt(Test.find("Workspace.Coins.Coin2").Position)
		expect(player.leaderstats.Coins.Value).toBe(2)
		Test.press("Menu")
		Test.click(menu.Pause.Load)
		Test.wait(function()
			return player.leaderstats.Coins.Value == 1
		end)
		expect(Test.find("Workspace.Coins.Coin2").Transparency).toBe(0)
		expect(Test.find("Workspace.Coins.Coin1").Transparency).toBe(1)
	end)

	it("puts you back on the checkpoint after a fall", function()
		local pad = Test.find("Workspace.Pad")
		standAt(pad.Position)
		standAt(Vector3.new(0, 0, -37))
		Test.wait(function()
			local at = character:GetPivot().Position
			return (at - pad.Position).Magnitude < 5
		end)
	end)

	it("puts you back on the spawn after a fall before the checkpoint", function()
		standAt(Vector3.new(0, 0, -37))
		Test.wait(function()
			local now = player.Character
			return now and (now:GetPivot().Position - Test.find("Workspace.Spawn").Position).Magnitude < 6
		end)
	end)
end)
