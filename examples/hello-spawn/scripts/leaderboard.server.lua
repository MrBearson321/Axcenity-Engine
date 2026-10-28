-- Phase 47: players, spawns, teams and the leaderboard. Play spawns your
-- character on your team's SpawnLocation (Red: Blue isn't AutoAssignable).
-- Each player gets a `leaderstats` Folder with Coins, which the default
-- leaderboard shows at the top right (Tab hides it). Walk into the coin to
-- collect it, and into the lava to die and respawn RespawnTime seconds later.
local Players = game:GetService("Players")
local RunService = game:GetService("RunService")
local coin = workspace:WaitForChild("Coin")

Players.PlayerAdded:Connect(function(player)
	local leaderstats = Instance.new("Folder")
	leaderstats.Name = "leaderstats"
	local coins = Instance.new("IntValue")
	coins.Name = "Coins"
	coins.Parent = leaderstats
	leaderstats.Parent = player
	local team = if player.Team then player.Team.Name else "no"
	print(player.Name .. " joined, on the " .. team .. " team")
	player.CharacterAdded:Connect(function(character)
		print(player.Name .. " spawned")
	end)
end)

-- Where the coin goes next, in turn.
local spots = { Vector3.new(-8, 1.5, 10), Vector3.new(8, 1.5, 12), Vector3.new(0, 1.5, 4) }
local nextSpot = 2

RunService.Heartbeat:Connect(function()
	for _, player in Players:GetPlayers() do
		local character = player.Character
		local stats = player:FindFirstChild("leaderstats")
		if character and character.Parent and stats then
			local feet = character:GetPivot().Position
			local across = Vector3.new(feet.X - coin.Position.X, 0, feet.Z - coin.Position.Z)
			if across.Magnitude < 2.5 then
				stats.Coins.Value += 1
				coin.Position = spots[nextSpot]
				nextSpot = nextSpot % #spots + 1
			end
		end
	end
end)
