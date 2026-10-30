-- START HERE: the course. Walk into a coin to collect it, step on the green
-- pad to make it your checkpoint, and if you fall off you're back on it
-- (or at the spawn, before you've reached it). Door.server.blocks.json
-- opens the door when its prompt is triggered.
--
-- Coins are player data, so Save in the pause menu keeps them in the slot
-- with the coins you've picked up (they're see-through), and Load brings
-- both back.
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local RunService = game:GetService("RunService")
local SaveService = game:GetService("SaveService")

local PICK_UP_DISTANCE = 3
local FALL_HEIGHT = -15

-- The leaderboard (Tab) and the HUD show leaderstats.Coins.
local function showCoins()
	for _, player in Players:GetPlayers() do
		local stats = player:FindFirstChild("leaderstats")
		if stats then
			stats.Coins.Value = SaveService:GetPlayerData("Coins") or 0
		end
	end
end

Players.PlayerAdded:Connect(function(player)
	local stats = Instance.new("Folder")
	stats.Name = "leaderstats"
	local coins = Instance.new("IntValue")
	coins.Name = "Coins"
	coins.Parent = stats
	stats.Parent = player
	showCoins()
end)
ReplicatedStorage:WaitForChild("GameLoaded").Event:Connect(showCoins)

local function collect(coin)
	coin.Transparency = 1
	SaveService:SetPlayerData("Coins", (SaveService:GetPlayerData("Coins") or 0) + 1)
	showCoins()
end

RunService.Heartbeat:Connect(function()
	for _, player in Players:GetPlayers() do
		local character = player.Character
		if not character or not character.Parent then
			continue
		end
		local at = character:GetPivot().Position
		for _, coin in workspace.Coins:GetChildren() do
			if coin.Transparency < 1 and (coin.Position - at).Magnitude < PICK_UP_DISTANCE then
				collect(coin)
			end
		end
		local pad = workspace.Pad
		local onPad = math.abs(at.X - pad.Position.X) < pad.Size.X / 2
			and math.abs(at.Z - pad.Position.Z) < pad.Size.Z / 2
			and at.Y < pad.Position.Y + 5
		if onPad and SaveService:GetCheckpoint() ~= workspace.Checkpoint then
			SaveService:SetCheckpoint(workspace.Checkpoint)
			print("Checkpoint!")
		end
		if at.Y < FALL_HEIGHT then
			if SaveService:GetCheckpoint() then
				SaveService:RestoreCheckpoint()
				showCoins()
			else
				player:LoadCharacter()
			end
		end
	end
end)
