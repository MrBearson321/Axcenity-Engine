-- Game tests for click to move, the enemies and the goal. Run them from
-- the editor's Tests panel (View > Panels > Tests), or with
-- `cargo run -p axcenity-testing -- <this project's folder>`.
describe("Top Down", function()
	local hero, agent

	beforeEach(function()
		Test.step(0.2)
		hero = Test.find("Workspace.Hero")
		agent = hero.NavigationAgent
	end)

	local function across(a, b)
		return Vector3.new(a.X - b.X, 0, a.Z - b.Z).Magnitude
	end

	it("sends the hero where the mouse clicks, under the top-down camera", function()
		-- The mouse is in the middle of the view, which looks down at the hero.
		Test.press("MouseButton1")
		Test.step(0.1)
		expect(across(agent.Destination, hero.Body.Position)).toBeLessThan(4)
		expect(Test.find("Workspace.MoveMarker").Transparency).toBeLessThan(1)
	end)

	it("walks round the walls to where it's sent", function()
		agent.Destination = Vector3.new(-30, 0, 0)
		Test.step(0.1)
		expect(agent.Status).toBe("Moving")
		Test.wait(function()
			return agent.Status == "Reached"
		end, 10)
		expect(across(hero.Body.Position, Vector3.new(-30, 0, 0))).toBeLessThan(1.5)
	end)

	it("is chased by a guard that sees it", function()
		local guard = Test.find("Workspace.Enemies.Guard1")
		hero:PivotTo(CFrame.new(0, 2.5, -8))
		agent.Destination = Vector3.new(0, 0, -8)
		local start = across(guard.Body.Position, hero.Body.Position)
		Test.wait(function()
			return guard.NavigationAgent.SeesPlayer
		end)
		Test.step(1)
		expect(across(guard.Body.Position, hero.Body.Position)).toBeLessThan(start - 3)
		expect(guard.Body.Color).toEqual(Color3.new(1, 0.1, 0.1))
	end)

	it("hurts when caught, and sends it back to the start at 0 health", function()
		local start = hero.Body.Position
		hero.Health:TakeDamage(95)
		local guard = Test.find("Workspace.Enemies.Guard1")
		hero:PivotTo(CFrame.new(guard.Body.Position + Vector3.new(0, 0.5, 2)))
		agent.Destination = Vector3.new(hero.Body.Position.X, 0, hero.Body.Position.Z)
		Test.wait(function()
			return across(hero.Body.Position, start) < 1
		end)
		expect(hero:GetAttribute("Status")).toBe("Caught! Back to the start.")
		expect(hero.Health.Health).toBe(100)
	end)

	it("counts a win at the goal and keeps it in the save slot", function()
		local menu = Test.find("Workspace.GameMenu")
		local player = game.Players:GetPlayers()[1]
		hero:PivotTo(CFrame.new(0, 2.5, -40))
		Test.wait(function()
			return player:GetAttribute("Wins") == 1
		end)
		expect(Test.find("Workspace.Hud.Wins").Text).toBe("Wins: 1")
		Test.press("Menu")
		Test.click(menu.Pause.Save)
		Test.wait(function()
			return menu.Pause.Status.Text == "Saved to slot 1"
		end)
		game:GetService("SaveService"):SetPlayerData("Wins", 0)
		Test.click(menu.Pause.Load)
		Test.wait(function()
			return player:GetAttribute("Wins") == 1
		end)
	end)
end)
