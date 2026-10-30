-- Game tests for the arena: teams, the sword, knockouts and respawning.
-- Playing with others is tested by the engine's own tests (a host and two
-- players); run these from the editor's Tests panel (View > Panels >
-- Tests), or with `cargo run -p axcenity-testing -- <this project's folder>`.
describe("Arena", function()
	local player

	beforeEach(function()
		Test.step(0.2)
		player = game.Players:GetPlayers()[1]
	end)

	it("puts you on a team, on its spawn, with a sword in hand", function()
		expect(player.Team.Name).toBe("Red")
		local spawn = Test.find("Workspace.RedSpawn")
		local at = player.Character:GetPivot().Position
		expect((at - spawn.Position).Magnitude).toBeLessThan(8)
		local inventory = player.Character.Inventory
		expect(inventory.Equipped.Name).toBe("Sword")
	end)

	it("shows your stats on the leaderboard", function()
		expect(player.leaderstats.Kills.Value).toBe(0)
		expect(player.leaderstats.Deaths.Value).toBe(0)
	end)

	it("hits the training dummy in reach", function()
		local dummy = Test.find("Workspace.TrainingDummy")
		player.Character:PivotTo(CFrame.new(dummy.Body.Position + Vector3.new(0, 0, 4)))
		Test.step(0.2)
		Test.press("Primary")
		Test.step(0.1)
		expect(dummy.Health.Health).toBe(75)
	end)

	it("misses the dummy out of reach", function()
		Test.press("Primary")
		Test.step(0.1)
		expect(Test.find("Workspace.TrainingDummy.Health").Health).toBe(100)
	end)

	-- A new character has a ForceField for 3 seconds (the SpawnLocation's
	-- Duration), and takes no damage while it does.
	local function unshield()
		local field = player.Character:FindFirstChildWhichIsA("ForceField")
		if field then
			field:Destroy()
		end
	end

	it("counts a knockout and respawns you with a new sword", function()
		unshield()
		local first = player.Character
		first.Humanoid:TakeDamage(100)
		Test.wait(function()
			return player.Character ~= first and player.Character ~= nil
		end, 6)
		expect(player.leaderstats.Deaths.Value).toBe(1)
		Test.step(0.2)
		expect(player.Character.Inventory.Equipped.Name).toBe("Sword")
	end)

	it("shows your health on the HUD", function()
		unshield()
		player.Character.Humanoid:TakeDamage(30)
		Test.step(0.1)
		expect(Test.find("Workspace.Hud.Health").Text).toBe("Health 70")
	end)
end)
