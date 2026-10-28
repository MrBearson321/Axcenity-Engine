-- Phase 48: levels. world.json is the start level, Meadow; Castle and
-- Dungeon are in levels/. Walk into the purple portal to go to the Castle,
-- where you arrive at the Gate, and through the Castle's portal to come
-- back Home. ReplicatedStorage (the PortalsTaken count) and the player
-- data (Coins) stay the same from level to level. Each arrival saves to
-- slot 1, which the menu's Continue loads.
local LevelService = game:GetService("LevelService")
local SaveService = game:GetService("SaveService")
local Players = game:GetService("Players")
local RunService = game:GetService("RunService")
local taken = game.ReplicatedStorage.Shared.PortalsTaken

if SaveService:GetPlayerData("Coins") == nil then
	SaveService:SetPlayerData("Coins", 3)
end

-- Where each level's portal goes, and where you arrive.
local destinations = {
	Meadow = { "Castle", "Gate" },
	Castle = { "Meadow", "Home" },
}

RunService.Heartbeat:Connect(function()
	local portal = workspace:FindFirstChild("Portal")
	local player = Players:GetPlayers()[1]
	local character = player and player.Character
	if not (portal and character) then
		return
	end
	for _, part in workspace:GetPartsInPart(portal) do
		if part:IsDescendantOf(character) then
			local to = destinations[LevelService.CurrentLevel]
			taken.Value += 1
			LevelService:LoadLevel(to[1], to[2])
			return
		end
	end
end)

LevelService.LevelLoaded:Connect(function(name)
	print("Arrived in " .. name .. " with " .. SaveService:GetPlayerData("Coins") .. " coins")
	SaveService:SaveSlot(1)
end)
