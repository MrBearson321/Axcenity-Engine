-- Every Model tagged "Target" (see its Tags in Properties) goes see-through
-- when its Health reaches 0, counts as a hit, and stands up again after
-- STAND_UP_AFTER seconds. Copy a target, or tag a Model of your own with a
-- Health inside, to add more. The count is player data, so the save slot
-- keeps it.
local CollectionService = game:GetService("CollectionService")
local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local SaveService = game:GetService("SaveService")

local STAND_UP_AFTER = 3

-- The HUD shows the player's TargetsDown attribute.
local function show()
	local down = SaveService:GetPlayerData("TargetsDown") or 0
	for _, player in Players:GetPlayers() do
		player:SetAttribute("TargetsDown", down)
	end
end
Players.PlayerAdded:Connect(show)
ReplicatedStorage:WaitForChild("GameLoaded").Event:Connect(show)

for _, target in CollectionService:GetTagged("Target") do
	local health = target.Health
	health.Died:Connect(function()
		SaveService:SetPlayerData("TargetsDown", (SaveService:GetPlayerData("TargetsDown") or 0) + 1)
		show()
		target.Board.Transparency = 0.8
		task.wait(STAND_UP_AFTER)
		health.Health = health.MaxHealth
		target.Board.Transparency = 0
	end)
end
show()
