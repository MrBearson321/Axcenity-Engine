-- Each player's Kills and Deaths: the leaderboard's columns (a
-- leaderstats Folder in the Player) and their saved totals. On a server,
-- SaveService keeps each player's data by their UserId, so the totals
-- come back the next time they join.
local SaveService = game:GetService("SaveService")

local COLUMNS = { "Kills", "Deaths" }

local stats = {}

function stats.start(player)
	local folder = Instance.new("Folder")
	folder.Name = "leaderstats"
	for _, name in COLUMNS do
		local value = Instance.new("IntValue")
		value.Name = name
		value.Value = SaveService:GetPlayerData(player, name) or 0
		value.Parent = folder
	end
	folder.Parent = player
end

function stats.add(player, name, amount)
	local value = player.leaderstats[name]
	value.Value += amount
	SaveService:SetPlayerData(player, name, value.Value)
end

-- What Save keeps and Load puts back, as player data.
function stats.save(player)
	for _, name in COLUMNS do
		SaveService:SetPlayerData(player, "Saved" .. name, player.leaderstats[name].Value)
	end
end

function stats.load(player)
	if SaveService:GetPlayerData(player, "SavedKills") == nil then
		return false
	end
	for _, name in COLUMNS do
		local value = SaveService:GetPlayerData(player, "Saved" .. name) or 0
		player.leaderstats[name].Value = value
		SaveService:SetPlayerData(player, name, value)
	end
	return true
end

return stats
